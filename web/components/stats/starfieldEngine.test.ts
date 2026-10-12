import { afterEach, describe, expect, it } from "bun:test";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import { createEngine, type Hover } from "./starfieldEngine";
import { buildScene, type Camera, flyTarget, project } from "./starfieldScene";

const W = 800;
const H = 400;

const topic = (id: number): Topic => ({
  id,
  name: `Topic ${id}`,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds: 3600,
  blocks: 2,
  days_active: 1,
  first_day: "2026-10-01",
  last_day: "2026-10-01",
  top_tickets: [],
});

const point = (block_id: number, topic_id: number, sx: number): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx,
  sy: 0.5,
  sz: 0.5,
  topic_id,
  day: "2026-10-01",
  seconds: 3600,
  label: `block ${block_id}`,
  ticket: null,
  novelty: 0.5,
  oddball: false,
});

const report = {
  topics: [topic(0), topic(1)],
  points: [point(1, 0, 0.3), point(2, 0, 0.35), point(3, 1, 0.65), point(4, 1, 0.7)],
  first_times: [],
} as unknown as TopicsReport;

// A 2-D context that accepts every call; measureText gives a fixed advance.
const fakeCtx = () =>
  new Proxy({} as Record<string, unknown>, {
    get: (_t, key) => (key === "measureText" ? (s: string) => ({ width: s.length * 8 }) : () => undefined),
    set: () => true,
  }) as unknown as CanvasRenderingContext2D;

const rect = { left: 0, top: 0, width: W, height: H, right: W, bottom: H, x: 0, y: 0, toJSON: () => ({}) };

function setup(camera?: Camera) {
  const wrap = document.createElement("div");
  const canvas = document.createElement("canvas");
  wrap.appendChild(canvas);
  document.body.appendChild(wrap);
  canvas.getBoundingClientRect = () => rect as DOMRect;
  wrap.getBoundingClientRect = () => rect as DOMRect;
  const frames: ((t: number) => void)[] = [];
  let clock = 0;
  const hovers: (Hover | null)[] = [];
  const scene = buildScene(report);
  const eng = createEngine(
    wrap,
    canvas,
    fakeCtx(),
    scene,
    2,
    { onHover: (h) => hovers.push(h), onPin: () => undefined },
    {
      now: () => clock,
      raf: (f) => frames.push(f),
      caf: () => undefined,
      camera,
    },
  );
  const run = (ms: number, step = 16) => {
    for (let t = 0; t < ms; t += step) {
      clock += step;
      const due = frames.splice(0);
      for (const f of due) f(clock);
    }
  };
  const move = (x: number, y: number) =>
    canvas.dispatchEvent(new PointerEvent("pointermove", { clientX: x, clientY: y, bubbles: true }));
  return { wrap, canvas, eng, scene, run, move, hovers, frames };
}

let hidden = false;
Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
afterEach(() => {
  hidden = false;
  document.body.replaceChildren();
});

describe("star field engine", () => {
  it("hovering a drawn star reports it (CSS-px hit boxes)", () => {
    const s = setup();
    s.eng.key("w", false); // any input stops the idle orbit so the camera is predictable
    s.run(50);
    const cam = s.eng.camera();
    const p = project(cam, s.scene.stars[0].pos, W, H)!;
    s.move(p.x, p.y);
    expect(s.hovers.at(-1)?.i).toBe(0);
  });

  it("flyTopic animates to the target, ends, and hover works again", () => {
    const s = setup();
    s.run(50);
    s.eng.flyTopic(1);
    const target = flyTarget(s.eng.camera(), s.scene.topics.find((t) => t.id === 1)!);
    s.run(1200);
    const cam = s.eng.camera();
    cam.pos.forEach((c, i) => expect(c).toBeCloseTo(target.pos[i], 6));
    expect(cam.yaw).toBeCloseTo(target.yaw, 6);
    const p = s.scene.stars.map((st) => project(cam, st.pos, W, H)).find(Boolean)!;
    const idx = s.scene.stars.findIndex((st) => project(cam, st.pos, W, H) === null ? false : true);
    s.move(p.x, p.y);
    expect(s.hovers.at(-1)?.i).toBe(idx); // catches a fly that never clears and blocks hover forever
  });

  it("on a hidden page flyTopic jumps instead of waiting for frames that never come", () => {
    const s = setup();
    s.run(50);
    hidden = true;
    const target = flyTarget(s.eng.camera(), s.scene.topics.find((t) => t.id === 1)!);
    s.eng.flyTopic(1);
    expect(s.eng.camera()).toEqual(target); // no frame ran, the camera is already there
    const p = s.scene.stars.map((st) => project(target, st.pos, W, H)).find(Boolean)!;
    s.move(p.x, p.y);
    expect(s.hovers.at(-1)).not.toBeNull(); // catches the stuck fly that blocked hover in a background tab
  });

  it("held keys are cleared when the page is hidden", () => {
    const s = setup();
    s.eng.key("w", true);
    s.run(100);
    const before = s.eng.camera().pos[2];
    hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
    hidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
    s.run(200);
    expect(s.eng.camera().pos[2]).toBeCloseTo(before, 1); // catches a stuck "w" that keeps flying
  });

  it("starts from a kept camera when one is passed", () => {
    const kept = { pos: [1, 2, 3] as [number, number, number], yaw: 0.5, pitch: 0.1, fov: Math.PI / 3 };
    const s = setup(kept);
    expect(s.eng.camera()).toEqual(kept);
  });

  it("dispose cancels frames and removes every listener", () => {
    const s = setup();
    const removed: string[] = [];
    const orig = s.canvas.removeEventListener.bind(s.canvas);
    s.canvas.removeEventListener = ((type: string, fn: EventListener, o?: EventListenerOptions) => {
      removed.push(type);
      return orig(type, fn, o);
    }) as typeof s.canvas.removeEventListener;
    s.eng.dispose();
    expect(removed.sort()).toEqual(["click", "pointercancel", "pointerdown", "pointermove", "pointerup", "wheel"]);
  });
});
