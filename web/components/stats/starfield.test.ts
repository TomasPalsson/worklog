import { describe, expect, it } from "bun:test";
import { readdirSync } from "node:fs";
import type { Highlight, MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import {
  buildScene,
  type Camera,
  dolly,
  ease,
  flyTarget,
  forward,
  initialCamera,
  look,
  orbit,
  pick,
  project,
  R,
  right,
  stepCamera,
} from "./starfieldScene.js";

const topic = (id: number, seconds = 3600): Topic => ({
  id,
  name: `topic ${id}`,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds,
  blocks: 1,
  days_active: 1,
  first_day: "2026-10-01",
  last_day: "2026-10-01",
  top_tickets: [],
});

const point = (block_id: number, over: Partial<MapPoint> = {}): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx: 0.5,
  sy: 0.5,
  sz: 0.5,
  topic_id: 0,
  day: "2026-10-01",
  seconds: 3600,
  label: `block ${block_id}`,
  ticket: null,
  novelty: 0.5,
  oddball: false,
  ...over,
});

const highlight = (block_id: number): Highlight => ({ block_id, day: "2026-10-01", label: "x", ticket: null, topic_id: 0, seconds: 1, score: 0.5 });

const report = (points: MapPoint[], over: Partial<TopicsReport> = {}): TopicsReport => ({
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  naming: "off",
  topics: [topic(0), topic(1)],
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
  ...over,
});

const dotp = (a: number[], b: number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a: number[], b: number[]) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const len = (a: number[]) => Math.hypot(a[0], a[1], a[2]);
const cam0 = (): Camera => ({ pos: [0, 0, 0], yaw: 0, pitch: 0, fov: Math.PI / 3 });
const keys = (...k: string[]) => new Set(k);

describe("buildScene stars", () => {
  it("maps 0.5 to the origin and 1 / 0 to +R / -R per axis", () => {
    const s = buildScene(report([point(1), point(2, { sx: 1, sy: 1, sz: 1 }), point(3, { sx: 0, sy: 0, sz: 0 })]));
    expect(s.stars[0].pos).toEqual([0, 0, 0]);
    expect(s.stars[1].pos).toEqual([R, R, R]);
    expect(s.stars[2].pos).toEqual([-R, -R, -R]);
  });

  it("uses distinct sx/sy/sz per axis (catches swapped axes)", () => {
    const s = buildScene(report([point(1, { sx: 0.6, sy: 0.7, sz: 0.9 })]));
    expect(s.stars[0].pos[0]).toBeCloseTo(20, 9);
    expect(s.stars[0].pos[1]).toBeCloseTo(40, 9);
    expect(s.stars[0].pos[2]).toBeCloseTo(80, 9);
  });

  it("radius is 0.8 + 2.6*sqrt(share): max 3.4, quarter 2.1 (not linear), zero 0.8", () => {
    const s = buildScene(report([point(1, { seconds: 3600 }), point(2, { seconds: 900 }), point(3, { seconds: 0 })]));
    expect(s.stars[0].r).toBeCloseTo(3.4, 9);
    expect(s.stars[1].r).toBeCloseTo(2.1, 9);
    expect(s.stars[2].r).toBeCloseTo(0.8, 9);
  });

  it("all-zero seconds does not produce NaN radii", () => {
    const s = buildScene(report([point(1, { seconds: 0 })]));
    expect(s.stars[0].r).toBeCloseTo(0.8, 9);
  });

  it("firstTime follows block_id membership, not array index; oddball and fields pass through", () => {
    const s = buildScene(
      report([point(10, { oddball: true, ticket: "ABC-1", topic_id: 1, day: "2026-10-02", novelty: 0.9, label: "hi" }), point(2)], {
        first_times: [highlight(2)],
      }),
    );
    expect(s.stars[0].firstTime).toBe(false);
    expect(s.stars[1].firstTime).toBe(true);
    expect(s.stars[0].oddball).toBe(true);
    expect(s.stars[1].oddball).toBe(false);
    expect(s.stars[0]).toMatchObject({ ticket: "ABC-1", topic: 1, day: "2026-10-02", novelty: 0.9, label: "hi", seconds: 3600 });
  });
});

