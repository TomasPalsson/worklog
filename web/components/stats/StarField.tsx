"use client";

import "@/app/stats/starfield.css";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { formatDuration } from "@/lib/format";
import type { TopicsReport } from "@/lib/topics_contract";
import {
  buildScene,
  type Camera,
  dolly,
  ease,
  flyTarget,
  initialCamera,
  look,
  orbit,
  type Pickable,
  pick,
  project,
  R,
  type Scene,
  stepCamera,
} from "./starfieldScene";
import { topicColor } from "./topicColor";

const HINT = "Click the field, then W A S D to fly · drag to look · Shift faster · R reset · Esc leave";
const MOVE_KEYS = new Set(["w", "a", "s", "d", "e", "q", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);
const FLY_MS = 900;
const FOG_NEAR = 1.5 * R;
const FOG_FAR = 7 * R;
const DRAG_SLOP = 4;
const LABEL_H = 16;
const SPRITE = 64;

type Rgb = [number, number, number];
interface Palette {
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
interface Hover {
  i: number;
  x: number;
  y: number;
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
function buildPalette(wrap: HTMLElement, k: number): Palette {
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

interface View {
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

function draw(ctx: CanvasRenderingContext2D, v: View, pal: Palette, scene: Scene, now: number, reduced: boolean) {
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

interface Engine {
  key: (k: string, down: boolean) => void;
  flyTopic: (id: number) => void;
  release: () => void;
  unpin: () => void;
  dispose: () => void;
}

interface Callbacks {
  onHover: (h: Hover | null) => void;
  onPin: (i: number | null) => void;
}

function createEngine(wrap: HTMLElement, canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D, scene: Scene, k: number, cb: Callbacks): Engine {
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  const home = initialCamera(scene);
  const v: View = { cam: home, keys: new Set(), orbit: !reduced, fly: null, drag: false, w: 0, h: 0, dpr: 1, visible: true, hover: null, pin: null, hits: [], labels: [] };
  let pal = buildPalette(wrap, k);
  let raf = 0;
  let last = 0;

  const running = () => v.visible && !document.hidden && (v.orbit || v.keys.size > 0 || v.fly !== null || v.drag);
  const frame = (now: number) => {
    raf = 0;
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    if (v.fly) {
      v.fly.t0 ??= now;
      const t = Math.min(1, (now - v.fly.t0) / FLY_MS);
      v.cam = ease(v.fly.from, v.fly.to, t);
      if (t >= 1) v.fly = null;
    } else if (v.keys.size > 0) v.cam = stepCamera(v.cam, v.keys, dt);
    else if (v.orbit) v.cam = orbit(v.cam, dt);
    draw(ctx, v, pal, scene, now, reduced);
    if (running()) raf = requestAnimationFrame(frame);
  };
  const wake = () => {
    if (raf) return;
    last = performance.now();
    raf = requestAnimationFrame(frame);
  };
  const interrupt = (cancelFly: boolean) => {
    v.orbit = false;
    if (cancelFly) v.fly = null;
  };
  const flyTo = (to: Camera) => {
    interrupt(true);
    if (reduced) v.cam = to;
    else v.fly = { from: v.cam, to, t0: null };
    wake();
  };

  const resize = () => {
    const r = canvas.getBoundingClientRect();
    v.dpr = window.devicePixelRatio || 1;
    v.w = r.width;
    v.h = r.height;
    canvas.width = Math.max(1, Math.round(r.width * v.dpr));
    canvas.height = Math.max(1, Math.round(r.height * v.dpr));
    wake();
  };
  const at = (e: MouseEvent) => {
    const r = canvas.getBoundingClientRect();
    return { mx: e.clientX - r.left, my: e.clientY - r.top, wx: e.clientX - wrap.getBoundingClientRect().left, wy: e.clientY - wrap.getBoundingClientRect().top };
  };
  const labelAt = (mx: number, my: number) => v.labels.find((l) => Math.abs(mx - l.x) <= l.w / 2 && Math.abs(my - l.y) <= LABEL_H / 2);

  let lastPtr = { x: 0, y: 0 };
  let moved = 0;
  const onDown = (e: PointerEvent) => {
    v.drag = true;
    moved = 0;
    lastPtr = { x: e.clientX, y: e.clientY };
    canvas.setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    if (v.drag) {
      const dx = e.clientX - lastPtr.x;
      const dy = e.clientY - lastPtr.y;
      lastPtr = { x: e.clientX, y: e.clientY };
      moved += Math.abs(dx) + Math.abs(dy);
      interrupt(true);
      v.cam = look(v.cam, dx, dy);
      wake();
      return;
    }
    const { mx, my, wx, wy } = at(e);
    canvas.style.cursor = labelAt(mx, my) ? "pointer" : "";
    const hit = v.keys.size > 0 || v.fly ? null : pick(v.hits, mx, my);
    if (hit === v.hover) return;
    v.hover = hit;
    cb.onHover(hit === null ? null : { i: hit, x: wx, y: wy });
    wake();
  };
  const onUp = () => {
    v.drag = false;
    wake();
  };
  const onClick = (e: MouseEvent) => {
    if (moved > DRAG_SLOP) return;
    const { mx, my } = at(e);
    const hit = pick(v.hits, mx, my);
    const lab = hit === null ? labelAt(mx, my) : undefined;
    v.pin = hit;
    cb.onPin(hit);
    if (lab) {
      const t = scene.topics.find((x) => x.id === lab.id);
      if (t) flyTo(flyTarget(v.cam, t));
    }
    wake();
  };
  const onWheel = (e: WheelEvent) => {
    if (document.activeElement !== wrap) return;
    e.preventDefault();
    interrupt(true);
    v.cam = dolly(v.cam, e.deltaY);
    wake();
  };
  const onVisibility = () => wake();

  canvas.addEventListener("pointerdown", onDown);
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerup", onUp);
  canvas.addEventListener("pointercancel", onUp);
  canvas.addEventListener("click", onClick);
  canvas.addEventListener("wheel", onWheel, { passive: false });
  document.addEventListener("visibilitychange", onVisibility);
  const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
  ro?.observe(canvas);
  const io =
    typeof IntersectionObserver === "undefined"
      ? null
      : new IntersectionObserver((entries) => {
          v.visible = entries.some((en) => en.isIntersecting);
          wake();
        });
  io?.observe(wrap);
  const retheme = () => {
    pal = buildPalette(wrap, k);
    wake();
  };
  const mo = new MutationObserver(retheme);
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class"] });
  const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
  scheme?.addEventListener?.("change", retheme);
  resize();

  return {
    key(key, down) {
      if (!down) {
        v.keys.delete(key);
        wake();
        return;
      }
      if (key === "r") return flyTo(home);
      interrupt(true);
      v.hover = null;
      cb.onHover(null);
      v.keys.add(key);
      wake();
    },
    flyTopic(id) {
      const t = scene.topics.find((x) => x.id === id);
      if (t) flyTo(flyTarget(v.cam, t));
    },
    release() {
      v.keys.clear();
      wake();
    },
    unpin() {
      v.pin = null;
      wake();
    },
    dispose() {
      cancelAnimationFrame(raf);
      canvas.removeEventListener("pointerdown", onDown);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("pointercancel", onUp);
      canvas.removeEventListener("click", onClick);
      canvas.removeEventListener("wheel", onWheel);
      document.removeEventListener("visibilitychange", onVisibility);
      ro?.disconnect();
      io?.disconnect();
      mo.disconnect();
      scheme?.removeEventListener?.("change", retheme);
    },
  };
}

const keyOf = (key: string) => (key.length === 1 ? key.toLowerCase() : key);

export default function StarField({ report }: { report: TopicsReport }) {
  const scene = useMemo(() => buildScene(report), [report]);
  const k = report.topics.length;
  const hintId = useId();
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engine = useRef<Engine | null>(null);
  const [noCanvas, setNoCanvas] = useState(false);
  const [hover, setHover] = useState<Hover | null>(null);
  const [pin, setPin] = useState<number | null>(null);

  useEffect(() => {
    const wrap = wrapRef.current;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!wrap || !canvas || !ctx) {
      setNoCanvas(true);
      return;
    }
    const eng = createEngine(wrap, canvas, ctx, scene, k, { onHover: setHover, onPin: setPin });
    engine.current = eng;
    return () => {
      eng.dispose();
      engine.current = null;
    };
  }, [scene, k]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      setPin(null);
      setHover(null);
      engine.current?.unpin();
      engine.current?.release();
      e.currentTarget.blur();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const key = keyOf(e.key);
    if (key === "Shift") return void engine.current?.key(key, true);
    if (!MOVE_KEYS.has(key) && key !== "r") return;
    e.preventDefault();
    engine.current?.key(key, true);
  };
  const onKeyUp = (e: React.KeyboardEvent<HTMLDivElement>) => engine.current?.key(keyOf(e.key), false);
  const star = (i: number | null) => (i === null ? null : scene.stars[i]);
  const topicName = (id: number) => report.topics.find((t) => t.id === id)?.name ?? "";
  const card = (i: number, extra: boolean) => {
    const s = scene.stars[i];
    return (
      <>
        <strong>{s.label}</strong>
        <span>{topicName(s.topic)}</span>
        <span>
          {s.day} · {formatDuration(s.seconds)}
          {s.ticket ? ` · ${s.ticket}` : ""}
        </span>
        {extra && <span>Newness {Math.round(s.novelty * 100)}%</span>}
      </>
    );
  };
  const pinned = star(pin);
  const hovered = hover && hover.i !== pin ? star(hover.i) : null;

  return (
    <div
      ref={wrapRef}
      className="sx-star"
      tabIndex={0}
      aria-describedby={hintId}
      onKeyDown={onKeyDown}
      onKeyUp={onKeyUp}
      onBlur={() => engine.current?.release()}
    >
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={`3-D star field: ${report.points.length} blocks in ${k} topics`}
      />
      {noCanvas && <p className="sx-star-fallback">Star field needs canvas support.</p>}
      <div className="sx-star-legend">
        {report.topics.map((t) => (
          <button key={t.id} type="button" className="sx-star-chip" onClick={() => engine.current?.flyTopic(t.id)}>
            <span className="sx-star-swatch" style={{ background: topicColor(t.id, k) }} />
            {t.name}
          </button>
        ))}
      </div>
      <p id={hintId} className="sx-star-hint">
        {HINT}
      </p>
      {hovered && hover && (
        <div className="sx-star-card" style={{ left: hover.x + 12, top: hover.y + 12 }} aria-hidden="true">
          {card(hover.i, false)}
        </div>
      )}
      {pinned && pin !== null && (
        <div className="sx-star-panel">
          {card(pin, true)}
          <button type="button" className="sx-star-close" aria-label="Close" onClick={() => {
              setPin(null);
              engine.current?.unpin();
            }}
          >
            ×
          </button>
        </div>
      )}
    </div>
  );
}
