import type { TopicsReport } from "@/lib/topics_contract";

export const R = 100;
const MIN_R = 0.8;
const R_RANGE = 2.6;
const DUST_COUNT = 700;
const DUST_RADIUS = 6 * R;
const SPEED = 0.6 * R;
const FAST = 3;
const TURN = 1.4;
const MAX_PITCH = 1.45;
const LOOK_PER_PX = 0.004;
const ORBIT_RATE = 0.06;
const FLY_MIN = 0.35 * R;
const FOV = Math.PI / 3;
const FIT = 0.75;
const HOME_PITCH = -0.05;
const SPREAD_MIN = 0.22;

export type Vec3 = [number, number, number];

export interface Star {
  /** The block's id: stable across report refreshes, unlike the array index. */
  id: number;
  pos: Vec3;
  r: number;
  topic: number;
  firstTime: boolean;
  oddball: boolean;
  label: string;
  day: string;
  seconds: number;
  ticket: string | null;
  novelty: number;
}

export interface SceneTopic {
  id: number;
  name: string;
  centroid: Vec3;
  spread: number;
  seconds: number;
}

export interface Dust {
  pos: Vec3;
  b: number;
}

export type Edge = [number, number];

export interface Scene {
  stars: Star[];
  topics: SceneTopic[];
  dust: Dust[];
  edges: Edge[];
  bound: { centre: Vec3; radius: number };
}

export interface Camera {
  pos: Vec3;
  yaw: number;
  pitch: number;
  fov: number;
}

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const add = (a: Vec3, b: Vec3, k: number): Vec3 => [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k];
const clampPitch = (p: number) => Math.max(-MAX_PITCH, Math.min(MAX_PITCH, p));

function makeDust(): Dust[] {
  let s = 1;
  const rnd = () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296;
  };
  return Array.from({ length: DUST_COUNT }, () => {
    const z = 2 * rnd() - 1;
    const phi = 2 * Math.PI * rnd();
    const xy = Math.sqrt(1 - z * z);
    return { pos: [DUST_RADIUS * xy * Math.cos(phi), DUST_RADIUS * z, DUST_RADIUS * xy * Math.sin(phi)], b: 0.2 + 0.4 * rnd() };
  });
}

function residualAxis(axes: Vec3[]): Vec3 {
  const units: Vec3[] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const rest = units.map((u) => axes.reduce((r, a) => add(r, a, -dot(r, a)), u));
  return rest.reduce((best, r) => (Math.hypot(...r) > Math.hypot(...best) ? r : best));
}

// Principal axes of a point cloud, largest spread first (power iteration with Gram-Schmidt, fixed starts: deterministic).
export function principalAxes(pts: Vec3[]): Vec3[] {
  const n = pts.length;
  const c = pts.reduce<Vec3>((acc, p) => add(acc, p, 1 / Math.max(1, n)), [0, 0, 0]);
  const cov = [0, 1, 2].map((i) => [0, 1, 2].map((j) => pts.reduce((acc, p) => acc + ((p[i] - c[i]) * (p[j] - c[j])) / Math.max(1, n), 0)));
  const axes: Vec3[] = [];
  for (const start of [[1, 0.3, 0.1], [0.2, 1, 0.3], [0.1, 0.2, 1]] as Vec3[]) {
    let v = start;
    for (let it = 0; it < 120; it++) {
      for (const a of axes) v = add(v, a, -dot(v, a));
      v = [dot(cov[0] as Vec3, v), dot(cov[1] as Vec3, v), dot(cov[2] as Vec3, v)];
      for (const a of axes) v = add(v, a, -dot(v, a));
      const len = Math.hypot(v[0], v[1], v[2]);
      if (len < 1e-12) break;
      v = [v[0] / len, v[1] / len, v[2] / len];
    }
    for (const a of axes) v = add(v, a, -dot(v, a));
    // A flat or straight cloud has no spread left in this direction: v is float noise. Use the unit axis that keeps
    // the most length once the axes found so far are removed, so the basis stays orthonormal (the turn stays rigid).
    if (Math.hypot(v[0], v[1], v[2]) < 1e-6) v = residualAxis(axes);
    const len = Math.hypot(v[0], v[1], v[2]) || 1;
    v = [v[0] / len, v[1] / len, v[2] / len];
    const big = v.reduce((b, x, i) => (Math.abs(x) > Math.abs(v[b]) ? i : b), 0);
    axes.push(v[big] < 0 ? [-v[0], -v[1], -v[2]] : v);
  }
  return axes;
}

