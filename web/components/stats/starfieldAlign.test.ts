import { describe, expect, it } from "bun:test";
import type { MapPoint, TopicsReport } from "@/lib/topics_contract";
import { buildScene, initialCamera, project, type Scene } from "./starfieldScene";

// world offset (x, y, z) from the centre -> sx/sy/sz in [0,1] (world = (s - 0.5) * 200)
const pt = (block_id: number, x: number, y: number, z: number): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx: 0.5 + x / 200,
  sy: 0.5 + y / 200,
  sz: 0.5 + z / 200,
  topic_id: 0,
  day: "2026-10-01",
  seconds: 3600,
  label: "l",
  ticket: null,
  novelty: 0.5,
  oddball: false,
});
const report = (points: MapPoint[]) =>
  ({ topics: [{ id: 0, name: "t0", seconds: 3600 }], points, first_times: [] }) as unknown as TopicsReport;

// a flat cloud stretched along the INPUT z axis, a little along y: aligned, its long side must become x
const pts = [-60, -30, 0, 30, 60].flatMap((z, i) => [pt(10 + i, 0, (i % 2) * 8, z), pt(20 + i, 4, 0, z)]);
const raw = buildScene(report(pts), { align: false });
const aligned = buildScene(report(pts));
const varAlong = (s: Scene, a: number) => {
  const m = s.stars.reduce((acc, st) => acc + st.pos[a], 0) / s.stars.length;
  return s.stars.reduce((acc, st) => acc + (st.pos[a] - m) ** 2, 0);
};
const dist = (s: Scene, i: number, j: number) =>
  Math.hypot(s.stars[i].pos[0] - s.stars[j].pos[0], s.stars[i].pos[1] - s.stars[j].pos[1], s.stars[i].pos[2] - s.stars[j].pos[2]);

describe("view alignment", () => {
  it("puts the largest spread on x, then y, then z (catches no rotation or the wrong order)", () => {
    expect(varAlong(raw, 2)).toBeGreaterThan(varAlong(raw, 0)); // the fixture really is long on input z
    expect(varAlong(aligned, 0)).toBeGreaterThan(varAlong(aligned, 1));
    expect(varAlong(aligned, 1)).toBeGreaterThanOrEqual(varAlong(aligned, 2) - 1e-9);
  });

  it("is rigid: every pairwise distance is kept (catches scaling or skew)", () => {
    for (let i = 0; i < raw.stars.length; i++)
      for (let j = i + 1; j < raw.stars.length; j++) expect(dist(aligned, i, j)).toBeCloseTo(dist(raw, i, j), 6);
  });

  it("is deterministic", () => {
    expect(buildScene(report(pts)).stars.map((s) => s.pos)).toEqual(aligned.stars.map((s) => s.pos));
  });

  it("a wide canvas frames the long side: no star past 75% of the half-width, the farthest close to it", () => {
    const cam = initialCamera(aligned, 16 / 9);
    const reach = Math.max(...aligned.stars.map((s) => Math.abs(project(cam, s.pos, 1600, 900)!.x - 800) / 800));
    expect(reach).toBeLessThanOrEqual(0.75 + 1e-6);
    expect(reach).toBeGreaterThan(0.6); // catches the old sphere fit that left the wide canvas mostly empty
  });
});
