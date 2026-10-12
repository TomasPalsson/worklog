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

export type Vec3 = [number, number, number];

export interface Star {
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

export interface Scene {
  stars: Star[];
  topics: SceneTopic[];
  dust: Dust[];
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

export function buildScene(report: TopicsReport): Scene {
  const maxSeconds = Math.max(1, ...report.points.map((p) => p.seconds));
  const first = new Set(report.first_times.map((h) => h.block_id));
  const stars: Star[] = report.points.map((p) => ({
    pos: [(p.sx - 0.5) * 2 * R, (p.sy - 0.5) * 2 * R, (p.sz - 0.5) * 2 * R],
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
  const topics = report.topics.map((t): SceneTopic => {
    const mine = stars.filter((s) => s.topic === t.id);
    const n = mine.length || 1;
    const centroid = mine.reduce<Vec3>((c, s) => add(c, s.pos, 1 / n), [0, 0, 0]);
    const ms = mine.reduce((acc, s) => acc + (s.pos[0] - centroid[0]) ** 2 + (s.pos[1] - centroid[1]) ** 2 + (s.pos[2] - centroid[2]) ** 2, 0) / n;
    return { id: t.id, name: t.name, centroid, spread: Math.sqrt(ms), seconds: t.seconds };
  });
  return { stars, topics, dust: makeDust() };
}

export const initialCamera = (_scene: Scene): Camera => ({ pos: [0, 0.15 * R, -2.6 * R], yaw: 0, pitch: -0.05, fov: Math.PI / 3 });

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
