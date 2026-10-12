import { placeLabels } from "./starfieldLabels";
import type { Camera, Pickable, Projected, Scene } from "./starfieldScene";
import { fitDistance, project } from "./starfieldScene";
import { topicColor } from "./topicColor";

const FOG_NEAR = 0.8;
const FOG_FAR = 3;
const LABEL_H = 16;
const CORE_MAX = 16;
const SPRITE = 64;

type Rgb = [number, number, number];
export interface Palette {
  dark: boolean;
  bg: Rgb;
  fg: Rgb;
  font: string;
  topics: { rgb: Rgb; sprite: HTMLCanvasElement }[];
}
interface LabelHit {
  id: number;
  x: number;
  y: number;
  w: number;
}

const rgba = (c: Rgb, a: number) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;
const mix = (a: Rgb, b: Rgb, t: number): Rgb => [a[0] * (1 - t) + b[0] * t, a[1] * (1 - t) + b[1] * t, a[2] * (1 - t) + b[2] * t];
const fogOf = (depth: number, fit: number) => Math.min(1, Math.max(0, (depth / fit - FOG_NEAR) / (FOG_FAR - FOG_NEAR)));

function sprite(c: Rgb): HTMLCanvasElement {
  const cv = document.createElement("canvas");
  cv.width = cv.height = SPRITE;
  const g = cv.getContext("2d");
  if (g) {
    const grad = g.createRadialGradient(SPRITE / 2, SPRITE / 2, 0, SPRITE / 2, SPRITE / 2, SPRITE / 2);
    grad.addColorStop(0, rgba(c, 1));
    grad.addColorStop(0.35, rgba(c, 0.45));
    grad.addColorStop(1, rgba(c, 0));
    g.fillStyle = grad;
    g.fillRect(0, 0, SPRITE, SPRITE);
  }
  return cv;
}

// CSS colours (oklch, var()) only become canvas-safe rgb once the browser has resolved them on a live element.
export function buildPalette(wrap: HTMLElement, k: number): Palette {
  const probe = document.createElement("span");
  probe.style.cssText = "position:absolute;visibility:hidden;pointer-events:none";
  wrap.appendChild(probe);
  const px = document.createElement("canvas");
  px.width = px.height = 1;
  const g = px.getContext("2d", { willReadFrequently: true });
  const resolve = (css: string): Rgb => {
    probe.style.color = css;
    if (!g) return [128, 128, 128];
    g.clearRect(0, 0, 1, 1);
    g.fillStyle = "#808080";
    g.fillStyle = getComputedStyle(probe).color;
    g.fillRect(0, 0, 1, 1);
    const d = g.getImageData(0, 0, 1, 1).data;
    return [d[0], d[1], d[2]];
  };
  const bg = resolve("var(--bg-sunk)");
  const fg = resolve("var(--fg)");
  const topics = Array.from({ length: k }, (_, id) => {
    const rgb = resolve(topicColor(id, k));
    return { rgb, sprite: sprite(rgb) };
  });
  probe.remove();
  const font = getComputedStyle(wrap).getPropertyValue("--font-mono").trim() || "monospace";
  return { dark: (0.299 * bg[0] + 0.587 * bg[1] + 0.114 * bg[2]) / 255 < 0.5, bg, fg, font, topics };
}

export interface View {
  cam: Camera;
  keys: Set<string>;
  orbit: boolean;
  fly: { from: Camera; to: Camera; t0: number | null } | null;
  drag: boolean;
  w: number;
  h: number;
  dpr: number;
  visible: boolean;
  hover: number | null;
  pin: number | null;
  hits: Pickable[];
  labels: LabelHit[];
  hotTopic?: number | null;
}

type Pj = Projected | null;
interface Frame {
  ctx: CanvasRenderingContext2D;
  v: View;
  pal: Palette;
  scene: Scene;
  now: number;
  reduced: boolean;
  fit: number;
  hot: number | null;
  proj: Pj[];
}

const lit = (f: Frame) => (f.pal.dark ? "lighter" : "source-over");
const starAlpha = (f: Frame, depth: number) => 1 - 0.85 * fogOf(depth, f.fit);

function drawDust(f: Frame) {
  const { ctx, v, pal, scene } = f;
  const skyCam = { ...v.cam, pos: [0, 0, 0] as [number, number, number] };
  ctx.fillStyle = rgba(pal.fg, 1);
  scene.dust.forEach((d, i) => {
    const p = project(skyCam, d.pos, v.w, v.h);
    if (!p) return;
    const tw = f.reduced ? 1 : 0.7 + 0.3 * Math.sin((f.now / 1000) * 0.9 + i * 1.7);
    ctx.globalAlpha = d.b * tw * (pal.dark ? 0.9 : 0.6);
    ctx.fillRect(p.x, p.y, 1.3, 1.3);
  });
}

function drawNebulae(f: Frame) {
  const { ctx, pal, scene, v } = f;
  ctx.globalCompositeOperation = lit(f);
  for (const t of scene.topics) {
    const p = project(v.cam, t.centroid, v.w, v.h);
    const sp = pal.topics[t.id];
    if (!p || !sp) continue;
    const rad = Math.max(t.spread * 2.2 * p.scale, 28);
    ctx.globalAlpha = (pal.dark ? 0.16 : 0.09) * (f.hot === t.id ? 1.6 : 1);
    ctx.drawImage(sp.sprite, p.x - rad, p.y - rad, rad * 2, rad * 2);
  }
}

