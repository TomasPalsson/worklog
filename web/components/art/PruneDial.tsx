// 31-day billing-cycle dial (day 1 at top, clockwise). Outer band: current
// cycle from the start day (sage wash). Inner band: the window where the
// month that just ended is still editable (amber hatch, start..close) and
// the stretch after the close day where it is eligible for pruning
// (terracotta wash, close+1..start-1). Redraws from the live form values.

import { useId } from "react";
import { FlagIcon } from "../icons";

const C = 66;
const N = 31;

/** Whole day 1..31 or null. */
export function parseDay(v: string | number): number | null {
  const s = String(v).trim();
  if (!/^\d+$/.test(s)) return null;
  const n = Number(s);
  return n >= 1 && n <= N ? n : null;
}

const pt = (day: number, r: number, edge = 0) => {
  const a = ((day - 1 + edge) / N) * 2 * Math.PI;
  return [C + Math.sin(a) * r, C - Math.cos(a) * r] as const;
};

/** Annular sector covering `len` days from `start` (wraps past day 31). */
export function sectorPath(start: number, len: number, r0: number, r1: number): string {
  if (len <= 0) return "";
  const f = (n: number) => n.toFixed(2);
  const L = Math.min(len, N);
  if (L >= N) {
    return (
      `M${C} ${C - r1}A${r1} ${r1} 0 1 1 ${C} ${C + r1}A${r1} ${r1} 0 1 1 ${C} ${C - r1}Z` +
      `M${C} ${C - r0}A${r0} ${r0} 0 1 0 ${C} ${C + r0}A${r0} ${r0} 0 1 0 ${C} ${C - r0}Z`
    );
  }
  const big = L / N > 0.5 ? 1 : 0;
  const [x0, y0] = pt(start, r1);
  const [x1, y1] = pt(start, r1, L);
  const [x2, y2] = pt(start, r0, L);
  const [x3, y3] = pt(start, r0);
  return `M${f(x0)} ${f(y0)}A${r1} ${r1} 0 ${big} 1 ${f(x1)} ${f(y1)}L${f(x2)} ${f(y2)}A${r0} ${r0} 0 ${big} 0 ${f(x3)} ${f(y3)}Z`;
}

/** Day windows derived from the two inputs; 0 = can't be derived. */
export function windows(start: number | null, close: number | null, today: number) {
  if (start === null) return { cycle: 0, editable: 0, prunable: 0 };
  // elapsed part of the current cycle: start..today, wrapping past day 31
  const cycle = ((today - start + N) % N) + 1;
  // the daemon rejects close < start, so draw no window for it
  if (close === null || close < start) return { cycle, editable: 0, prunable: 0 };
  const editable = close - start + 1; // start..close
  return { cycle, editable, prunable: N - editable }; // close+1..start-1
}

export function PruneDial({
  enabled,
  startDay,
  closeDay,
  today = new Date().getDate(),
}: {
  enabled: boolean;
  startDay: string;
  closeDay: string;
  today?: number;
}) {
  const id = useId();
  const start = parseDay(startDay);
  const close = parseDay(closeDay);
  const w = windows(start, close, today);
  const [fx, fy] = close ? pt(close, 25, 0.5) : [0, 0];
  const [tx, ty] = pt(today, 60, 0.5);
  const label =
    `Billing cycle dial. Pruning ${enabled ? "on" : "off"}.` +
    (start ? ` Cycle starts day ${start}, ${w.cycle} days elapsed.` : "") +
    (close ? ` Month closes on day ${close}.` : "") +
    (start && close
      ? close < start
        ? " Close day is before the start day, so no editable or pruning window is drawn."
        : ` The previous month is editable for ${w.editable} days, then eligible for pruning for ${w.prunable} days.`
      : "") +
    ` Today is day ${today}.`;
  return (
    <div className="art-settings-dialwrap" data-off={!enabled || undefined}>
    <svg
      className="art-settings-dial"
      width="168"
      height="168"
      viewBox="-14 -14 160 160"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      role="img"
      aria-label={label}
    >
      <defs>
        <pattern id={`${id}h`} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(135)">
          <line x1="0" y1="0" x2="0" y2="6" stroke="var(--amber)" strokeWidth="2" />
        </pattern>
      </defs>
      {Array.from({ length: N }, (_, i) => {
        const long = (i + 1) % 5 === 0 || i === 0;
        const [x0, y0] = pt(i + 1, 54, 0.5);
        const [x1, y1] = pt(i + 1, long ? 60 : 57, 0.5);
        return <line key={i} x1={x0} y1={y0} x2={x1} y2={y1} stroke="var(--border-strong)" strokeWidth={1} />;
      })}
      {[1, 8, 15, 22].map((d) => {
        const [x, y] = pt(d, 68, 0.5);
        return (
          <text key={d} className="art-label" x={x} y={y} textAnchor="middle" dominantBaseline="middle" stroke="none">
            {d}
          </text>
        );
      })}
      {w.cycle > 0 && start && (
        <path className="art-fade" d={sectorPath(start, w.cycle, 42, 50)} fill="var(--sage)" fillOpacity={0.24} stroke="var(--sage)" />
      )}
      {w.editable > 0 && start && (
        <path className="art-fade" d={sectorPath(start, w.editable, 31, 39)} fill={`url(#${id}h)`} stroke="var(--amber)" />
      )}
      {w.prunable > 0 && close && (
        <path className="art-fade" d={sectorPath(close === N ? 1 : close + 1, w.prunable, 31, 39)} fill="var(--terracotta)" fillOpacity={0.24} stroke="var(--terracotta)" />
      )}
      {close && (
        <g style={{ color: "var(--fg)" }}>
          <FlagIcon size={12} x={fx - 6} y={fy - 6} />
        </g>
      )}
      {enabled && (
        <g stroke="none" textAnchor="middle">
          <text className="art-settings-dial-today" x={C} y={C + 2} dominantBaseline="middle">{today}</text>
          <text className="art-label" x={C} y={C + 16} dominantBaseline="middle">today</text>
        </g>
      )}
      <circle className="art-settings-today" cx={tx} cy={ty} r={2.6} fill="var(--fg)" stroke="var(--bg)" strokeWidth={1} />
      {!enabled && (
        <text className="art-label" x={C} y={C} textAnchor="middle" dominantBaseline="middle" stroke="none">off</text>
      )}
    </svg>
    {/* the svg's aria-label already says all of this */}
    <ul className="art-settings-legend" aria-hidden="true">
      {w.cycle > 0 && <li data-k="cycle">this cycle so far · {w.cycle} d</li>}
      {w.editable > 0 && <li data-k="edit">last month still editable · days {start}–{close}</li>}
      {w.prunable > 0 && <li data-k="prune">then pruned · {w.prunable} d</li>}
      {close && <li data-k="close">month closes · day {close}</li>}
    </ul>
    </div>
  );
}
