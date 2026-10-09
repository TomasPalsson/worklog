import { useId } from "react";

const HALF = 1800;
const W = 60; // rail length in px; the pencil notch lives in the 8px after it
const X0 = 1;

/** Scale max = max(tracked, billed) up to the next half hour, at least 1h. */
export function railMath(tracked: number, billed: number) {
  const max = Math.max(3600, Math.ceil(Math.max(tracked, billed, 0) / HALF) * HALF);
  const px = (s: number) => (Math.max(0, s) / max) * W;
  return { max, trackedW: px(tracked), billedW: px(billed), ticks: max / HALF - 1 };
}

function hm(seconds: number): string {
  const m = Math.round(seconds / 60);
  const h = Math.floor(m / 60);
  return h && m % 60 ? `${h}h ${m % 60}m` : h ? `${h}h` : `${m}m`;
}

export function railLabel(tracked: number, billed: number, overridden: boolean): string {
  const base = `Tracked ${hm(tracked)}, billed ${hm(billed)}`;
  const diff = billed - tracked;
  const how = overridden ? ", set by hand" : "";
  if (diff > 0) return `${base}, rounded up ${hm(diff)}${how}`;
  if (diff < 0) return `${base}, ${overridden ? "cut" : "rounded down"} ${hm(-diff)}${how}`;
  return `${base}${how}`;
}

/** Tracked vs billed on a half-hour ruler: solid bar, empty bracket for the rounded-up sliver. */
export function RoundingRail({
  tracked,
  billed,
  overridden,
}: {
  tracked: number;
  billed: number;
  overridden: boolean;
}) {
  const hatch = useId();
  const { trackedW, billedW, ticks } = railMath(tracked, billed);
  const cut = tracked > billed;
  return (
    <svg
      className="art-rail"
      width={W + 10}
      height={10}
      viewBox={`0 0 ${W + 10} 10`}
      role="img"
      aria-label={railLabel(tracked, billed, overridden)}
    >
      <defs>
        <pattern id={hatch} width="4" height="4" patternUnits="userSpaceOnUse" patternTransform="rotate(135)">
          <line x1="0" y1="0" x2="0" y2="4" className="art-rail-hatch" />
        </pattern>
      </defs>
      <g className="art-rail-ticks">
        {Array.from({ length: ticks }, (_, i) => {
          const x = X0 + ((i + 1) / (ticks + 1)) * W;
          return <line key={i} x1={x} x2={x} y1={1} y2={9} />;
        })}
        <line x1={X0} x2={X0 + W} y1={9.5} y2={9.5} />
      </g>
      {cut && (
        <rect x={X0 + billedW} y={2.5} width={trackedW - billedW} height={5} fill={`url(#${hatch})`} className="art-rail-cut" />
      )}
      {billedW > 0 && <rect x={X0} y={2.5} width={billedW} height={5} rx={1.5} className="art-rail-billed" />}
      {trackedW > 0 && (
        <rect
          x={X0}
          y={3.5}
          width={cut ? billedW : trackedW}
          height={3}
          rx={1}
          className="art-rail-tracked art-grow"
          style={{ transformOrigin: `${X0}px 5px`, transformBox: "view-box" }}
        />
      )}
      {overridden && (
        <path
          className="art-rail-pencil"
          transform={`translate(${X0 + billedW + 1} 1)`}
          d="M0.5 7.5 L1 5.2 L5.6 0.6 L7.4 2.4 L2.8 7 Z M4.6 1.6 L6.4 3.4"
          fill="none"
          strokeWidth={1.6}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )}
    </svg>
  );
}
