import { describe, expect, it } from "bun:test";
import type { MapPoint, TopicsReport } from "@/lib/topics_contract";
import { buildScene, starIndexOf } from "./starfieldScene";

const pt = (block_id: number, sx: number): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx,
  sy: 0.5,
  sz: 0.5,
  topic_id: 0,
  day: "2026-10-01",
  seconds: 3600,
  label: `block ${block_id}`,
  ticket: null,
  novelty: 0.5,
  oddball: false,
});
const report = (points: MapPoint[]) =>
  ({ topics: [{ id: 0, name: "t0", seconds: 3600 }], points, first_times: [] }) as unknown as TopicsReport;

describe("a pinned star across a report refresh", () => {
  const before = buildScene(report([pt(5, 0.3), pt(7, 0.4), pt(9, 0.6)]));
  // block 5 was deleted between refreshes: every later star shifts down one index
  const after = buildScene(report([pt(7, 0.4), pt(9, 0.6)]));

  it("is found again by block id, not by its old array index (catches showing another block)", () => {
    const pinned = before.stars[2].id; // block 9 at index 2
    const i = starIndexOf(after, pinned);
    expect(i).toBe(1);
    expect(after.stars[i!].label).toBe("block 9");
    expect(after.stars[2]).toBeUndefined(); // the old index now points nowhere
  });

  it("is dropped when its block is gone", () => {
    expect(starIndexOf(after, before.stars[0].id)).toBeNull();
  });

  it("no pin stays no pin", () => {
    expect(starIndexOf(after, null)).toBeNull();
  });

  it("every star carries its block id", () => {
    expect(after.stars.map((s) => s.id)).toEqual([7, 9]);
  });
});
