"use client";

import "@/app/stats/starfield.css";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { formatDuration } from "@/lib/format";
import type { TopicsReport } from "@/lib/topics_contract";
import { type Engine, type Hover, createEngine } from "./starfieldEngine";
import { buildScene, type Camera } from "./starfieldScene";
import { topicColor } from "./topicColor";

const HINT = "Click the field, then W A S D to fly · drag to look · Shift faster · R reset · Esc leave";
const MOVE_KEYS = new Set(["w", "a", "s", "d", "e", "q", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);

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
  // Camera survives a report refresh (AI names arriving) when the topic count is unchanged.
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
    const eng = createEngine(wrap, canvas, ctx, scene, k, { onHover: setHover, onPin: setPin }, { camera });
    engine.current = eng;
    return () => {
      kept.current = { cam: eng.camera(), k };
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
