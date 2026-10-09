"use client";

import { useState, type KeyboardEvent, type PointerEvent } from "react";
import { formatDuration } from "@/lib/format";
import { chartModel, type ChartModel } from "@/lib/progress";
import type { TicketProgress } from "@/lib/types";

const W = 260;
const X0 = 22;
const X1 = 236;
const Y_BASE = 84;
const Y_TOP = 17.3;
const TIP_W = 190;

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** "Tue 6" for "2026-10-06"; UTC so the label never shifts with the browser zone. */
function dayLabel(iso: string): string {
  const d = new Date(`${iso}T00:00:00Z`);
  return `${WEEKDAYS[d.getUTCDay()]} ${d.getUTCDate()}`;
}

const nameOf = (s: { name: string; is_you: boolean }) => (s.is_you ? "You" : s.name);
const pts = (xs: number[], ys: number[]) => xs.map((x, i) => `${x.toFixed(1)},${ys[i].toFixed(1)}`).join(" ");

export function ProgressChart({ ticket, pending }: { ticket: TicketProgress; pending: number }) {
  const m = chartModel(ticket);
  const [hover, setHover] = useState<number | null>(null);
  if (!m) return null;

  const n = m.days.length;
  const xs = m.days.map((_, i) => (n > 1 ? X0 + (i * (X1 - X0)) / (n - 1) : X1));
  const last = n - 1;
  const totals = m.days.map((_, i) => m.series.reduce((a, s) => a + s.values[i], 0));

  const show = (i: number) => setHover(Math.min(last, Math.max(0, i)));
  const nearest = (e: PointerEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - box.left) / box.width) * W;
    show(xs.reduce((best, v, i) => (Math.abs(v - x) < Math.abs(xs[best] - x) ? i : best), 0));
  };
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const at = hover ?? last;
    if (e.key === "ArrowLeft") show(at - 1);
    else if (e.key === "ArrowRight") show(at + 1);
    else if (e.key === "Escape") setHover(null);
    else return;
    e.preventDefault();
  };

  const label = `Running total of hours on ${ticket.key}: ${m.days
    .map((d, i) => `${dayLabel(d)} ${formatDuration(totals[i])}`)
    .join(", ")}; estimate ${formatDuration(m.estimate)}`;

  return (
    <figure className="ep-burn">
      <figcaption className="ep-burn-title">Running total on this ticket</figcaption>
      <svg
        viewBox={`0 0 ${W} 104`}
        role="group"
        aria-roledescription="chart"
        aria-label={label}
        tabIndex={0}
        onPointerMove={nearest}
        onPointerDown={nearest}
        onPointerLeave={() => setHover(null)}
        onFocus={() => show(hover ?? last)}
        onBlur={() => setHover(null)}
        onKeyDown={onKey}
      >
        <Plot m={m} xs={xs} totals={totals} pending={pending} hover={hover} />
      </svg>
      {hover !== null && <Tip m={m} i={hover} x={xs[hover]} pending={hover === last ? pending : 0} />}
      <DataTable m={m} pending={pending} />
    </figure>
  );
}

type PlotProps = { m: ChartModel; xs: number[]; totals: number[]; pending: number; hover: number | null };

