// Pure flat-map layout: dot positions and nudged, island-anchored labels.
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { topicColor } from "./topicColor";

export const GAL_W = 1000;
export const GAL_H = 620;
const MIN_H = 360;
const PAD = 40;
const NUDGE_ITERATIONS = 50;
const NAME_CHAR_W = 0.62; // mono advance per font-size, including letter-spacing
const LABEL_MIN_X = 8;
const LABEL_MAX_X = GAL_W - 8;
const MIN_FONT = 11;
const LABEL_GAP = 8; // breathing room kept between nudged labels
export const NAME_LINE_H = 1.1; // second name line step, per font-size
export const LABEL_LINE_GAP = 16; // name-to-hours baseline step; the hours line is part of the label's box
export const HOURS_FONT = 13; // keep equal to .sx-gal-hours font-size in galaxy.css
const MAX_LABEL_CHARS = 24;
const MAX_DELAY_MS = 1200;

export const hoursLabel = (sec: number) => `${(sec / 3600).toFixed(1)}h`;

export interface GalaxyDot {
  point: MapPoint;
  cx: number;
  cy: number;
  r: number;
  fill: string;
  rank: number;
  delayMs: number;
}

export interface GalaxyLabel {
  topic: Topic;
  lines: string[];
  x: number;
  y: number;
  fontSize: number;
  w: number;
  h: number;
}

export interface GalaxyLayout {
  height: number;
  dots: GalaxyDot[];
  labels: GalaxyLabel[];
}

const ISLAND_GAP = 6;

function coversDot(x: number, y: number, w: number, h: number, d: GalaxyDot): boolean {
  const nx = Math.min(x + w / 2, Math.max(x - w / 2, d.cx));
  const ny = Math.min(y + h / 2, Math.max(y - h / 2, d.cy));
  return (nx - d.cx) ** 2 + (ny - d.cy) ** 2 < d.r ** 2;
}

function weightedMedian(vals: { v: number; w: number }[]): number {
  const sorted = [...vals].sort((a, b) => a.v - b.v);
  const half = sorted.reduce((s, e) => s + e.w, 0) / 2;
  let acc = 0;
  for (const e of sorted) {
    acc += e.w;
    if (acc >= half) return e.v;
  }
  return sorted[0].v;
}

export function wrapName(name: string): string[] {
  if (name.length <= MAX_LABEL_CHARS) return [name];
  const space = name.lastIndexOf(" ", MAX_LABEL_CHARS);
  const at = space > 0 ? space : MAX_LABEL_CHARS;
  const rest = name.slice(space > 0 ? at + 1 : at);
  return [name.slice(0, at), rest.length > MAX_LABEL_CHARS ? `${rest.slice(0, MAX_LABEL_CHARS - 1)}…` : rest];
}

/** Dots in a 1000-wide viewBox whose height follows the y extent. */
function placeDots(points: MapPoint[], k: number): { height: number; dots: GalaxyDot[] } {
  const maxSec = Math.max(1, ...points.map((p) => p.seconds));
  const order = [...points].sort((a, b) => a.day.localeCompare(b.day) || a.block_id - b.block_id);
  const rankOf = new Map(order.map((p, i) => [p.block_id, i]));
  const ys = points.map((p) => p.y);
  const yMin = Math.min(...ys);
  const span = (Math.max(...ys) - yMin) * (GAL_H - 2 * PAD);
  const height = points.length ? Math.max(MIN_H, span + 2 * PAD) : GAL_H;
  const top = (height - span) / 2;
  const dots = points.map((p) => {
    const rank = rankOf.get(p.block_id) ?? 0;
    return {
      point: p,
      cx: PAD + p.x * (GAL_W - 2 * PAD),
      cy: top + (p.y - yMin) * (GAL_H - 2 * PAD),
      r: 2.5 + 6 * Math.sqrt(p.seconds / maxSec),
      fill: topicColor(p.topic_id, k),
      rank,
      delayMs: Math.min(rank * 3, MAX_DELAY_MS),
    };
  });
  return { height, dots };
}

