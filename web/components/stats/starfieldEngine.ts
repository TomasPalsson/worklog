import { type Palette, buildPalette, draw, type View } from "./starfieldDraw";
import {
  type Camera,
  dolly,
  ease,
  flyTarget,
  initialCamera,
  look,
  orbit,
  pick,
  type Scene,
  stepCamera,
} from "./starfieldScene";

const FLY_MS = 900;
const DRAG_SLOP = 4;
const LABEL_H = 16;

export interface Hover {
  i: number;
  x: number;
  y: number;
}

export interface Engine {
  key: (k: string, down: boolean) => void;
  flyTopic: (id: number) => void;
  release: () => void;
  unpin: () => void;
  dispose: () => void;
  camera: () => Camera;
}

export interface Callbacks {
  onHover: (h: Hover | null) => void;
  onPin: (i: number | null) => void;
}

export interface EngineOpts {
  now?: () => number;
  raf?: (f: (t: number) => void) => number;
  caf?: (id: number) => void;
  camera?: Camera;
}

export function createEngine(
  wrap: HTMLElement,
  canvas: HTMLCanvasElement,
  ctx: CanvasRenderingContext2D,
  scene: Scene,
  k: number,
  cb: Callbacks,
  opts: EngineOpts = {},
): Engine {
  const now = opts.now ?? (() => performance.now());
  const requestFrame = opts.raf ?? ((f) => requestAnimationFrame(f));
  const cancelFrame = opts.caf ?? ((id) => cancelAnimationFrame(id));
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  const home = initialCamera(scene, canvas.getBoundingClientRect().width / canvas.getBoundingClientRect().height || 16 / 9);
  const v: View = { cam: opts.camera ?? home, keys: new Set(), orbit: !reduced, fly: null, drag: false, w: 0, h: 0, dpr: 1, visible: true, hover: null, pin: null, hits: [], labels: [] };
  let pal = buildPalette(wrap, k);
  let raf = 0;
  let last = 0;

  const running = () => v.visible && !document.hidden && (v.orbit || v.keys.size > 0 || v.fly !== null || v.drag);
  const frame = (ts: number) => {
    raf = 0;
    const dt = Math.min(0.05, (ts - last) / 1000);
    last = ts;
    if (v.fly) {
      v.fly.t0 ??= ts;
      const t = Math.min(1, (ts - v.fly.t0) / FLY_MS);
      v.cam = ease(v.fly.from, v.fly.to, t);
      if (t >= 1) v.fly = null;
    } else if (v.keys.size > 0) v.cam = stepCamera(v.cam, v.keys, dt);
    else if (v.orbit) v.cam = orbit(v.cam, dt);
    draw(ctx, v, pal, scene, ts, reduced);
    if (running()) raf = requestFrame(frame);
  };
  const wake = () => {
    if (raf) return;
    last = now();
    raf = requestFrame(frame);
  };
  const interrupt = (cancelFly: boolean) => {
    v.orbit = false;
    if (cancelFly) v.fly = null;
  };
  const flyTo = (to: Camera) => {
    interrupt(true);
    // the star under the pointer is about to move away: drop its card
    v.hover = null;
    cb.onHover(null);
    // A hidden page gets no animation frames: jump instead of leaving a fly that never ends.
    if (reduced || document.hidden) v.cam = to;
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
  const onVisibility = () => {
    if (document.hidden) v.keys.clear();
    wake();
  };

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
    camera() {
      return v.cam;
    },
    dispose() {
      cancelFrame(raf);
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