// Centre the cloud and turn its longest side onto x (the wide side of the canvas), the next onto y.
function alignToAxes(pts: Vec3[]): Vec3[] {
  if (pts.length < 3) return pts;
  const axes = principalAxes(pts);
  const c = pts.reduce<Vec3>((acc, p) => add(acc, p, 1 / pts.length), [0, 0, 0]);
  return pts.map((p) => {
    const d: Vec3 = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    return [dot(d, axes[0]), dot(d, axes[1]), dot(d, axes[2])];
  });
}

/** Index of the star for a block id in this scene, or null when the block is gone (or no id). */
export function starIndexOf(scene: Scene, blockId: number | null): number | null {
  if (blockId === null) return null;
  const i = scene.stars.findIndex((s) => s.id === blockId);
  return i < 0 ? null : i;
}

/** `align: false` keeps the raw sx/sy/sz mapping (tests of the geometry itself); the view always aligns. */
export function buildScene(report: TopicsReport, opts: { align?: boolean } = {}): Scene {
  const maxSeconds = Math.max(1, ...report.points.map((p) => p.seconds));
  const first = new Set(report.first_times.map((h) => h.block_id));
  const raw = report.points.map((p): Vec3 => [(p.sx - 0.5) * 2 * R, (p.sy - 0.5) * 2 * R, (p.sz - 0.5) * 2 * R]);
  const placed = opts.align === false ? raw : alignToAxes(raw);
  const stars: Star[] = report.points.map((p, i) => ({
    id: p.block_id,
    pos: placed[i],
    r: MIN_R + R_RANGE * Math.sqrt(p.seconds / maxSeconds),
    topic: p.topic_id,
    firstTime: first.has(p.block_id),
    oddball: p.oddball,
    label: p.label,
    day: p.day,
    seconds: p.seconds,
    ticket: p.ticket,
    novelty: p.novelty,
  }));
  inflate(stars);
  const topics = report.topics.map((t): SceneTopic => {
    const { centroid, spread } = spreadOf(stars.filter((s) => s.topic === t.id).map((s) => s.pos));
    return { id: t.id, name: t.name, centroid, spread, seconds: t.seconds };
  });
  return { stars, topics, dust: makeDust(), edges: edgesOf(stars), bound: boundOf(stars) };
}

function spreadOf(pts: Vec3[]): { centroid: Vec3; spread: number } {
  const n = pts.length || 1;
  const centroid = pts.reduce<Vec3>((c, p) => add(c, p, 1 / n), [0, 0, 0]);
  const ms = pts.reduce((acc, p) => acc + (p[0] - centroid[0]) ** 2 + (p[1] - centroid[1]) ** 2 + (p[2] - centroid[2]) ** 2, 0) / n;
  return { centroid, spread: Math.sqrt(ms) };
}

function groupByTopic(stars: Star[]): Map<number, Star[]> {
  const by = new Map<number, Star[]>();
  for (const s of stars) by.set(s.topic, [...(by.get(s.topic) ?? []), s]);
  return by;
}

// A topic's stars are scaled about their centroid until the RMS spread reaches SPREAD_MIN x the distance to the nearest
// other populated topic, so the 0.45 x d ceiling can only matter for a spread that was already larger (never shrunk).
function inflate(stars: Star[]) {
  const groups = [...groupByTopic(stars).values()].map((g) => ({ g, ...spreadOf(g.map((s) => s.pos)) }));
  for (const t of groups) {
    if (t.g.length < 2 || t.spread === 0) continue;
    const d = Math.min(...groups.filter((o) => o !== t).map((o) => Math.hypot(o.centroid[0] - t.centroid[0], o.centroid[1] - t.centroid[1], o.centroid[2] - t.centroid[2])));
    if (!Number.isFinite(d)) continue;
    const k = Math.max(t.spread, SPREAD_MIN * d) / t.spread;
    for (const s of t.g) s.pos = [t.centroid[0] + (s.pos[0] - t.centroid[0]) * k, t.centroid[1] + (s.pos[1] - t.centroid[1]) * k, t.centroid[2] + (s.pos[2] - t.centroid[2]) * k];
  }
}