/** A topic's label at its island's weighted-median x, just above the island (below when there is no room). */
function placeLabel(topic: Topic, mine: GalaxyDot[], dots: GalaxyDot[], height: number, totalSec: number): GalaxyLabel {
  const fontSize = MIN_FONT + 9 * Math.sqrt(totalSec > 0 ? topic.seconds / totalSec : 0);
  const lines = wrapName(topic.name);
  const hoursW = hoursLabel(topic.seconds).length * HOURS_FONT * NAME_CHAR_W;
  const w = Math.max(Math.max(...lines.map((t) => t.length)) * fontSize * NAME_CHAR_W, hoursW);
  const h = fontSize * (1 + (lines.length - 1) * NAME_LINE_H) + LABEL_LINE_GAP + HOURS_FONT;
  const mid = weightedMedian(mine.map((d) => ({ v: d.cx, w: d.point.seconds + 1 })));
  const cy = weightedMedian(mine.map((d) => ({ v: d.cy, w: d.point.seconds + 1 })));
  const top = Math.min(...mine.map((d) => d.cy - d.r));
  const bottom = Math.max(...mine.map((d) => d.cy + d.r));
  const left = Math.min(...mine.map((d) => d.cx - d.r));
  const right = Math.max(...mine.map((d) => d.cx + d.r));
  // Candidate spots in order of preference: above, below, right of, left of the island. The first one that is inside
  // the map and covers no dot wins; if none is clear, the first one inside the map (above, else below).
  const spots = [
    { x: mid, y: top - ISLAND_GAP - h / 2 },
    { x: mid, y: bottom + ISLAND_GAP + h / 2 },
    { x: right + ISLAND_GAP + w / 2, y: cy },
    { x: left - ISLAND_GAP - w / 2, y: cy },
  ];
  // above/below only need to fit vertically (x is clamped onto the map afterwards); a side spot must fit both ways
  const inside = (q: { x: number; y: number }, i: number) =>
    q.y - h / 2 >= 0 && q.y + h / 2 <= height && (i < 2 || (q.x - w / 2 >= LABEL_MIN_X && q.x + w / 2 <= LABEL_MAX_X));
  const clear = (q: { x: number; y: number }) => dots.every((d) => !coversDot(q.x, q.y, w, h, d));
  const pick = spots.find((q, i) => inside(q, i) && clear({ x: Math.min(LABEL_MAX_X - w / 2, Math.max(LABEL_MIN_X + w / 2, q.x)), y: q.y })) ?? (spots[0].y - h / 2 >= 0 ? spots[0] : spots[1]);
  const { x, y } = pick;
  return { topic, lines, x, y, fontSize, w, h };
}

/** One pass pushing overlapping label pairs apart vertically; true when anything moved. */
function separateLabels(labels: GalaxyLabel[]): boolean {
  let moved = false;
  const byY = [...labels].sort((p, q) => p.y - q.y);
  for (let i = 0; i < byY.length; i++) {
    for (let j = i + 1; j < byY.length; j++) {
      const a = byY[i];
      const b = byY[j];
      if (Math.abs(a.x - b.x) >= (a.w + b.w) / 2) continue;
      const need = (a.h + b.h) / 2 + LABEL_GAP - Math.abs(a.y - b.y);
      if (need <= 1e-9) continue;
      const dir = a.y <= b.y ? 1 : -1;
      a.y -= (dir * need) / 2;
      b.y += (dir * need) / 2;
      moved = true;
    }
  }
  return moved;
}

/** One pass moving each label off any other topic's dot it covers; true when anything moved. */
function clearOtherDots(labels: GalaxyLabel[], dots: GalaxyDot[]): boolean {
  let moved = false;
  for (const l of labels) {
    for (const d of dots) {
      if (d.point.topic_id === l.topic.id) continue;
      const dx = Math.abs(d.cx - l.x) - l.w / 2;
      if (dx >= d.r) continue;
      const dy = Math.abs(d.cy - l.y) - l.h / 2;
      if (dy >= d.r) continue;
      const reach = Math.sqrt(d.r ** 2 - Math.max(dx, 0) ** 2);
      if (Math.max(dy, 0) ** 2 + Math.max(dx, 0) ** 2 >= d.r ** 2) continue;
      l.y += (l.y <= d.cy ? -1 : 1) * (reach - dy + 0.01);
      moved = true;
    }
  }
  return moved;
}

/** Pure placement of dots and labels in a 1000-wide viewBox whose height follows the y extent. */
export function galaxyLayout(report: Pick<TopicsReport, "topics" | "points">): GalaxyLayout {
  const { topics } = report;
  const index = new Set(topics.map((t) => t.id));
  const points = report.points.filter((p) => index.has(p.topic_id));
  const { height, dots } = placeDots(points, topics.length);
  const totalSec = topics.reduce((s, t) => s + t.seconds, 0);
  const labels: GalaxyLabel[] = [];
  for (const topic of topics) {
    const mine = dots.filter((d) => d.point.topic_id === topic.id);
    if (mine.length > 0) labels.push(placeLabel(topic, mine, dots, height, totalSec));
  }
  const clamp = (l: GalaxyLabel) => {
    l.x = Math.min(LABEL_MAX_X - l.w / 2, Math.max(LABEL_MIN_X + l.w / 2, l.x));
    l.y = Math.min(height - l.h / 2, Math.max(l.h / 2, l.y));
  };
  labels.forEach(clamp);
  for (let it = 0; it < NUDGE_ITERATIONS; it++) {
    const apart = separateLabels(labels);
    const offDots = clearOtherDots(labels, dots);
    const moved = apart || offDots;
    labels.forEach(clamp);
    if (!moved) break;
  }
  return { height, dots, labels };
}
