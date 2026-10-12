import { describe, expect, it } from "bun:test";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { buildScene, constellation, initialCamera, project, R, type Vec3 } from "./starfieldScene";

const topic = (id: number): Topic => ({
  id,
  name: `t${id}`,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds: 3600,
  blocks: 1,
  days_active: 1,
  first_day: "2026-10-01",
  last_day: "2026-10-01",
  top_tickets: [],
});
// world x = (sx - 0.5) * 200
const pt = (block_id: number, topic_id: number, x: number, y = 0, z = 0): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx: 0.5 + x / 200,
  sy: 0.5 + y / 200,
  sz: 0.5 + z / 200,
  topic_id,
  day: "2026-10-01",
  seconds: 3600,
  label: "l",
  ticket: null,
  novelty: 0.5,
  oddball: false,
});
const report = (points: MapPoint[], n = 2): TopicsReport => ({
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  naming: "off",
  topics: Array.from({ length: n }, (_, i) => topic(i)),
  points,
  bin: "day",
  river: [],
  days: [],
  day_similarity: [],
  first_times: [],
  oddballs: [],
  oddball_median: 0,
  embedded_blocks: points.length,
  skipped_blocks: 0,
});
const sorted = (e: [number, number][]) => e.map(([a, b]) => [Math.min(a, b), Math.max(a, b)]).sort((p, q) => p[0] - q[0] || p[1] - q[1]);

describe("initialCamera fit", () => {
  const axes: Vec3[] = [[R, 0, 0], [-R, 0, 0], [0, R, 0], [0, -R, 0], [0, 0, R], [0, 0, -R]];
  const scene = buildScene(report(axes.map((a, i) => pt(i, 0, a[0], a[1], a[2])), 1));

  it("16:9 landscape: every extreme lands inside the canvas, the vertical extremes at 70-80% of the half-height", () => {
    const cam = initialCamera(scene, 16 / 9);
    const ps = axes.map((a) => project(cam, a, 1600, 900));
    for (const p of ps) {
      expect(p).not.toBeNull();
      expect(p?.x).toBeGreaterThan(0);
      expect(p?.x).toBeLessThan(1600);
      expect(p?.y).toBeGreaterThan(0);
      expect(p?.y).toBeLessThan(900);
    }
    const top = Math.abs(450 - (ps[2]?.y ?? 0)) / 450;
    expect(top).toBeGreaterThan(0.7);
    expect(top).toBeLessThan(0.8);
  });

  it("portrait: the narrower horizontal FOV governs, so x extremes land at 70-80% of the half-width (catches using fovY only)", () => {
    const cam = initialCamera(scene, 0.5);
    const p = project(cam, axes[0], 450, 900);
    const r = Math.abs(225 - (p?.x ?? 0)) / 225;
    expect(r).toBeGreaterThan(0.7);
    expect(r).toBeLessThan(0.8);
  });

  it("looks at the bounding centre, not the origin, with a slight downward pitch", () => {
    const off = buildScene(report([pt(1, 0, 50), pt(2, 0, 90)], 1));
    const cam = initialCamera(off, 16 / 9);
    const p = project(cam, [70, 0, 0], 1600, 900);
    expect(p?.x).toBeCloseTo(800, 0);
    expect(cam.pitch).toBeLessThan(0);
    expect(cam.pitch).toBeGreaterThan(-0.2);
  });

  it("empty scene keeps the old default pose", () => {
    expect(initialCamera(buildScene(report([]))).pos).toEqual([0, 0.15 * R, -2.6 * R]);
  });

  it("a single star does not collapse to distance 0", () => {
    const cam = initialCamera(buildScene(report([pt(1, 0, 10)], 1)));
    expect(Math.hypot(cam.pos[0] - 10, cam.pos[1], cam.pos[2])).toBeGreaterThan(10);
  });
});

describe("island inflation", () => {
  it("a tight 2-star topic is spread to exactly 0.22 x the nearest centroid distance; its centroid is unchanged", () => {
    const s = buildScene(report([pt(1, 0, -2), pt(2, 0, 2), pt(3, 1, 78), pt(4, 1, 82)]));
    expect(s.topics[0].spread).toBeCloseTo(0.22 * 80, 9);
    expect(s.topics[1].spread).toBeCloseTo(0.22 * 80, 9);
    expect(s.topics[0].centroid[0]).toBeCloseTo(0, 9);
    expect(s.stars[0].pos[0]).toBeCloseTo(-17.6, 9);
    expect(s.topics[0].spread).toBeLessThanOrEqual(0.45 * 80);
  });

  it("a spread already above 0.22 x d is left alone (catches always rescaling / min instead of max)", () => {
    const s = buildScene(report([pt(1, 0, -20), pt(2, 0, 20), pt(3, 1, 80)]));
    expect(s.topics[0].spread).toBeCloseTo(20, 9);
    expect(s.stars[0].pos[0]).toBeCloseTo(-20, 9);
  });

  it("a 1-star topic is unchanged", () => {
    const s = buildScene(report([pt(1, 0, -2), pt(2, 0, 2), pt(3, 1, 80)]));
    expect(s.stars[2].pos[0]).toBeCloseTo(80, 9);
    expect(s.topics[1].spread).toBe(0);
  });

  it("coincident stars stay finite; an empty topic is not a neighbour at the origin", () => {
    const s = buildScene(report([pt(1, 0, 40), pt(2, 0, 40), pt(3, 1, 80)], 3));
    for (const st of s.stars) for (const v of st.pos) expect(Number.isFinite(v)).toBe(true);
    const lone = buildScene(report([pt(1, 0, 78), pt(2, 0, 82)], 2));
    expect(lone.topics[0].spread).toBeCloseTo(2, 9);
  });
});

describe("constellation", () => {
  it("4-point fixture returns exactly the minimum spanning tree (not a nearest-neighbour chain)", () => {
    const pts: Vec3[] = [[0, 0, 0], [1, 0, 0], [0, 1.5, 0], [1, -3, 0]];
    expect(sorted(constellation(pts))).toEqual([[0, 1], [0, 2], [1, 3]]);
  });

  it("m-1 edges, none for 0 or 1 points", () => {
    expect(constellation([])).toEqual([]);
    expect(constellation([[1, 2, 3]])).toEqual([]);
    expect(constellation([[0, 0, 0], [1, 0, 0], [5, 0, 0]])).toHaveLength(2);
  });

  it("scene edges are global star indices and never join two topics", () => {
    const s = buildScene(report([pt(1, 0, -20), pt(2, 1, 60), pt(3, 0, 20), pt(4, 1, 100), pt(5, 1, 80)]));
    expect(s.edges).toHaveLength(1 + 2);
    for (const [a, b] of s.edges) expect(s.stars[a].topic).toBe(s.stars[b].topic);
    expect(sorted(s.edges)).toEqual([[0, 2], [1, 4], [3, 4]]);
  });
});
