// Behaviour of the overlap split popover (opened by clicking a band on
// the day strip): "Give all to X" sends {X:1}, the two-way slider sends
// the split it shows, "Reset to automatic" deletes the saved allocation,
// and with 3+ projects moving one slider rebalances the others.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Overlap } from "@/lib/types";

const refreshImpl = mock(() => {});
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshImpl }),
}));

const allocateCalls: Array<[string, string, string, Record<string, number>]> = [];
const deleteCalls: Array<[string, string, string]> = [];
mock.module("@/app/actions-overlaps", () => ({
  allocateOverlap: mock(
    async (day: string, s: string, e: string, shares: Record<string, number>) => {
      allocateCalls.push([day, s, e, shares]);
      return { ok: true as const, data: undefined };
    },
  ),
  resetOverlapAllocation: mock(async (day: string, s: string, e: string) => {
    deleteCalls.push([day, s, e]);
    return { ok: true as const, data: undefined };
  }),
}));

let OverlapPopover: (props: {
  day: string;
  overlap: Overlap;
  onClose: () => void;
  initialShares?: Record<string, number> | null;
}) => React.JSX.Element;

beforeAll(async () => {
  const mod = await import("./OverlapPopover");
  OverlapPopover = mod.OverlapPopover;
});

afterEach(() => {
  cleanup();
  allocateCalls.length = 0;
  deleteCalls.length = 0;
  refreshImpl.mockClear();
});

const DAY = "2026-09-24";

function twoProjectOverlap(allocation: Overlap["allocation"] = null): Overlap {
  return {
    started_at: "2026-09-24T10:00:00Z",
    ended_at: "2026-09-24T11:00:00Z",
    minutes: 60,
    projects: [
      { project: "vitinn-infra", human_events: 10, background_events: 2 },
      { project: "lyfjastofnun", human_events: 3, background_events: 8 },
    ],
    allocation,
  };
}

describe("OverlapPopover — Give all", () => {
  it("sends the whole window to the clicked project", () => {
    render(<OverlapPopover day={DAY} overlap={twoProjectOverlap()} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Give all to vitinn-infra" }));
    expect(allocateCalls).toEqual([
      [DAY, "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", { "vitinn-infra": 1 }],
    ]);
  });
});

describe("OverlapPopover — two-way slider", () => {
  it("moving the slider to 70% and saving sends {A:0.7,B:0.3}", () => {
    render(<OverlapPopover day={DAY} overlap={twoProjectOverlap()} onClose={() => {}} />);
    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    fireEvent.change(slider, { target: { value: "70" } });
    expect(screen.getByText("70% · 42m")).toBeTruthy();
    expect(screen.getByText("30% · 18m")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(allocateCalls).toEqual([
      [
        DAY,
        "2026-09-24T10:00:00Z",
        "2026-09-24T11:00:00Z",
        { "vitinn-infra": 0.7, lyfjastofnun: 0.3 },
      ],
    ]);
  });
});

describe("OverlapPopover — cancel", () => {
  it("calls onClose without saving or resetting anything", () => {
    const onClose = mock(() => {});
    render(<OverlapPopover day={DAY} overlap={twoProjectOverlap()} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(allocateCalls).toEqual([]);
    expect(deleteCalls).toEqual([]);
  });
});

describe("OverlapPopover — reset", () => {
  it("shows a reset link only when an allocation is saved, and it deletes", () => {
    const { rerender } = render(
      <OverlapPopover day={DAY} overlap={twoProjectOverlap()} onClose={() => {}} />,
    );
    expect(screen.queryByText("Reset to automatic")).toBeNull();

    rerender(
      <OverlapPopover
        day={DAY}
        overlap={twoProjectOverlap({ shares: { "vitinn-infra": 0.5, lyfjastofnun: 0.5 } })}
        onClose={() => {}}
      />,
    );
    fireEvent.click(screen.getByText("Reset to automatic"));
    expect(deleteCalls).toEqual([[DAY, "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z"]]);
  });
});

describe("OverlapPopover — 3+ projects", () => {
  function threeProjectOverlap(): Overlap {
    return {
      started_at: "2026-09-24T10:00:00Z",
      ended_at: "2026-09-24T11:00:00Z",
      minutes: 60,
      projects: [
        { project: "a", human_events: 1, background_events: 0 },
        { project: "b", human_events: 1, background_events: 0 },
        { project: "c", human_events: 1, background_events: 0 },
      ],
      allocation: null,
    };
  }

  it("moving one slider moves the others so the total stays 100", () => {
    render(<OverlapPopover day={DAY} overlap={threeProjectOverlap()} onClose={() => {}} />);
    const sliders = document.querySelectorAll('input[type="range"]');
    expect(sliders.length).toBe(3);
    expect(document.querySelectorAll('input[type="number"]').length).toBe(0);

    fireEvent.change(sliders[0], { target: { value: "50" } });
    expect((sliders[1] as HTMLInputElement).value).toBe("25");
    expect((sliders[2] as HTMLInputElement).value).toBe("25");

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(allocateCalls).toEqual([
      [DAY, "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", { a: 0.5, b: 0.25, c: 0.25 }],
    ]);
  });
});

describe("OverlapPopover — zero shares", () => {
  it("leaves 0% projects out of the save (the daemon rejects a 0 share)", () => {
    render(<OverlapPopover day={DAY} overlap={twoProjectOverlap()} onClose={() => {}} />);
    const slider = document.querySelector('input[type="range"]') as HTMLInputElement;
    fireEvent.change(slider, { target: { value: "100" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(allocateCalls).toEqual([
      [DAY, "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", { "vitinn-infra": 1 }],
    ]);
  });
});

describe("OverlapPopover — starting split", () => {
  it("starts the sliders at the given current split when nothing is saved", () => {
    render(
      <OverlapPopover
        day={DAY}
        overlap={twoProjectOverlap()}
        initialShares={{ "vitinn-infra": 0.8, lyfjastofnun: 0.2 }}
        onClose={() => {}}
      />,
    );
    const sliders = document.querySelectorAll('input[type="range"]');
    expect((sliders[0] as HTMLInputElement).value).toBe("80");
    expect((sliders[1] as HTMLInputElement).value).toBe("20");
  });
});
