import type { Camera, Pickable, Scene } from "./starfieldScene";
import { project, R } from "./starfieldScene";
import { topicColor } from "./topicColor";

const FOG_NEAR = 1.5 * R;
const FOG_FAR = 7 * R;
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
const fogOf = (depth: number) => Math.min(1, Math.max(0, (depth - FOG_NEAR) / (FOG_FAR - FOG_NEAR)));

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
}

export function draw(ctx: CanvasRenderingContext2D, v: View, pal: Palette, scene: Scene, now: number, reduced: boolean) {
  const { w, h, cam } = v;
  ctx.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  ctx.globalCompositeOperation = "source-over";
  ctx.globalAlpha = 1;
  ctx.fillStyle = rgba(pal.bg, 1);
  ctx.fillRect(0, 0, w, h);

  const skyCam = { ...cam, pos: [0, 0, 0] as [number, number, number] };
  ctx.fillStyle = rgba(pal.fg, 1);
  for (const d of scene.dust) {
    const p = project(skyCam, d.pos, w, h);
    if (!p) continue;
    ctx.globalAlpha = d.b * (pal.dark ? 0.9 : 0.6);
    ctx.fillRect(p.x, p.y, 1.3, 1.3);
  }

  ctx.globalCompositeOperation = pal.dark ? "lighter" : "source-over";
  for (const t of scene.topics) {
    const p = project(cam, t.centroid, w, h);
    const sp = pal.topics[t.id];
    if (!p || !sp) continue;
    const rad = Math.max(t.spread * 1.8 * p.scale, 12);
    ctx.globalAlpha = pal.dark ? 0.1 : 0.07;
    ctx.drawImage(sp.sprite, p.x - rad, p.y - rad, rad * 2, rad * 2);
  }

  const seen: (Pickable & { si: number })[] = [];
  scene.stars.forEach((s, si) => {
    const p = project(cam, s.pos, w, h);
    if (p) seen.push({ i: si, si, x: p.x, y: p.y, rPx: Math.max(1, s.r * p.scale), depth: p.depth });
  });
  seen.sort((a, b) => (b.depth ?? 0) - (a.depth ?? 0));
  for (const q of seen) {
    const s = scene.stars[q.si];
    const sp = pal.topics[s.topic];
    if (!sp) continue;
    let a = 1 - 0.85 * fogOf(q.depth ?? 0);
    if (s.firstTime && !reduced) a *= 0.75 + 0.25 * Math.sin((now / 1000) * 3 + q.si);
    ctx.globalCompositeOperation = pal.dark ? "lighter" : "source-over";
    ctx.globalAlpha = a * (pal.dark ? 0.55 : 0.3);
    ctx.drawImage(sp.sprite, q.x - q.rPx * 4, q.y - q.rPx * 4, q.rPx * 8, q.rPx * 8);
    ctx.globalCompositeOperation = "source-over";
    ctx.globalAlpha = Math.min(1, a + (pal.dark ? 0 : 0.2));
    ctx.fillStyle = rgba(sp.rgb, 1);
    ctx.beginPath();
    ctx.arc(q.x, q.y, q.rPx, 0, Math.PI * 2);
    ctx.fill();
    const ringed = s.oddball || v.hover === q.si || v.pin === q.si;
    if (ringed) {
      ctx.globalAlpha = a;
      ctx.strokeStyle = rgba(pal.fg, 1);
      ctx.lineWidth = v.pin === q.si ? 2 : 1;
      ctx.beginPath();
      ctx.arc(q.x, q.y, q.rPx + 3, 0, Math.PI * 2);
      ctx.stroke();
    }
  }
  v.hits = seen.map(({ i, x, y, rPx, depth }) => ({ i, x, y, rPx, depth }));

  ctx.globalCompositeOperation = "source-over";
  ctx.font = `600 13px ${pal.font}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  v.labels = [];
  for (const t of scene.topics) {
    const p = project(cam, t.centroid, w, h);
    const sp = pal.topics[t.id];
    if (!p || !sp) continue;
    const text = t.name.toUpperCase();
    ctx.globalAlpha = 1 - 0.7 * fogOf(p.depth);
    ctx.lineWidth = 4;
    ctx.lineJoin = "round";
    ctx.strokeStyle = rgba(pal.bg, 1);
    ctx.strokeText(text, p.x, p.y);
    ctx.fillStyle = rgba(mix(sp.rgb, pal.fg, 0.5), 1);
    ctx.fillText(text, p.x, p.y);
    v.labels.push({ id: t.id, x: p.x, y: p.y, w: ctx.measureText(text).width });
  }
  ctx.globalAlpha = 1;
}
