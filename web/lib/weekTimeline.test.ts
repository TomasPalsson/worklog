import { describe, expect, it } from "bun:test";
import { assignLanes, blockSpan, hourHeight, timelineRange } from "./weekTimeline";

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
