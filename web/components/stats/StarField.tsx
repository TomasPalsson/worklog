"use client";

import "@/app/stats/starfield.css";
import { type RefObject, useEffect, useId, useMemo, useRef, useState } from "react";
import { formatDuration } from "@/lib/format";
import type { Topic, TopicsReport } from "@/lib/topics_contract";
import { type Engine, type Hover, createEngine } from "./starfieldEngine";
import { buildScene, type Camera, type Scene, type Star, starIndexOf } from "./starfieldScene";
import { topicColor } from "./topicColor";

const HINT = "Click the field, then W A S D to fly · drag to look · Shift faster · R reset · Esc leave";
const MOVE_KEYS = new Set(["w", "a", "s", "d", "e", "q", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);

const keyOf = (key: string) => (key.length === 1 ? key.toLowerCase() : key);

/**
 * One engine per scene. The camera survives a report refresh (AI names arriving) when the topic count is unchanged,
 * and the pinned star is held by block id so a refresh that reorders or drops points never shows another block.
 */
function useStarEngine(scene: Scene, k: number, wrapRef: RefObject<HTMLDivElement | null>, canvasRef: RefObject<HTMLCanvasElement | null>) {
  const engine = useRef<Engine | null>(null);
  const [noCanvas, setNoCanvas] = useState(false);
  const [hover, setHover] = useState<Hover | null>(null);
  const [pin, setPin] = useState<number | null>(null);
  const pinRef = useRef<number | null>(null);
  pinRef.current = pin;
  const kept = useRef<{ cam: Camera; k: number } | null>(null);

  useEffect(() => {
    const wrap = wrapRef.current;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!wrap || !canvas || !ctx) {
      setNoCanvas(true);
      return;
    }
    const camera = kept.current?.k === k ? kept.current.cam : undefined;
    const pinIndex = starIndexOf(scene, pinRef.current);
    if (pinRef.current !== null && pinIndex === null) setPin(null);
    const onPin = (i: number | null) => setPin(i === null ? null : scene.stars[i].id);
    const eng = createEngine(wrap, canvas, ctx, scene, k, { onHover: setHover, onPin }, { camera, pin: pinIndex });
    engine.current = eng;
    return () => {
      kept.current = { cam: eng.camera(), k };
      setHover(null);
      eng.dispose();
      engine.current = null;
    };
  }, [scene, k, wrapRef, canvasRef]);

  return { engine, noCanvas, hover, setHover, pin, setPin };
}

function StarCard({ s, topics, k, extra }: { s: Star; topics: Topic[]; k: number; extra: boolean }) {
  return (
    <>
      <strong>{s.label}</strong>
      <span style={{ color: topicColor(s.topic, k) }}>{topics.find((t) => t.id === s.topic)?.name ?? ""}</span>
      <span>
        {s.day} · {formatDuration(s.seconds)}
        {s.ticket ? ` · ${s.ticket}` : ""}
      </span>
      {extra && <span>Newness {Math.round(s.novelty * 100)}%</span>}
    </>
  );
}

function StarLegend({ topics, k, engine }: { topics: Topic[]; k: number; engine: RefObject<Engine | null> }) {
  return (
    <div className="sx-star-legend">
      {topics.map((t) => (
        <button
          key={t.id}
          type="button"
          className="sx-star-chip"
          onClick={() => engine.current?.flyTopic(t.id)}
          onMouseEnter={() => engine.current?.hotTopic(t.id)}
          onMouseLeave={() => engine.current?.hotTopic(null)}
          onFocus={() => engine.current?.hotTopic(t.id)}
          onBlur={() => engine.current?.hotTopic(null)}
        >
          <span className="sx-star-swatch" style={{ background: topicColor(t.id, k) }} />
          {t.name}
        </button>
      ))}
    </div>
  );
}

export default function StarField({ report }: { report: TopicsReport }) {
  const scene = useMemo(() => buildScene(report), [report]);
  const k = report.topics.length;
  const hintId = useId();
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { engine, noCanvas, hover, setHover, pin, setPin } = useStarEngine(scene, k, wrapRef, canvasRef);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      setPin(null);
      setHover(null);
      engine.current?.unpin();
      engine.current?.release();
      return e.currentTarget.blur();
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const key = keyOf(e.key);
    if (key === "Shift") return void engine.current?.key(key, true);
    if (!MOVE_KEYS.has(key) && key !== "r") return;
    e.preventDefault();
    engine.current?.key(key, true);
  };
  const pinned = pin === null ? null : (scene.stars.find((st) => st.id === pin) ?? null);
  const hoveredStar = hover ? scene.stars[hover.i] : undefined;
  const hovered = hoveredStar && hoveredStar.id !== pin ? hoveredStar : null;

  return (
    <div ref={wrapRef} className="sx-star" tabIndex={0} aria-describedby={hintId} onKeyDown={onKeyDown} onKeyUp={(e) => engine.current?.key(keyOf(e.key), false)} onBlur={() => engine.current?.release()}>
      <canvas ref={canvasRef} role="img" aria-label={`3-D star field: ${report.points.length} blocks in ${k} topics`} />
      {noCanvas && <p className="sx-star-fallback">Star field needs canvas support.</p>}
      <StarLegend topics={report.topics} k={k} engine={engine} />
      <p id={hintId} className="sx-star-hint">
        {HINT}
      </p>
      {hovered && hover && (
        <div className="sx-star-card" style={{ left: hover.x + 12, top: hover.y + 12 }} aria-hidden="true">
          <StarCard s={hovered} topics={report.topics} k={k} extra={false} />
        </div>
      )}
      {pinned && (
        <div className="sx-star-panel">
          <StarCard s={pinned} topics={report.topics} k={k} extra />
          <button type="button" className="sx-star-close" aria-label="Close" onClick={() => (setPin(null), engine.current?.unpin())}>
            ×
          </button>
        </div>
      )}
    </div>
  );
}
