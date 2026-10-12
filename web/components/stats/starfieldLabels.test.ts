import { describe, expect, it } from "bun:test";
import { placeLabels } from "./starfieldLabels";

const size = { w: 400, h: 300 };
const c = (id: number, x: number, y: number, priority: number, over = {}) => ({ id, x, y, w: 100, h: 16, priority, ...over });
const ids = (r: { id: number }[]) => r.map((p) => p.id).sort();

describe("placeLabels", () => {
  it("overlapping: only the higher priority is placed, whatever the input order (catches input-order greedy)", () => {
    expect(ids(placeLabels([c(1, 200, 100, 1), c(2, 210, 105, 9)], 6, size))).toEqual([2]);
  });

  it("non-overlapping candidates are all placed at their own x,y, highest priority first", () => {
    const r = placeLabels([c(1, 100, 50, 1), c(2, 300, 250, 2)], 6, size);
    expect(r).toEqual([
      { id: 2, x: 300, y: 250 },
      { id: 1, x: 100, y: 50 },
    ]);
  });

  it("padding: a 5px gap is skipped, exactly pad (6px) is placed (catches ignoring pad and >= vs >)", () => {
    expect(ids(placeLabels([c(1, 200, 100, 2), c(2, 200, 100 + 16 + 5, 1)], 6, size))).toEqual([1]);
    expect(ids(placeLabels([c(1, 200, 100, 2), c(2, 200, 100 + 16 + 6, 1)], 6, size))).toEqual([1, 2]);
  });

  it("a candidate fully off-canvas is skipped; one clipped by an edge is skipped too", () => {
    expect(placeLabels([c(1, 900, 100, 1)], 6, size)).toEqual([]);
    expect(placeLabels([c(1, 40, 100, 1)], 6, size)).toEqual([]);
    expect(placeLabels([c(1, 50, 100, 1)], 6, size)).toHaveLength(1);
    expect(placeLabels([c(1, 200, 7, 1)], 6, size)).toEqual([]);
  });

  it("a skipped lower-priority label does not block a later one that fits", () => {
    expect(ids(placeLabels([c(1, 200, 100, 3), c(2, 205, 100, 2), c(3, 330, 100, 1)], 6, size))).toEqual([1, 3]);
  });

  it("force places a label over a collision but still not off-canvas, and others avoid it", () => {
    const r = placeLabels([c(1, 200, 100, 9), c(2, 205, 100, 1, { force: true }), c(3, 900, 100, 1, { force: true })], 6, size);
    expect(ids(r)).toEqual([2]);
    const avoid = placeLabels([c(1, 205, 100, 9), c(2, 200, 100, 1, { force: true })], 6, size);
    expect(ids(avoid)).toEqual([2]);
  });

  it("empty input gives empty output", () => {
    expect(placeLabels([], 6, size)).toEqual([]);
  });
});
