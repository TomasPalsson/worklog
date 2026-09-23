import { describe, expect, it } from "bun:test";
import { isLunchGap } from "./format";
import {
  buildLegend,
  buildSegments,
  computeTrackWindow,
  hourTicks,
  projectHue,
  type StripBlock,
  type StripGap,
} from "./dayStrip";

// Local-time constructor, same trick as format.test.ts: build a Date from
// local components and round-trip through toISOString so the test is
// self-consistent regardless of the machine's TZ.
const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

describe("computeTrackWindow", () => {
  it("floors the first block's start and ceils the last block's end to the hour", () => {
    const blocks: StripBlock[] = [
      { id: 1, started_at: at(9, 12), ended_at: at(10, 5), project_path: "/x/a", is_personal: false, confidence: "high" },
      { id: 2, started_at: at(10, 30), ended_at: at(11, 47), project_path: "/x/b", is_personal: false, confidence: "high" },
    ];
    const window = computeTrackWindow(blocks)!;
    expect(new Date(window.startMs).getHours()).toBe(9);
    expect(new Date(window.startMs).getMinutes()).toBe(0);
    expect(new Date(window.endMs).getHours()).toBe(12);
    expect(new Date(window.endMs).getMinutes()).toBe(0);
  });

  it("returns null for no blocks", () => {
    expect(computeTrackWindow([])).toBeNull();
  });
});

describe("hourTicks", () => {
  it("ticks every hour for a span of 10h or less", () => {
    const window = { startMs: new Date(at(9, 0)).getTime(), endMs: new Date(at(12, 0)).getTime() };
    const ticks = hourTicks(window);
    expect(ticks.map((t) => t.label)).toEqual(["09:00", "10:00", "11:00", "12:00"]);
  });

  it("ticks every 2h for a span over 10h", () => {
    const window = { startMs: new Date(at(8, 0)).getTime(), endMs: new Date(at(20, 0)).getTime() };
    const ticks = hourTicks(window);
    expect(ticks.map((t) => t.label)).toEqual([
      "08:00",
      "10:00",
      "12:00",
      "14:00",
      "16:00",
      "18:00",
      "20:00",
    ]);
  });

  it("first tick sits at 0% and last at 100%", () => {
    const window = { startMs: new Date(at(9, 0)).getTime(), endMs: new Date(at(11, 0)).getTime() };
    const ticks = hourTicks(window);
    expect(ticks[0].pct).toBe(0);
    expect(ticks[ticks.length - 1].pct).toBe(100);
  });
});

describe("projectHue", () => {
  it("is stable for the same key", () => {
    expect(projectHue("vitinn-infra")).toBe(projectHue("vitinn-infra"));
  });

  it("differs for two different keys", () => {
    expect(projectHue("vitinn-infra")).not.toBe(projectHue("sjukra"));
  });

  it("is always in [0, 360)", () => {
    expect(projectHue("x")).toBeGreaterThanOrEqual(0);
    expect(projectHue("x")).toBeLessThan(360);
  });
});

describe("buildSegments", () => {
  // 09:00–12:00 window; block A 09:00–10:00 (25% of 3h), gap 10:00–10:30
  // (lunch, 8.3%), block B 10:30–12:00 (41.6%).
  const window = { startMs: new Date(at(9, 0)).getTime(), endMs: new Date(at(12, 0)).getTime() };
  const blocks: StripBlock[] = [
    { id: 1, started_at: at(9, 0), ended_at: at(10, 0), project_path: "/x/vitinn-infra", is_personal: false, confidence: "high" },
    { id: 2, started_at: at(10, 30), ended_at: at(12, 0), project_path: "/x/sjukra", is_personal: false, confidence: "medium" },
  ];
  const gaps: StripGap[] = [{ started_at: at(10, 0), ended_at: at(10, 30), minutes: 30 }];

  it("computes left/width percentages for blocks and gaps in chronological order", () => {
    const segs = buildSegments(blocks, gaps, window);
    expect(segs.map((s) => s.kind)).toEqual(["block", "gap", "block"]);

    expect(segs[0].leftPct).toBeCloseTo(0, 5);
    expect(segs[0].widthPct).toBeCloseTo((60 / 180) * 100, 5); // 1h of 3h

    expect(segs[1].leftPct).toBeCloseTo((60 / 180) * 100, 5);
    expect(segs[1].widthPct).toBeCloseTo((30 / 180) * 100, 5);

    expect(segs[2].leftPct).toBeCloseTo((90 / 180) * 100, 5);
    expect(segs[2].widthPct).toBeCloseTo((90 / 180) * 100, 5);
  });

  it("marks the gap covering 11:30 as lunch, others as away", () => {
    const [, gapSeg] = buildSegments(blocks, gaps, window);
    expect(gapSeg.kind).toBe("gap");
    if (gapSeg.kind === "gap") {
      // This gap is 10:00-10:30, not covering 11:30 lunch.
      expect(gapSeg.isLunch).toBe(false);
      expect(gapSeg.label).toBe("Away");
    }
  });

  it("hides the gap label below the 6% width threshold", () => {
    const tinyGap: StripGap[] = [{ started_at: at(10, 0), ended_at: at(10, 2), minutes: 2 }];
    const [, gapSeg] = buildSegments(blocks, tinyGap, window);
    expect(gapSeg.kind).toBe("gap");
    if (gapSeg.kind === "gap") expect(gapSeg.showLabel).toBe(false);
  });

  it("uses --slate (no hue) for personal/unassigned blocks", () => {
    const personalBlocks: StripBlock[] = [
      { id: 3, started_at: at(9, 0), ended_at: at(10, 0), project_path: null, is_personal: true, confidence: "high" },
    ];
    const [seg] = buildSegments(personalBlocks, [], window);
    expect(seg.kind).toBe("block");
    if (seg.kind === "block") expect(seg.slate).toBe(true);
  });
});

describe("buildLegend", () => {
  const blocks: StripBlock[] = [
    { id: 1, started_at: at(9, 0), ended_at: at(10, 0), project_path: "/x/sjukra", is_personal: false, confidence: "high" }, // 1h
    { id: 2, started_at: at(10, 30), ended_at: at(12, 35), project_path: "/x/vitinn-infra", is_personal: false, confidence: "high" }, // 2h05m
  ];
  const gaps: StripGap[] = [{ started_at: at(10, 0), ended_at: at(10, 36), minutes: 36 }];

  it("sorts projects by total desc, then puts Away last", () => {
    const legend = buildLegend(blocks, gaps);
    expect(legend.map((e) => e.label)).toEqual(["vitinn-infra", "sjukra", "Away"]);
  });

  it("sums seconds per project and the away total from gap minutes", () => {
    const legend = buildLegend(blocks, gaps);
    const vitinn = legend.find((e) => e.label === "vitinn-infra")!;
    const away = legend.find((e) => e.away)!;
    expect(vitinn.totalSeconds).toBe(2 * 3600 + 5 * 60);
    expect(away.totalSeconds).toBe(36 * 60);
  });

  it("omits Away entirely when there are no gaps", () => {
    const legend = buildLegend(blocks, []);
    expect(legend.some((e) => e.away)).toBe(false);
  });
});

describe("isLunchGap (reused from lib/format)", () => {
  it("is true for a gap covering 11:30 local", () => {
    expect(isLunchGap({ started_at: at(11, 23), ended_at: at(11, 59) })).toBe(true);
  });

  it("is false for a gap that doesn't cover 11:30", () => {
    expect(isLunchGap({ started_at: at(14, 6), ended_at: at(14, 41) })).toBe(false);
  });
});