function Plot({ m, xs, totals, pending, hover }: PlotProps) {
  const n = xs.length;
  const grand = totals[n - 1] + pending;
  const ymax = Math.max(m.estimate, grand);
  const y = (v: number) => Y_BASE - (v / ymax) * (Y_BASE - Y_TOP);
  const over = grand > m.estimate;

  // Stack bottom-up: each series sits on the running sum of those before it.
  let floor = xs.map(() => 0);
  const bands = m.series.map((s) => {
    const lo = floor;
    const hi = s.values.map((v, i) => lo[i] + v);
    floor = hi;
    return { lo: lo.map(y), hi: hi.map(y) };
  });

  return (
    <>
      {over && <rect className="ep-over-band" x={X0} y={8} width={X1 - X0} height={y(m.estimate) - 8} />}
      <line className="ep-grid-l" x1={X0} x2={X1} y1={Y_BASE} y2={Y_BASE} />
      {bands.map((b, i) => (
        <g key={m.series[i].name} data-p={i}>
          <polygon className="ep-area" points={`${pts(xs, b.hi)} ${pts([...xs].reverse(), [...b.lo].reverse())}`} />
          <polyline className="ep-edge" points={pts(xs, b.hi)} />
        </g>
      ))}
      {pending > 0 && (
        <>
          <line className="ep-pend-line" x1={X1} x2={X1} y1={y(totals[n - 1])} y2={y(grand)} />
          <circle className="ep-pend-dot" cx={X1} cy={y(grand)} r={3} />
        </>
      )}
      <line className="ep-est-line" x1={X0} x2={X1} y1={y(m.estimate)} y2={y(m.estimate)} />
      <text className="ep-est-t" x={X0 - 4} y={y(m.estimate) + 3} textAnchor="end">
        {formatDuration(m.estimate)}
      </text>
      <text className="ep-axis-t" x={xs[0]} y={98} textAnchor={n > 1 ? "start" : "middle"}>
        {dayLabel(m.days[0])}
      </text>
      {n > 1 && (
        <text className="ep-axis-t" x={xs[n - 1]} y={98} textAnchor="end">
          {dayLabel(m.days[n - 1])}
        </text>
      )}
      {hover !== null && <line className="ep-cross" x1={xs[hover]} x2={xs[hover]} y1={8} y2={Y_BASE} />}
    </>
  );
}

function Tip({ m, i, x, pending }: { m: ChartModel; i: number; x: number; pending: number }) {
  const all = m.series.reduce((a, s) => a + s.values[i], 0) + pending;
  const state = all > m.estimate ? `${formatDuration(all - m.estimate)} over` : `${formatDuration(m.estimate - all)} left`;
  return (
    <div className="ep-tip" aria-hidden="true" style={{ left: `clamp(0px, calc(${((x / W) * 100).toFixed(1)}% - ${TIP_W / 2}px), calc(100% - ${TIP_W}px))` }}>
      <b>Total by end of {dayLabel(m.days[i])}</b>
      {m.series.map((s) => (
        <p key={s.name}>
          <span>{nameOf(s)}</span>
          <span>{formatDuration(s.values[i])}</span>
        </p>
      ))}
      {pending > 0 && (
        <p>
          <span>Not in Tempo yet</span>
          <span>+{formatDuration(pending)}</span>
        </p>
      )}
      <p className="ep-tot">
        <span>{`${formatDuration(all)} of ${formatDuration(m.estimate)}`}</span>
        <span>{state}</span>
      </p>
    </div>
  );
}

function DataTable({ m, pending }: { m: ChartModel; pending: number }) {
  return (
    <details className="ep-tbl">
      <summary>Show as table</summary>
      <table>
        <thead>
          <tr>
            <th>Total by end of</th>
            {m.series.map((s) => (
              <th key={s.name}>{nameOf(s)}</th>
            ))}
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {m.days.map((d, i) => (
            <tr key={d}>
              <td>{dayLabel(d)}</td>
              {m.series.map((s) => (
                <Cell key={s.name} v={s.values[i]} />
              ))}
              <Cell v={m.series.reduce((a, s) => a + s.values[i], 0)} />
            </tr>
          ))}
          {pending > 0 && (
            <tr>
              <td>After sync</td>
              {m.series.map((s) => (
                <Cell key={s.name} v={s.values[m.days.length - 1] + (s.is_you ? pending : 0)} />
              ))}
              <Cell v={m.series.reduce((a, s) => a + s.values[m.days.length - 1], 0) + pending} />
            </tr>
          )}
        </tbody>
      </table>
    </details>
  );
}

const Cell = ({ v }: { v: number }) => <td>{formatDuration(v)}</td>;
