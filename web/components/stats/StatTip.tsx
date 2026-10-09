"use client";

// One rich hover card for every stats graphic. A mark opts in by spreading
// tipProps({...}) (a data-stip JSON attribute). TipLayer, wrapped once around
// the page, finds the nearest [data-stip] under the pointer / keyboard focus /
// tap and shows the card near it. Marks should drop their native <title> so
// the browser's grey tooltip doesn't double up.

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type React from "react";
import "@/app/stats/tip.css";
import { parseTip, placeTip, type Tip } from "./tip";

function TipCard({ tip }: { tip: Tip }) {
  const pct = tip.bar && tip.bar.max > 0 ? Math.max(0, Math.min(1, tip.bar.value / tip.bar.max)) : 0;
  return (
    <>
      <div className="stip-head">
        {tip.accent && <span className="stip-swatch" style={{ background: tip.accent }} />}
        <span className="stip-title">{tip.title}</span>
      </div>
      {tip.sub && <div className="stip-sub">{tip.sub}</div>}
      {tip.rows && tip.rows.length > 0 && (
        <dl className="stip-rows">
          {tip.rows.map(([k, v], i) => (
            <div key={i} className="stip-row">
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
        </dl>
      )}
      {tip.bar && (
        <div className="stip-bar">
          <div className="stip-bar-track">
            <div className="stip-bar-fill" style={{ width: `${pct * 100}%`, background: tip.accent ?? "var(--sage)" }} />
          </div>
          {tip.bar.label && <div className="stip-bar-label">{tip.bar.label}</div>}
        </div>
      )}
      {tip.note && <div className="stip-note">{tip.note}</div>}
    </>
  );
}

const centre = (el: Element) => {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
};

type Show = (el: Element | null, x: number, y: number) => void;
type Ref<T> = { current: T };

/** Pointer / touch / focus handlers: one tip open for the nearest [data-stip]. */
function tipHandlers(show: Show, place: () => void, anchor: Ref<Element | null>, point: Ref<{ x: number; y: number }>) {
  const near = (e: { target: EventTarget }) => (e.target as Element).closest("[data-stip]");
  // Layers can nest (DayFlow carries its own for the day page): the innermost
  // layer claims the event so only one card opens.
  const claim = (e: React.SyntheticEvent) => {
    const n = e.nativeEvent as Event & { stipClaimed?: boolean };
    if (n.stipClaimed) {
      // an inner layer owns this pointer now: drop our own card
      if (anchor.current) show(null, 0, 0);
      return false;
    }
    n.stipClaimed = true;
    return true;
  };
  return {
    onPointerMove: (e: React.PointerEvent) => {
      if (!claim(e)) return;
      if (e.pointerType === "touch") return;
      const el = near(e);
      if (el !== anchor.current) return show(el, e.clientX, e.clientY);
      if (!el) return;
      point.current = { x: e.clientX, y: e.clientY };
      place();
    },
    onPointerLeave: (e: React.PointerEvent) => {
      if (!claim(e)) return;
      if (e.pointerType !== "touch") show(null, 0, 0);
    },
    onPointerDown: (e: React.PointerEvent) => {
      if (!claim(e)) return;
      if (e.pointerType !== "touch") return;
      const el = near(e);
      if (el && el === anchor.current) show(null, 0, 0);
      else show(el, e.clientX, e.clientY);
    },
    onFocus: (e: React.FocusEvent) => {
      if (!claim(e)) return;
      const el = near(e);
      if (!el) return;
      const c = centre(el);
      show(el, c.x, c.y);
    },
    onBlur: () => show(null, 0, 0),
  };
}

/** Open tip, its position and the card ref. */
function useTip() {
  const [tip, setTip] = useState<Tip | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: -9999, top: -9999 });
  const card = useRef<HTMLDivElement>(null);
  const anchor = useRef<Element | null>(null);
  const point = useRef({ x: 0, y: 0 });
  const scrollAt = useRef({ x: 0, y: 0 });

  const place = useCallback(() => {
    const el = card.current;
    if (!el) return;
    const { x, y } = point.current;
    setPos(placeTip(x, y, el.offsetWidth, el.offsetHeight, window.innerWidth, window.innerHeight));
  }, []);

  const show = useCallback((el: Element | null, x: number, y: number) => {
    const t = parseTip(el?.getAttribute("data-stip"));
    anchor.current = t ? el : null;
    point.current = { x, y };
    scrollAt.current = { x: window.scrollX, y: window.scrollY };
    setTip(t);
  }, []);

  // Re-measure once the new content has rendered.
  useEffect(() => {
    if (tip) place();
  }, [tip, place]);

  useEffect(() => {
    const hide = (e: KeyboardEvent) => {
      if (e.key === "Escape") show(null, 0, 0);
    };
    // Close only on a real scroll: some browsers (and automation) fire
    // late or zero-distance scroll events that would snap the card shut.
    const away = () => {
      const s0 = scrollAt.current;
      if (Math.abs(window.scrollY - s0.y) > 4 || Math.abs(window.scrollX - s0.x) > 4) show(null, 0, 0);
    };
    window.addEventListener("keydown", hide);
    window.addEventListener("scroll", away, { passive: true });
    return () => {
      window.removeEventListener("keydown", hide);
      window.removeEventListener("scroll", away);
    };
  }, [show]);

  const handlers = tipHandlers(show, place, anchor, point);
  return { tip, pos, card, handlers };
}

export function TipLayer({ children }: { children: ReactNode }) {
  const { tip, pos, card, handlers } = useTip();
  return (
    <div className="stip-layer" {...handlers}>
      {children}
      <div
        ref={card}
        className="stip-card"
        role="tooltip"
        aria-hidden={tip ? undefined : true}
        data-open={tip ? "" : undefined}
        style={{ left: pos.left, top: pos.top }}
      >
        {tip && <TipCard tip={tip} />}
      </div>
    </div>
  );
}
