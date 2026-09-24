// Behaviour of the overlap split popover (opened by clicking a band on
// the day strip): "Give all to X" sends {X:1}, the two-way slider sends
// the split it shows, "Reset to automatic" deletes the saved allocation,
// and a 3+-project form only enables Save once its percents sum to 100.

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
    expect(screen.getByText(/vitinn-infra 70%/)).toBeTruthy();
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

  it("disables Save until the percents sum to 100", () => {
    render(<OverlapPopover day={DAY} overlap={threeProjectOverlap()} onClose={() => {}} />);
    const inputs = document.querySelectorAll('input[type="number"]');
    expect(inputs.length).toBe(3);
    const saveBtn = screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

    fireEvent.change(inputs[0], { target: { value: "50" } });
    fireEvent.change(inputs[1], { target: { value: "30" } });
    fireEvent.change(inputs[2], { target: { value: "10" } }); // sums to 90
    expect(saveBtn.disabled).toBe(true);

    fireEvent.change(inputs[2], { target: { value: "20" } }); // sums to 100
    expect(saveBtn.disabled).toBe(false);
    fireEvent.click(saveBtn);
    expect(allocateCalls).toEqual([
      [DAY, "2026-09-24T10:00:00Z", "2026-09-24T11:00:00Z", { a: 0.5, b: 0.3, c: 0.2 }],
    ]);
  });
});