describe("buildScene topics", () => {
  const rep = report(
    [point(1, { sx: 1 }), point(2, { sx: 0 }), point(3), point(4, { topic_id: 1, sx: 0.6, sy: 0.5, sz: 0.5 })],
    { topics: [topic(0, 7200), topic(1), topic(2)] },
  );
  const s = buildScene(rep);

  it("centroid and RMS spread exact on a 3-point fixture", () => {
    expect(s.topics[0].centroid).toEqual([0, 0, 0]);
    expect(s.topics[0].spread).toBeCloseTo(R * Math.sqrt(2 / 3), 9);
    expect(s.topics[0].seconds).toBe(7200);
    expect(s.topics[0].name).toBe("topic 0");
  });

  it("only counts its own stars; single star has spread 0 at its position", () => {
    expect(s.topics[1].centroid[0]).toBeCloseTo(20, 9);
    expect(s.topics[1].spread).toBe(0);
  });

  it("topic without stars stays finite", () => {
    expect(s.topics[2].centroid).toEqual([0, 0, 0]);
    expect(s.topics[2].spread).toBe(0);
  });
});

describe("buildScene dust", () => {
  it("has 700 points on the 6R sphere with brightness 0.2-0.6", () => {
    const { dust } = buildScene(report([]));
    expect(dust.length).toBe(700);
    for (const d of dust) {
      expect(len(d.pos)).toBeCloseTo(6 * R, 6);
      expect(d.b).toBeGreaterThanOrEqual(0.2);
      expect(d.b).toBeLessThanOrEqual(0.6);
    }
    expect(new Set(dust.map((d) => d.pos[0])).size).toBeGreaterThan(600);
  });

  it("is deterministic across builds", () => {
    const a = buildScene(report([])).dust.slice(0, 3);
    const b = buildScene(report([point(1)])).dust.slice(0, 3);
    expect(a).toEqual(b);
  });
});

describe("camera basis", () => {
  it("initialCamera sits behind the origin looking at it, +z forward", () => {
    const c = initialCamera(buildScene(report([])));
    expect(c.pos).toEqual([0, 0.15 * R, -2.6 * R]);
    expect(c.yaw).toBe(0);
    expect(c.pitch).toBe(-0.05);
    expect(c.fov).toBeCloseTo(Math.PI / 3, 9);
  });

  it("forward/right are unit and orthogonal; yaw 0 faces +z, yaw pi/2 faces +x", () => {
    const f0 = forward(cam0());
    expect(f0[2]).toBeCloseTo(1, 9);
    expect(right(cam0())[0]).toBeCloseTo(1, 9);
    const c: Camera = { ...cam0(), yaw: Math.PI / 2, pitch: 0.3 };
    expect(len(forward(c))).toBeCloseTo(1, 9);
    expect(len(right(c))).toBeCloseTo(1, 9);
    expect(dotp(forward(c), right(c))).toBeCloseTo(0, 9);
    expect(forward({ ...cam0(), yaw: Math.PI / 2 })[0]).toBeCloseTo(1, 9);
  });
});

