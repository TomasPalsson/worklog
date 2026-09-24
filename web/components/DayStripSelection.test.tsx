// Click-and-drag time-range selection in the lanes view: dragging opens
// the split popover listing every project active in the dragged range,
// Save sends that window + shares, Escape cancels the in-progress drag,
// and a saved allocation's bracket renders with a working reset.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { StripBlock, StripGap } from "@/lib/dayStrip";
import type { ProjectActivity, SavedAllocation } from "@/lib/types";

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

let DayStrip: (props: {
  day: string;
  blocks: StripBlock[];
  gaps: StripGap[];
  activity?: ProjectActivity[];
  allocations?: SavedAllocation[];
}) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./DayStrip");
  DayStrip = mod.DayStrip;
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  allocateCalls.length = 0;
  deleteCalls.length = 0;
  refreshImpl.mockClear();
});

const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

function makeBlock(overrides: Partial<StripBlock> & { id: number }): StripBlock {
  return {
    started_at: at(9, 0),
    ended_at: at(10, 0),
    project_path: "/x/alpha",
    is_personal: false,
    confidence: "high",
    description: null,
    ...overrides,
  };
}

const blocks: StripBlock[] = [
  makeBlock({ id: 1, started_at: at(9, 0), ended_at: at(10, 0), project_path: "/x/alpha" }),
  makeBlock({ id: 2, started_at: at(10, 0), ended_at: at(11, 0), project_path: "/x/beta" }),
];

// Window is computeTrackWindow's floor/ceil-to-hour of the blocks: 9:00–11:00
// (2h = 120 minutes). alpha is active 9:00–10:30, beta 9:30–11:00, so a drag
// over the middle third (9:30–10:30) picks up both.
const activity: ProjectActivity[] = [
  { project: "alpha", spans: [{ started_at: at(9, 0), ended_at: at(10, 30) }] },
  { project: "beta", spans: [{ started_at: at(9, 30), ended_at: at(11, 0) }] },
];

/** Stub the lanes container's geometry (happy-dom does no real layout) and
 * expand the strip, returning the container to drag on. */
function renderExpandedLanes(allocations: SavedAllocation[] = []) {
  render(<DayStrip day="2026-09-23" blocks={blocks} gaps={[]} activity={activity} allocations={allocations} />);
  fireEvent.click(screen.getByRole("button", { name: /expand/i }));
  const lanes = document.querySelector(".day-strip-lanes") as HTMLElement;
  const rect = (left: number, width: number) =>
    ({ left, width, top: 0, bottom: 0, right: left + width, height: 0, x: left, y: 0, toJSON() {} }) as DOMRect;
  // Like the real layout: a 160px name column, then the 1000px tracks.
  lanes.getBoundingClientRect = () => rect(0, LABEL_W + 1000);
  lanes.querySelectorAll<HTMLElement>(".day-strip-lane-track").forEach((t) => {
    t.getBoundingClientRect = () => rect(LABEL_W, 1000);
  });
  return lanes;
}

const LABEL_W = 160;
/** clientX for a fraction (0..1) along the mocked 1000px-wide tracks. */
const clientXFor = (frac: number) => LABEL_W + Math.round(frac * 1000);

describe("DayStrip selection drag", () => {
  it("dragging over empty track opens the split popover listing every active project", () => {
    const lanes = renderExpandedLanes();
    fireEvent.pointerDown(lanes, { button: 0, clientX: clientXFor(0.25), pointerId: 1 });
    fireEvent.pointerMove(lanes, { clientX: clientXFor(0.75), pointerId: 1 });
    expect(screen.getByText(/09:30–10:30/)).toBeTruthy();
    fireEvent.pointerUp(lanes, { clientX: clientXFor(0.75), pointerId: 1 });

    const dialog = screen.getByRole("dialog", { name: /split this overlap/i });
    expect(dialog).toBeTruthy();
    expect(screen.getByRole("button", { name: "Give all to alpha" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Give all to beta" })).toBeTruthy();
  });

  it("Save sends the dragged window and the chosen split", () => {
    const lanes = renderExpandedLanes();
    fireEvent.pointerDown(lanes, { button: 0, clientX: clientXFor(0.25), pointerId: 1 });
    fireEvent.pointerMove(lanes, { clientX: clientXFor(0.75), pointerId: 1 });
    fireEvent.pointerUp(lanes, { clientX: clientXFor(0.75), pointerId: 1 });

    fireEvent.click(screen.getByRole("button", { name: "Give all to alpha" }));
    expect(allocateCalls).toEqual([
      ["2026-09-23", new Date(at(9, 30)).toISOString(), new Date(at(10, 30)).toISOString(), { alpha: 1 }],
    ]);
  });

  it("Escape cancels the in-progress drag before it's released", () => {
    const lanes = renderExpandedLanes();
    fireEvent.pointerDown(lanes, { button: 0, clientX: clientXFor(0.25), pointerId: 1 });
    fireEvent.pointerMove(lanes, { clientX: clientXFor(0.75), pointerId: 1 });
    expect(screen.getByText(/09:30–10:30/)).toBeTruthy();

    fireEvent.keyDown(lanes, { key: "Escape" });
    expect(screen.queryByText(/09:30–10:30/)).toBeNull();

    fireEvent.pointerUp(lanes, { clientX: clientXFor(0.75), pointerId: 1 });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("DayStrip saved allocation bracket", () => {
  it("renders a bracket with the split ratio and a working reset", () => {
    const allocation: SavedAllocation = {
      started_at: at(9, 30),
      ended_at: at(10, 30),
      shares: { alpha: 0.9, beta: 0.1 },
    };
    renderExpandedLanes([allocation]);
    expect(screen.getByText("you set 90/10")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "reset" }));
    expect(deleteCalls).toEqual([
      ["2026-09-23", new Date(at(9, 30)).toISOString(), new Date(at(10, 30)).toISOString()],
    ]);
  });
});
