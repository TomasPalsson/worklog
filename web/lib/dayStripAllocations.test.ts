import { describe, expect, it } from "bun:test";
import type { TrackWindow } from "./dayStrip";
import { buildAllocationBands, formatSharesRatio } from "./dayStripAllocations";
import type { SavedAllocation } from "./types";

const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

const window: TrackWindow = {
  startMs: new Date(at(9, 0)).getTime(),
  endMs: new Date(at(15, 0)).getTime(),
};

describe("formatSharesRatio", () => {
  it("formats percentages largest first", () => {
    expect(formatSharesRatio({ alpha: 0.9, beta: 0.1 })).toBe("you set 90/10");
    expect(formatSharesRatio({ alpha: 0.3, beta: 0.7 })).toBe("you set 70/30");
  });
});

describe("buildAllocationBands", () => {
  it("positions a band by percentage with its ratio label", () => {
    const allocation: SavedAllocation = {
      started_at: at(10, 0),
      ended_at: at(11, 0),
      shares: { alpha: 0.6, beta: 0.4 },
    };
    const bands = buildAllocationBands([allocation], window);
    expect(bands).toHaveLength(1);
    expect(bands[0].ratioLabel).toBe("you set 60/40");
    expect(bands[0].leftPct).toBeGreaterThan(0);
    expect(bands[0].widthPct).toBeGreaterThan(0);
  });

  it("drops an allocation outside the track window", () => {
    const allocation: SavedAllocation = {
      started_at: at(20, 0),
      ended_at: at(21, 0),
      shares: { alpha: 1 },
    };
    expect(buildAllocationBands([allocation], window)).toHaveLength(0);
  });
});