describe("stepCamera", () => {
  const c: Camera = { pos: [1, 2, 3], yaw: 0.7, pitch: 0.2, fov: 1 };

  it("W moves +forward by 0.6R*dt, S the opposite", () => {
    const w = stepCamera(c, keys("w"), 0.5);
    const d = sub(w.pos, c.pos);
    expect(len(d)).toBeCloseTo(30, 9);
    expect(dotp(d, forward(c))).toBeCloseTo(30, 9);
    expect(dotp(sub(stepCamera(c, keys("s"), 0.5).pos, c.pos), forward(c))).toBeCloseTo(-30, 9);
  });

  it("Shift triples the speed", () => {
    expect(len(sub(stepCamera(c, keys("w", "Shift"), 0.5).pos, c.pos))).toBeCloseTo(90, 9);
  });

  it("A/D move along right (perpendicular to forward), D positive", () => {
    const d = sub(stepCamera(c, keys("d"), 0.5).pos, c.pos);
    expect(dotp(d, right(c))).toBeCloseTo(30, 9);
    expect(dotp(d, forward(c))).toBeCloseTo(0, 9);
    expect(dotp(sub(stepCamera(c, keys("a"), 0.5).pos, c.pos), right(c))).toBeCloseTo(-30, 9);
  });

  it("E/Q move along world up/down regardless of pitch", () => {
    expect(sub(stepCamera(c, keys("e"), 0.5).pos, c.pos)[1]).toBeCloseTo(30, 9);
    expect(sub(stepCamera(c, keys("q"), 0.5).pos, c.pos)[1]).toBeCloseTo(-30, 9);
  });

  it("no keys, or opposing keys, leave the camera put", () => {
    expect(stepCamera(c, keys(), 1)).toEqual(c);
    expect(stepCamera(c, keys("w", "s"), 1).pos).toEqual(c.pos);
  });

  it("arrows turn at 1.4 rad/s: right = +yaw, up = +pitch", () => {
    expect(stepCamera(c, keys("ArrowRight"), 0.5).yaw).toBeCloseTo(0.7 + 0.7, 9);
    expect(stepCamera(c, keys("ArrowLeft"), 0.5).yaw).toBeCloseTo(0, 9);
    expect(stepCamera(c, keys("ArrowUp"), 0.1).pitch).toBeCloseTo(0.2 + 0.14, 9);
    expect(stepCamera(c, keys("ArrowDown"), 0.1).pitch).toBeCloseTo(0.2 - 0.14, 9);
  });

  it("pitch clamps at +-1.45", () => {
    expect(stepCamera(c, keys("ArrowUp"), 10).pitch).toBe(1.45);
    expect(stepCamera(c, keys("ArrowDown"), 10).pitch).toBe(-1.45);
  });
});

describe("look / dolly / orbit", () => {
  const c: Camera = { pos: [0, 0, 0], yaw: 0, pitch: 0, fov: 1 };

  it("look turns 0.004 rad per px; dragging down looks down; pitch clamps", () => {
    const l = look(c, 100, 50);
    expect(l.yaw).toBeCloseTo(0.4, 9);
    expect(l.pitch).toBeCloseTo(-0.2, 9);
    expect(look(c, 0, -1e6).pitch).toBe(1.45);
    expect(look(c, 0, 1e6).pitch).toBe(-1.45);
  });

  it("dolly moves -deltaY*0.05 along forward", () => {
    expect(dolly(c, -100).pos[2]).toBeCloseTo(5, 9);
    expect(dolly(c, 100).pos[2]).toBeCloseTo(-5, 9);
    expect(dolly(c, 0).pos).toEqual([0, 0, 0]);
  });

  it("orbit advances 0.06 rad/s keeping distance and height, facing the origin", () => {
    const s = initialCamera(buildScene(report([])));
    const o = orbit(s, 10);
    expect(Math.hypot(o.pos[0], o.pos[2])).toBeCloseTo(Math.hypot(s.pos[0], s.pos[2]), 9);
    expect(o.pos[1]).toBe(s.pos[1]);
    const raw = Math.atan2(o.pos[0], o.pos[2]) - Math.atan2(s.pos[0], s.pos[2]);
    const swept = Math.atan2(Math.sin(raw), Math.cos(raw));
    expect(Math.abs(swept)).toBeCloseTo(0.6, 9);
    const toOrigin = [-o.pos[0], -o.pos[1], -o.pos[2]];
    expect(dotp(forward(o), toOrigin)).toBeCloseTo(len(toOrigin), 6);
  });
});