// Euclidean minimum spanning tree (Prim, O(m^2)); edges are index pairs into `points`.
export function constellation(points: Vec3[]): Edge[] {
  const m = points.length;
  if (m < 2) return [];
  const d2 = (a: Vec3, b: Vec3) => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;
  const best = points.map((p) => d2(p, points[0]));
  const from = points.map(() => 0);
  const inTree = points.map((_, i) => i === 0);
  const edges: Edge[] = [];
  for (let n = 1; n < m; n++) {
    let j = -1;
    for (let i = 0; i < m; i++) if (!inTree[i] && (j < 0 || best[i] < best[j])) j = i;
    inTree[j] = true;
    edges.push([from[j], j]);
    for (let i = 0; i < m; i++) {
      const d = d2(points[i], points[j]);
      if (!inTree[i] && d < best[i]) {
        best[i] = d;
        from[i] = j;
      }
    }
  }
  return edges;
}

function edgesOf(stars: Star[]): Edge[] {
  const idx = new Map<number, number[]>();
  stars.forEach((s, i) => idx.set(s.topic, [...(idx.get(s.topic) ?? []), i]));
  return [...idx.values()].flatMap((ids) => constellation(ids.map((i) => stars[i].pos)).map(([a, b]): Edge => [ids[a], ids[b]]));
}

function boundOf(stars: Star[]): Scene["bound"] {
  const { centroid } = spreadOf(stars.map((s) => s.pos));
  const radius = Math.max(0, ...stars.map((s) => Math.hypot(s.pos[0] - centroid[0], s.pos[1] - centroid[1], s.pos[2] - centroid[2])));
  return { centre: centroid, radius };
}

// Camera sits on the line back from the bound centre along +z. A star offset (x, y, z) from the centre, seen from
// distance d, lands at x / ((d + z) tan(halfX)) of the half-width (same for y), so the smallest d that keeps every star
// within FIT (75%) of each half-extent is max over stars of |x| / (FIT tan halfX) - z and |y| / (FIT tan halfY) - z.
// A wide canvas therefore frames the cloud's long (x) side, not a sphere around it. Floored at FLY_MIN's fit.
export function fitDistance(scene: Scene, aspect = 16 / 9): number {
  const halfY = FOV / 2;
  const halfX = Math.atan(Math.tan(halfY) * aspect);
  const c = scene.bound.centre;
  const need = scene.stars.map((s) => {
    const [x, y, z] = [s.pos[0] - c[0], s.pos[1] - c[1], s.pos[2] - c[2]];
    return Math.max(Math.abs(x) / (FIT * Math.tan(halfX)), Math.abs(y) / (FIT * Math.tan(halfY))) - z;
  });
  return Math.max(FLY_MIN / (FIT * Math.tan(Math.min(halfY, halfX))), ...need);
}

export function initialCamera(scene: Scene, aspect = 16 / 9): Camera {
  if (scene.stars.length === 0) return { pos: [0, 0.15 * R, -2.6 * R], yaw: 0, pitch: HOME_PITCH, fov: FOV };
  const cam: Camera = { pos: [0, 0, 0], yaw: 0, pitch: HOME_PITCH, fov: FOV };
  return { ...cam, pos: add(scene.bound.centre, forward(cam), -fitDistance(scene, aspect)) };
}

