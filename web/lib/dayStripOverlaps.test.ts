import { describe, expect, it } from "bun:test";
import type { TrackWindow } from "./dayStrip";
import {
  buildOverlapBands,
  giveAllShares,
  overlapLabel,
  overlapsInWindow,
  percentsSumToHundred,
  percentsToShares,
  selectionOverlap,
} from "./dayStripOverlaps";
import type { Overlap } from "./types";

const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

function overlap(overrides: Partial<Overlap> = {}): Overlap {
  return {
    started_at: at(10, 45),
    ended_at: at(13, 54),
    minutes: 189,
    projects: [
      { project: "vitinn-infra", human_events: 10, background_events: 2 },
      { project: "lyfjastofnun", human_events: 3, background_events: 8 },
    ],
    allocation: null,
    ...overrides,
  };
}

const window: TrackWindow = {
  startMs: new Date(at(9, 0)).getTime(),
  endMs: new Date(at(15, 0)).getTime(),
};

describe("overlapLabel", () => {
  it("formats the time range and duration", () => {
    expect(overlapLabel(overlap())).toBe("Overlap 10:45–13:54 · 3h 9m");
  });
});

describe("overlapsInWindow", () => {
  it("keeps overlaps fully inside the track window", () => {
    expect(overlapsInWindow([overlap()], window)).toHaveLength(1);
  });

  it("drops an overlap outside the window", () => {
    const outside = overlap({ started_at: at(20, 0), ended_at: at(21, 0) });
    expect(overlapsInWindow([outside], window)).toHaveLength(0);
  });
});

describe("buildOverlapBands", () => {
  it("positions a band by percentage and finds its lane range", () => {
    const bands = buildOverlapBands([overlap()], window, ["vitinn-infra", "lyfjastofnun", "other"]);
    expect(bands).toHaveLength(1);
    expect(bands[0].label).toBe("Overlap 10:45–13:54 · 3h 9m");
    expect(bands[0].laneRange).toEqual([0, 1]);
    expect(bands[0].leftPct).toBeGreaterThan(0);
    expect(bands[0].widthPct).toBeGreaterThan(0);
  });

  it("laneRange is null when none of the overlap's projects have a lane", () => {
    const bands = buildOverlapBands([overlap()], window, ["unrelated"]);
    expect(bands[0].laneRange).toBeNull();
  });
});

describe("giveAllShares", () => {
  it("sends the whole window to one project", () => {
    expect(giveAllShares("vitinn-infra")).toEqual({ "vitinn-infra": 1 });
  });
});

describe("percentsToShares / percentsSumToHundred", () => {
  it("converts a 70/30 split to fractions", () => {
    expect(percentsSumToHundred([70, 30])).toBe(true);
    expect(percentsToShares(["vitinn-infra", "lyfjastofnun"], [70, 30])).toEqual({
      "vitinn-infra": 0.7,
      lyfjastofnun: 0.3,
    });
  });

  it("rejects percents that don't sum to 100", () => {
    expect(percentsSumToHundred([70, 20])).toBe(false);
  });
});

describe("selectionOverlap", () => {
  it("builds an Overlap-shaped object from a selection, no existing shares", () => {
    const startMs = new Date(at(10, 45)).getTime();
    const endMs = new Date(at(12, 15)).getTime();
    const o = selectionOverlap(startMs, endMs, ["vitinn-infra", "lyfjastofnun"], null);
    expect(o.started_at).toBe(new Date(startMs).toISOString());
    expect(o.ended_at).toBe(new Date(endMs).toISOString());
    expect(o.minutes).toBe(90);
    expect(o.projects.map((p) => p.project)).toEqual(["vitinn-infra", "lyfjastofnun"]);
    expect(o.allocation).toBeNull();
  });

  it("prefills the allocation when existing shares are given", () => {
    const o = selectionOverlap(0, 60_000, ["vitinn-infra"], { "vitinn-infra": 1 });
    expect(o.allocation).toEqual({ shares: { "vitinn-infra": 1 } });
  });
});
