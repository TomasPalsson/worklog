import { describe, expect, it } from "bun:test";
import { assignLanes, blockSpan, hourHeight, mergeRuns, runActivities, timelineRange } from "./weekTimeline";

const at = (h: number, m = 0) => new Date(2026, 8, 28, h, m).toISOString();
const blk = (s: [number, number?], e: [number, number?], is_personal = false) => ({
  started_at: at(s[0], s[1] ?? 0),
  ended_at: at(e[0], e[1] ?? 0),
  is_personal,
});

describe("timelineRange", () => {
  it("defaults to 08:00-18:00 with no work blocks", () => {
    expect(timelineRange([])).toEqual({ startHour: 8, endHour: 18 });
    expect(timelineRange([blk([3], [4], true)])).toEqual({ startHour: 8, endHour: 18 });
  });
  it("never narrows below 08:00-18:00", () => {
    expect(timelineRange([blk([10], [11])])).toEqual({ startHour: 8, endHour: 18 });
  });
  it("floors the earliest start and ceils the latest end", () => {
    expect(timelineRange([blk([6, 40], [7]), blk([17], [19, 10])])).toEqual({ startHour: 6, endHour: 20 });
  });
});

describe("hourHeight", () => {
  it("clamps between 36 and 56", () => {
    expect(hourHeight(10)).toBe(56);
    expect(hourHeight(16)).toBe(40);
    expect(hourHeight(24)).toBe(36);
  });
});

describe("blockSpan", () => {
  const range = { startHour: 8, endHour: 18 };
  it("measures from the range start", () => {
    expect(blockSpan(blk([9, 39], [10, 40]), range)).toEqual({ top: 99, length: 61 });
  });
  it("clips to the range edges and drops blocks fully outside", () => {
    expect(blockSpan(blk([7], [9]), range)).toEqual({ top: 0, length: 60 });
    expect(blockSpan(blk([17], [19]), range)).toEqual({ top: 540, length: 60 });
    expect(blockSpan(blk([5], [6]), range)).toBeNull();
  });
});

describe("assignLanes", () => {
  it("keeps non-overlapping items in one full-width lane", () => {
    expect(assignLanes([{ start: 0, end: 60 }, { start: 60, end: 90 }])).toEqual([
      { lane: 0, lanes: 1 },
      { lane: 0, lanes: 1 },
    ]);
  });
  it("splits overlapping items and reuses a freed lane", () => {
    const r = assignLanes([
      { start: 0, end: 60 },
      { start: 30, end: 90 },
      { start: 60, end: 120 },
    ]);
    expect(r).toEqual([
      { lane: 0, lanes: 2 },
      { lane: 1, lanes: 2 },
      { lane: 0, lanes: 2 },
    ]);
  });
  it("sizes each cluster separately and keeps input order", () => {
    const r = assignLanes([
      { start: 200, end: 260 },
      { start: 0, end: 60 },
      { start: 30, end: 90 },
    ]);
    expect(r).toEqual([
      { lane: 0, lanes: 1 },
      { lane: 0, lanes: 2 },
      { lane: 1, lanes: 2 },
    ]);
  });
});

describe("mergeRuns", () => {
  const mb = (s: [number, number], e: [number, number], over: Record<string, unknown> = {}) => {
    const b = blk(s, e);
    return {
      ...b,
      jira_issue: "A-1" as string | null,
      tempo_worklog_id: null as string | null,
      dirty: false,
      description: null as string | null,
      duration_seconds: (Date.parse(b.ended_at) - Date.parse(b.started_at)) / 1000,
      ...over,
    };
  };
  it("merges same-ticket blocks across a 5-min gap", () => {
    const r = mergeRuns([mb([9, 0], [9, 20]), mb([9, 25], [9, 40])]);
    expect(r.length).toBe(1);
    expect(r[0].count).toBe(2);
    expect(r[0].ended_at).toBe(at(9, 40));
  });
  it("does not merge across a 15-min gap", () => {
    expect(mergeRuns([mb([9, 0], [9, 20]), mb([9, 35], [9, 40])]).length).toBe(2);
  });
  it("does not merge different tickets or synced vs unsynced", () => {
    expect(mergeRuns([mb([9, 0], [9, 20]), mb([9, 20], [9, 40], { jira_issue: "A-2" })]).length).toBe(2);
    expect(mergeRuns([mb([9, 0], [9, 20]), mb([9, 20], [9, 40], { tempo_worklog_id: "1" })]).length).toBe(2);
  });
  it("merges overlapping same-key blocks", () => {
    const r = mergeRuns([mb([9, 0], [10, 0]), mb([9, 10], [9, 30])]);
    expect(r.length).toBe(1);
    expect(r[0].ended_at).toBe(at(10, 0));
  });
  it("workSeconds sums durations, not span; description from longest", () => {
    const r = mergeRuns([mb([9, 0], [9, 10], { description: "short" }), mb([9, 15], [9, 45], { description: "long" })]);
    expect(r[0].workSeconds).toBe(600 + 1800);
    expect(r[0].description).toBe("long");
  });
  it("personal merges separately from work", () => {
    const r = mergeRuns([mb([9, 0], [9, 20]), mb([9, 20], [9, 30], { is_personal: true }), mb([9, 30], [9, 40], { is_personal: true })]);
    expect(r.map((x) => [x.is_personal, x.count])).toEqual([[false, 1], [true, 2]]);
  });
  describe("runActivities", () => {
    it("dedupes, sums, sorts by seconds desc and drops empty descriptions", () => {
      const r = mergeRuns([
        mb([9, 0], [9, 10], { description: "a" }),
        mb([9, 10], [9, 40], { description: "b" }),
        mb([9, 40], [9, 50], { description: "a" }),
        mb([9, 50], [9, 55], { description: "" }),
        mb([9, 55], [10, 0], { description: null }),
      ])[0];
      expect(runActivities(r)).toEqual([
        { description: "b", seconds: 1800 },
        { description: "a", seconds: 1200 },
      ]);
    });
  });
});