describe("project", () => {
  const c = cam0();

  it("a point straight ahead lands at the canvas centre", () => {
    const p = project(c, [0, 0, 50], 800, 600);
    expect(p?.x).toBeCloseTo(400, 9);
    expect(p?.y).toBeCloseTo(300, 9);
    expect(p?.depth).toBeCloseTo(50, 9);
  });

  it("right is +x px, up is -y px, scale = focal/depth", () => {
    const p = project(c, [10, 10, 50], 800, 600);
    const f = 300 / Math.tan(Math.PI / 6);
    expect(p?.x).toBeCloseTo(400 + (10 / 50) * f, 6);
    expect(p?.y).toBeCloseTo(300 - (10 / 50) * f, 6);
    expect(p?.scale).toBeCloseTo(f / 50, 9);
  });

  it("behind the camera is null; depth 0.5 is null, just past it is not", () => {
    expect(project(c, [0, 0, -10], 800, 600)).toBeNull();
    expect(project(c, [0, 0, 0.5], 800, 600)).toBeNull();
    expect(project(c, [0, 0, 0.51], 800, 600)).not.toBeNull();
  });
});

describe("pick", () => {
  const items = [
    { i: 0, x: 100, y: 100, rPx: 3, depth: 10 },
    { i: 1, x: 108, y: 100, rPx: 3, depth: 10 },
  ];

  it("returns the nearest centre", () => {
    expect(pick(items, 107, 100)).toBe(1);
    expect(pick(items, 101, 100)).toBe(0);
  });

  it("empty and outside-radius give null; exactly maxPx is in, just past is out", () => {
    expect(pick([], 0, 0)).toBeNull();
    expect(pick(items, 100, 114)).toBe(0);
    expect(pick(items, 100, 114.01)).toBeNull();
    expect(pick(items, 100, 130)).toBeNull();
  });

  it("a star larger than maxPx extends its own radius; custom maxPx is honoured", () => {
    expect(pick([{ i: 5, x: 0, y: 0, rPx: 30 }], 25, 0)).toBe(5);
    expect(pick([{ i: 5, x: 0, y: 0, rPx: 30 }], 31, 0)).toBeNull();
    expect(pick(items, 100, 104, 3)).toBeNull();
  });

  it("equal distance goes to the nearer depth regardless of array order", () => {
    const tie = [
      { i: 0, x: 90, y: 100, rPx: 3, depth: 50 },
      { i: 1, x: 110, y: 100, rPx: 3, depth: 20 },
    ];
    expect(pick(tie, 100, 100)).toBe(1);
    expect(pick([...tie].reverse(), 100, 100)).toBe(1);
  });
});

describe("flyTarget / ease", () => {
  const start: Camera = { pos: [5, 5, 5], yaw: 0.4, pitch: -0.2, fov: 1 };

  it("looks at the centroid from max(2.2*spread, 0.35R) away", () => {
    const t = flyTarget(start, { id: 0, name: "x", centroid: [30, -10, 20], spread: 40, seconds: 1 });
    const to = sub([30, -10, 20], t.pos);
    expect(dotp(forward(t), to)).toBeGreaterThan(0.99 * len(to));
    expect(len(to)).toBeCloseTo(88, 9);
  });

  it("small spread falls back to the 0.35R floor", () => {
    const t = flyTarget(start, { id: 0, name: "x", centroid: [0, 0, 0], spread: 1, seconds: 1 });
    expect(len(t.pos)).toBeCloseTo(35, 9);
  });

  it("ease hits both ends and is cubic ease-out in between", () => {
    const b: Camera = { pos: [10, 0, 0], yaw: 1.4, pitch: 0.6, fov: 1 };
    expect(ease(start, b, 0)).toEqual(start);
    expect(ease(start, b, 1)).toEqual(b);
    const m = ease(start, b, 0.5);
    expect(m.pos[0]).toBeCloseTo(5 + 5 * 0.875, 9);
    expect(m.yaw).toBeCloseTo(0.4 + 1.0 * 0.875, 9);
    expect(m.pitch).toBeCloseTo(-0.2 + 0.8 * 0.875, 9);
  });
});

describe("module names", () => {
  it("no two modules differ only by case (webpack resolves the wrong one)", () => {
    const stems = readdirSync(import.meta.dir)
      .filter((f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f))
      .map((f) => f.replace(/\.tsx?$/, "").toLowerCase());
    expect(stems.filter((s) => s === "starfield")).toHaveLength(1);
  });
});
