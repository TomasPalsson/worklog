import { describe, expect, it } from "bun:test";
import type { TrackWindow } from "./dayStrip";
import {
  findExactAllocation,
  projectsActiveInRange,
  selectionFromFractions,
  snapToMinute,
} from "./dayStripSelection";
import type { ProjectActivity, SavedAllocation } from "./types";

const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

const window: TrackWindow = {
  startMs: new Date(at(9, 0)).getTime(),
  endMs: new Date(at(17, 0)).getTime(), // 8h = 480 minutes
};

describe("snapToMinute", () => {
  it("rounds to the nearest whole minute", () => {
    expect(snapToMinute(new Date(at(9, 0)).getTime() + 20_000)).toBe(new Date(at(9, 0)).getTime());
    expect(snapToMinute(new Date(at(9, 0)).getTime() + 40_000)).toBe(new Date(at(9, 1)).getTime());
  });
});

describe("selectionFromFractions", () => {
  it("builds a selection regardless of drag direction, snapped to minutes", () => {
    // 10:45 is (10.75-9)/8 = 0.21875 of the way through; 17:41... use round figures.
    const startFrac = (new Date(at(10, 45)).getTime() - window.startMs) / (window.endMs - window.startMs);
    const endFrac = (new Date(at(12, 15)).getTime() - window.startMs) / (window.endMs - window.startMs);
    const forward = selectionFromFractions(startFrac, endFrac, window);
    const backward = selectionFromFractions(endFrac, startFrac, window);
    expect(forward).not.toBeNull();
    expect(forward).toEqual(backward);
    expect(forward!.startMs).toBe(new Date(at(10, 45)).getTime());
    expect(forward!.endMs).toBe(new Date(at(12, 15)).getTime());
    expect(forward!.label).toBe("10:45–12:15 · 1h 30m");
  });

  it("clamps to the window's bounds", () => {
    const sel = selectionFromFractions(-0.5, 1.5, window);
    expect(sel!.startMs).toBe(window.startMs);
    expect(sel!.endMs).toBe(window.endMs);
  });

  it("is null for a selection under a minute (a click, not a drag)", () => {
    expect(selectionFromFractions(0.5, 0.5, window)).toBeNull();
    expect(selectionFromFractions(0.5, 0.5001, window)).toBeNull();
  });
});

describe("projectsActiveInRange", () => {
  const activity: ProjectActivity[] = [
    { project: "alpha", spans: [{ started_at: at(10, 0), ended_at: at(11, 0) }] },
    { project: "beta", spans: [{ started_at: at(13, 0), ended_at: at(14, 0) }] },
  ];

  it("returns projects whose spans intersect the range", () => {
    const startMs = new Date(at(10, 30)).getTime();
    const endMs = new Date(at(13, 30)).getTime();
    expect(projectsActiveInRange(activity, startMs, endMs)).toEqual(["alpha", "beta"]);
  });

  it("excludes a project with no overlap at all", () => {
    const startMs = new Date(at(11, 30)).getTime();
    const endMs = new Date(at(12, 30)).getTime();
    expect(projectsActiveInRange(activity, startMs, endMs)).toEqual([]);
  });
});

describe("findExactAllocation", () => {
  const allocations: SavedAllocation[] = [
    { started_at: at(10, 0), ended_at: at(11, 0), shares: { alpha: 0.9, beta: 0.1 } },
  ];

  it("returns the shares of an exact window match", () => {
    const startMs = new Date(at(10, 0)).getTime();
    const endMs = new Date(at(11, 0)).getTime();
    expect(findExactAllocation(allocations, startMs, endMs)).toEqual({ alpha: 0.9, beta: 0.1 });
  });

  it("is null for a window that doesn't match exactly", () => {
    const startMs = new Date(at(10, 5)).getTime();
    const endMs = new Date(at(11, 0)).getTime();
    expect(findExactAllocation(allocations, startMs, endMs)).toBeNull();
  });
});