function drawEdges(f: Frame) {
  const { ctx, pal, scene, proj } = f;
  ctx.globalCompositeOperation = "source-over";
  ctx.lineWidth = 1;
  for (const [a, b] of scene.edges) {
    const pa = proj[a];
    const pb = proj[b];
    const sp = pal.topics[scene.stars[a].topic];
    if (!pa || !pb || !sp) continue;
    const hot = f.hot === scene.stars[a].topic;
    ctx.globalAlpha = (hot ? 0.6 : pal.dark ? 0.28 : 0.18) * starAlpha(f, Math.min(pa.depth, pb.depth));
    ctx.strokeStyle = rgba(sp.rgb, 1);
    ctx.beginPath();
    ctx.moveTo(pa.x, pa.y);
    ctx.lineTo(pb.x, pb.y);
    ctx.stroke();
  }
}

function drawStar(f: Frame, si: number, p: Projected) {
  const { ctx, v, pal, scene } = f;
  const s = scene.stars[si];
  const sp = pal.topics[s.topic];
  if (!sp) return;
  const core = Math.min(CORE_MAX, Math.max(1.6, s.r * p.scale));
  let a = starAlpha(f, p.depth);
  if (s.firstTime && !f.reduced) a *= 0.75 + 0.25 * Math.sin((f.now / 1000) * 3 + si);
  if (f.hot !== null && f.hot !== s.topic) a *= 0.45;
  ctx.globalCompositeOperation = lit(f);
  ctx.globalAlpha = a * (pal.dark ? 0.8 : 0.35);
  ctx.drawImage(sp.sprite, p.x - core * 5, p.y - core * 5, core * 10, core * 10);
  ctx.globalCompositeOperation = "source-over";
  ctx.globalAlpha = Math.min(1, a + (pal.dark ? 0 : 0.2));
  ctx.fillStyle = rgba(sp.rgb, 1);
  ctx.beginPath();
  ctx.arc(p.x, p.y, core, 0, Math.PI * 2);
  ctx.fill();
  if (s.oddball || v.hover === si || v.pin === si) {
    ctx.globalAlpha = a;
    ctx.strokeStyle = rgba(pal.fg, 1);
    ctx.lineWidth = v.pin === si ? 2 : 1;
    ctx.beginPath();
    ctx.arc(p.x, p.y, core + 3, 0, Math.PI * 2);
    ctx.stroke();
  }
}

function drawLabels(f: Frame) {
  const { ctx, v, pal, scene } = f;
  ctx.globalCompositeOperation = "source-over";
  ctx.font = `600 13px ${pal.font}`;
  ctx.letterSpacing = "0.06em";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const cands = scene.topics.flatMap((t) => {
    const p = project(v.cam, t.centroid, v.w, v.h);
    if (!p || !pal.topics[t.id]) return [];
    const text = t.name.toUpperCase();
    const y = p.y - (t.spread * p.scale + 12);
    return [{ id: t.id, x: p.x, y, w: ctx.measureText(text).width, h: LABEL_H, priority: t.seconds, force: f.hot === t.id || v.hotTopic === t.id, text, depth: p.depth }];
  });
  const placed = placeLabels(cands, 6, { w: v.w, h: v.h });
  v.labels = [];
  ctx.lineWidth = 4;
  ctx.lineJoin = "round";
  for (const pl of placed) {
    const c = cands.find((x) => x.id === pl.id);
    const sp = pal.topics[pl.id];
    if (!c || !sp) continue;
    ctx.globalAlpha = 1 - 0.7 * fogOf(c.depth, f.fit);
    ctx.strokeStyle = rgba(pal.bg, 1);
    ctx.strokeText(c.text, pl.x, pl.y);
    ctx.fillStyle = rgba(mix(sp.rgb, pal.fg, 0.5), 1);
    ctx.fillText(c.text, pl.x, pl.y);
    v.labels.push({ id: pl.id, x: pl.x, y: pl.y, w: c.w });
  }
}

export function draw(ctx: CanvasRenderingContext2D, v: View, pal: Palette, scene: Scene, now: number, reduced: boolean) {
  const { w, h, cam } = v;
  ctx.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  ctx.globalCompositeOperation = "source-over";
  ctx.globalAlpha = 1;
  ctx.fillStyle = rgba(pal.bg, 1);
  ctx.fillRect(0, 0, w, h);

  const focus = v.hover ?? v.pin;
  const proj = scene.stars.map((s) => project(cam, s.pos, w, h));
  const f: Frame = { ctx, v, pal, scene, now, reduced, fit: fitDistance(scene, w / Math.max(1, h)), hot: focus === null ? null : scene.stars[focus]?.topic ?? null, proj };
  drawDust(f);
  drawNebulae(f);
  drawEdges(f);
  const order = proj.flatMap((p, si) => (p ? [{ si, p }] : [])).sort((a, b) => b.p.depth - a.p.depth);
  for (const { si, p } of order) drawStar(f, si, p);
  v.hits = order.map(({ si, p }) => ({ i: si, x: p.x, y: p.y, rPx: Math.min(CORE_MAX, Math.max(1.6, scene.stars[si].r * p.scale)), depth: p.depth }));
  drawLabels(f);
  ctx.globalAlpha = 1;
}