export const forward = (c: Camera): Vec3 => [Math.cos(c.pitch) * Math.sin(c.yaw), Math.sin(c.pitch), Math.cos(c.pitch) * Math.cos(c.yaw)];
export const right = (c: Camera): Vec3 => [Math.cos(c.yaw), 0, -Math.sin(c.yaw)];
const up = (c: Camera): Vec3 => [-Math.sin(c.pitch) * Math.sin(c.yaw), Math.cos(c.pitch), -Math.sin(c.pitch) * Math.cos(c.yaw)];

export function stepCamera(cam: Camera, keys: ReadonlySet<string>, dt: number): Camera {
  const v = SPEED * dt * (keys.has("Shift") ? FAST : 1);
  const axis = (pos: string, neg: string) => (keys.has(pos) ? 1 : 0) - (keys.has(neg) ? 1 : 0);
  let pos = cam.pos;
  pos = add(pos, forward(cam), v * axis("w", "s"));
  pos = add(pos, right(cam), v * axis("d", "a"));
  pos = add(pos, [0, 1, 0], v * axis("e", "q"));
  return {
    ...cam,
    pos,
    yaw: cam.yaw + TURN * dt * axis("ArrowRight", "ArrowLeft"),
    pitch: clampPitch(cam.pitch + TURN * dt * axis("ArrowUp", "ArrowDown")),
  };
}

export const look = (cam: Camera, dxPx: number, dyPx: number): Camera => ({
  ...cam,
  yaw: cam.yaw + dxPx * LOOK_PER_PX,
  pitch: clampPitch(cam.pitch - dyPx * LOOK_PER_PX),
});

export const dolly = (cam: Camera, wheelDeltaY: number): Camera => ({ ...cam, pos: add(cam.pos, forward(cam), (-wheelDeltaY * 0.05 * R) / 100) });

export function orbit(cam: Camera, dt: number): Camera {
  const rho = Math.hypot(cam.pos[0], cam.pos[2]);
  const theta = Math.atan2(cam.pos[0], cam.pos[2]) + ORBIT_RATE * dt;
  const x = rho * Math.sin(theta);
  const z = rho * Math.cos(theta);
  return { ...cam, pos: [x, cam.pos[1], z], yaw: Math.atan2(-x, -z), pitch: Math.atan2(-cam.pos[1], rho) };
}

export interface Projected {
  x: number;
  y: number;
  depth: number;
  scale: number;
}

export function project(cam: Camera, p: Vec3, w: number, h: number): Projected | null {
  const rel: Vec3 = [p[0] - cam.pos[0], p[1] - cam.pos[1], p[2] - cam.pos[2]];
  const depth = dot(rel, forward(cam));
  if (depth <= 0.5) return null;
  const f = h / 2 / Math.tan(cam.fov / 2);
  return { x: w / 2 + (dot(rel, right(cam)) / depth) * f, y: h / 2 - (dot(rel, up(cam)) / depth) * f, depth, scale: f / depth };
}

export interface Pickable {
  i: number;
  x: number;
  y: number;
  rPx: number;
  depth?: number;
}

export function pick(projected: Pickable[], mx: number, my: number, maxPx = 14): number | null {
  let best: { i: number; d: number; depth: number } | null = null;
  for (const p of projected) {
    const d = Math.hypot(p.x - mx, p.y - my);
    if (d > Math.max(maxPx, p.rPx)) continue;
    const depth = p.depth ?? 0;
    if (!best || d < best.d || (d === best.d && depth < best.depth)) best = { i: p.i, d, depth };
  }
  return best ? best.i : null;
}

export function flyTarget(cam: Camera, topic: SceneTopic): Camera {
  const dist = Math.max(2.2 * topic.spread, FLY_MIN);
  return { ...cam, pos: add(topic.centroid, forward(cam), -dist) };
}

export function ease(a: Camera, b: Camera, t: number): Camera {
  const e = 1 - (1 - t) ** 3;
  const mix = (x: number, y: number) => x * (1 - e) + y * e;
  return {
    pos: [mix(a.pos[0], b.pos[0]), mix(a.pos[1], b.pos[1]), mix(a.pos[2], b.pos[2])],
    yaw: mix(a.yaw, b.yaw),
    pitch: mix(a.pitch, b.pitch),
    fov: mix(a.fov, b.fov),
  };
}
