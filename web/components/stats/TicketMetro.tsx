import { useId } from "react";
import "@/app/stats/metro.css";
import type { TicketStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { iso, isoBox, tones } from "./iso";
import { dayLabel, dowMon, MONTHS, parseDay } from "./time-utils";

const DIP_DAYS = 7;
const W = 640;
const XL = 92; // gutter for the line badges
const XR = 16;
const TOP = 34;
const LANE = 40;
const BOT = 14;
const BADGE_H = 16;
const DAY_MS = 86400000;
const DIP = 9;
const DIP_DX = DIP * Math.sqrt(3); // a 30° bend: dx = dy / tan 30°
const MAX_LINES = 10;

const ell = (s: string, n: number) =>
  s.length > n ? `${s.slice(0, Math.max(1, n - 1))}…` : s;
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

/** Days in the inclusive range; 0 when unparsable. */
export function rangeDays(from: string, to: string): number {
  const n =
    Math.round(
      (Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) /
        DAY_MS,
    ) + 1;
  return Number.isFinite(n) && n > 0 ? n : 0;
}

/** Day index of d in the range, clamped to [0, n-1]. */
export function dayIdx(from: string, d: string, n: number): number {
  const i = Math.round(
    (Date.parse(`${d}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / DAY_MS,
  );
  return Number.isFinite(i) ? Math.min(n - 1, Math.max(0, i)) : 0;
}

/** Sorted distinct day indices a ticket has stations on; missing days fall back to first and last day. */
export function stationIdx(
  t: Pick<TicketStat, "first_day" | "last_day" | "days">,
  from: string,
  n: number,
): number[] {
  const src = t.days && t.days.length > 0 ? t.days : [t.first_day, t.last_day];
  return [...new Set(src.map((d) => dayIdx(from, d, n)))].sort((a, b) => a - b);
}

/**
 * Greedy lane assignment: walk lines by start; each takes the first lane whose
 * previous line ended more than `gap` days before it starts. Returns lane per input.
 */
export function assignLanes(
  spans: { start: number; end: number }[],
  gap = 0,
): number[] {
  const order = spans
    .map((_, i) => i)
    .sort(
      (a, b) =>
        spans[a].start - spans[b].start || spans[a].end - spans[b].end || a - b,
    );
  const ends: number[] = [];
  const lane = new Array<number>(spans.length).fill(0);
  for (const i of order) {
    let l = ends.findIndex((e) => e + gap < spans[i].start);
    if (l < 0) l = ends.length;
    ends[l] = spans[i].end;
    lane[i] = l;
  }
  return lane;
}

/** 2..7px, proportional to the ticket's share of the biggest ticket's seconds. */
export function thickness(seconds: number, max: number): number {
  if (!(max > 0) || !Number.isFinite(seconds)) return 3;
  return 3 + 6 * Math.min(1, Math.max(0, seconds / max));
}

/** Even hue spread across n lines. */
export const hueAt = (i: number, n: number) =>
  Math.round(((i * 360) / Math.max(1, n) + 25) % 360);

/** Pixel x of day index i of n. */
export const xOf = (i: number, n: number, x0 = XL, x1 = W - XR) =>
  n <= 1 ? (x0 + x1) / 2 : x0 + (i * (x1 - x0)) / (n - 1);

/**
 * Horizontal run through the station xs at y. Quiet stretches of >= 7 days
 * (by day index idx, when given) dip DIP px with 30° bends and come back up;
 * a dip too narrow to draw (< minGap px) stays flat.
 */
export function linePath(
  xs: number[],
  y: number,
  minGap = 20,
  idx?: number[],
): string {
  if (xs.length === 0) return "";
  let d = `M${xs[0].toFixed(1)} ${y}`;
  for (let k = 1; k < xs.length; k++) {
    const a = xs[k - 1];
    const b = xs[k];
    if (b - a >= minGap && (!idx || idx[k] - idx[k - 1] >= DIP_DAYS)) {
      const p = 6;
      // narrow gaps get a shallower bend, still at 30°
      const dx = Math.min(DIP_DX, (b - a - 2 * p) / 2);
      const dy = Math.min(DIP, dx / Math.sqrt(3));
      d += ` L${(a + p).toFixed(1)} ${y} L${(a + p + dx).toFixed(1)} ${(y + dy).toFixed(1)} L${(b - p - dx).toFixed(1)} ${(y + dy).toFixed(1)} L${(b - p).toFixed(1)} ${y}`;
    }
    d += ` L${b.toFixed(1)} ${y}`;
  }
  return d;
}

/** Month labels (1st of month, plus the first day) kept at least minPx apart. */
export function monthTicks(
  from: string,
  n: number,
  step: number,
  minPx = 30,
): { i: number; label: string }[] {
  const out: { i: number; label: string }[] = [];
  let lastX = -Infinity;
  for (let i = 0; i < n; i++) {
    const d = new Date(parseDay(from).getTime() + i * DAY_MS);
    if (i === 0 || d.getUTCDate() === 1) {
      const x = i * step;
      if (x - lastX >= minPx) {
        out.push({ i, label: MONTHS[d.getUTCMonth()] });
        lastX = x;
      }
    }
  }
  return out;
}

/** Day ticks: every day when there is room, otherwise Mondays. */
export function dayTicks(
  from: string,
  n: number,
  step: number,
): { i: number; major: boolean }[] {
  const out: { i: number; major: boolean }[] = [];
  for (let i = 0; i < n; i++) {
    const day = new Date(parseDay(from).getTime() + i * DAY_MS)
      .toISOString()
      .slice(0, 10);
    const mon = dowMon(day) === 0;
    if (step >= 3 || mon) out.push({ i, major: mon });
  }
  return out;
}

export interface MetroLine {
  t: TicketStat;
  rank: number;
  lane: number;
  y: number;
  xs: number[];
  idx: number[];
  stations: number;
  thick: number;
  hue: number;
  badge: { x: number; w: number };
  labelChars: number;
}

export function layoutLines(
  tickets: TicketStat[],
  from: string,
  n: number,
): { lines: MetroLine[]; lanes: number } {
  const top = [...tickets]
    .sort((a, b) => b.seconds - a.seconds || a.key.localeCompare(b.key))
    .slice(0, MAX_LINES);
  if (top.length === 0 || n === 0) return { lines: [], lanes: 0 };
  const step = (W - XR - XL) / Math.max(1, n - 1);
  const idx = top.map((t) => stationIdx(t, from, n));
  const badgeW = (t: TicketStat) => 14 + Math.min(10, t.key.length) * 6.6;
  const spans = idx.map((a) => ({ start: a[0], end: a[a.length - 1] }));
  const lane = assignLanes(spans, Math.ceil(100 / step));
  const max = top[0].seconds;
  const lines = top.map((t, i): MetroLine => {
    const xs = idx[i].map((k) => xOf(k, n));
    const bw = badgeW(t);
    const nextStart = Math.min(
      W - XR,
      ...top.map((_, j) =>
        j !== i && lane[j] === lane[i] && spans[j].start > spans[i].start
          ? xOf(spans[j].start, n) - 110
          : Infinity,
      ),
    );
    return {
      t,
      rank: i,
      lane: lane[i],
      y: TOP + 20 + lane[i] * LANE,
      xs,
      idx: idx[i],
      stations: idx[i].length,
      thick: thickness(t.seconds, max),
      hue: hueAt(i, top.length),
      badge: { x: Math.max(2, xs[0] - bw - 8), w: bw },
      labelChars: Math.floor((nextStart - xs[0] - 12) / 6.2),
    };
  });
  return { lines, lanes: Math.max(...lane) + 1 };
}

export interface Headline {
  key: string;
  stations: number;
  days: number;
}

/** The ticket with the most stations (ties: more seconds), with its calendar span. */
export function longestRide(lines: MetroLine[]): Headline | null {
  const best = [...lines].sort(
    (a, b) => b.stations - a.stations || b.t.seconds - a.t.seconds,
  )[0];
  if (!best) return null;
  const span =
    Math.round(
      (Date.parse(`${best.t.last_day}T00:00:00Z`) -
        Date.parse(`${best.t.first_day}T00:00:00Z`)) /
        DAY_MS,
    ) + 1;
  return {
    key: best.t.key,
    stations: best.stations,
    days: Math.max(best.stations, Number.isFinite(span) ? span : 1),
  };
}

export const metroAria = (
  from: string,
  to: string,
  lines: MetroLine[],
  h: Headline | null,
) =>
  lines.length === 0 || !h
    ? "Ticket metro map: nothing built yet."
    : `Ticket metro map, ${plural(lines.length, "line")} from ${from} to ${to}. Longest ride ${h.key}: ${plural(h.stations, "station")} over ${plural(h.days, "day")}. Biggest ticket ${lines[0].t.key}, ${formatDuration(lines[0].t.seconds)}.`;

function Terminus({ x, y, color }: { x: number; y: number; color: string }) {
  const u = 4.5;
  const f = isoBox(0, 0, 0, 1, 1, 2.2, u);
  const c = tones(color);
  const [ox, oy] = iso(0.5, 0.5, 0, u);
  return (
    <g
      transform={`translate(${(x - ox).toFixed(2)} ${(y - oy).toFixed(2)})`}
      style={{ pointerEvents: "none" }}
    >
      <polygon points={f.left} fill={c.left} />
      <polygon points={f.right} fill={c.right} />
      <polygon points={f.top} fill={c.top} />
    </g>
  );
}

function Empty() {
  const f = isoBox(-3, -3, 0, 6, 6, 0.5, 12);
  const c = tones("var(--bg-sunk)");
  return (
    <div className="sx-metro">
      <div
        className="sx-metro-empty"
        role="img"
        aria-label={metroAria("", "", [], null)}
      >
        <svg
          viewBox="-90 -10 180 90"
          width="200"
          style={{ maxWidth: "100%" }}
          aria-hidden="true"
        >
          <g transform="translate(0 30)">
            <polygon points={f.left} fill={c.left} />
            <polygon points={f.right} fill={c.right} />
            <polygon points={f.top} fill={c.top} />
          </g>
        </svg>
        <span>nothing built yet</span>
      </div>
    </div>
  );
}

function LineMarks({ l, k, step }: { l: MetroLine; k: number; step: number }) {
  const delay = `${k * 90}ms`;
  const summary = l.t.summary ? ell(l.t.summary, l.labelChars) : "";
  const r = Math.min(4.2, Math.max(1.8, step * 0.42, l.thick * 0.5 + 0.6));
  const last = l.xs.length - 1;
  const named = l.t.days && l.t.days.length === l.xs.length;
  const col = `oklch(var(--project-l) var(--project-c) ${l.hue})`;
  return (
    <g>
      <title>{`${l.t.key}${l.t.summary ? ` · ${l.t.summary}` : ""} · ${formatDuration(l.t.seconds)} · ${plural(l.t.days_active, "day")} active`}</title>
      <path
        className="sx-metro-line"
        d={linePath(l.xs, l.y, undefined, l.idx)}
        pathLength={1}
        fill="none"
        stroke={col}
        strokeWidth={l.thick}
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{
          strokeDasharray: 1,
          strokeDashoffset: 0,
          animationDelay: delay,
        }}
      />
      <g className="art-fade" style={{ animationDelay: `${k * 90 + 500}ms` }}>
        {l.xs.map((x, i) => (
          <circle
            key={i}
            cx={x}
            cy={l.y}
            r={r}
            fill="var(--bg)"
            stroke={col}
            strokeWidth={Math.min(2, r * 0.6)}
          >
            {named ? (
              <title>{`${l.t.key} · ${dayLabel(l.t.days[i])}`}</title>
            ) : null}
          </circle>
        ))}
        <Terminus x={l.xs[0]} y={l.y} color={col} />
        {last > 0 ? <Terminus x={l.xs[last]} y={l.y} color={col} /> : null}
        <rect
          x={l.badge.x}
          y={l.y - BADGE_H / 2}
          width={l.badge.w}
          height={BADGE_H}
          rx={BADGE_H / 2}
          fill="var(--bg)"
          stroke={col}
          strokeWidth={2}
        />
        <text
          x={l.badge.x + l.badge.w / 2}
          y={l.y + 4}
          textAnchor="middle"
          className="art-label"
          style={{ fill: "var(--fg)", fontSize: 11 }}
        >
          {ell(l.t.key, 10)}
        </text>
        {summary.length >= 4 ? (
          <text
            x={l.xs[0] + 9}
            y={l.y - l.thick / 2 - 6}
            className="art-label"
            style={{ fontSize: 11 }}
          >
            {summary}
          </text>
        ) : null}
      </g>
    </g>
  );
}

export function TicketMetro({
  tickets,
  from,
  to,
}: {
  tickets: TicketStat[];
  from: string;
  to: string;
}) {
  const uid = useId();
  const n = rangeDays(from, to);
  const { lines, lanes } = layoutLines(tickets, from, n);
  if (lines.length === 0) return <Empty />;

  const step = (W - XR - XL) / Math.max(1, n - 1);
  const H = TOP + 20 + (lanes - 1) * LANE + DIP + BOT + 8;
  const hd = longestRide(lines)!;

  return (
    <div className="sx-metro">
      <p className="sx-metro-head">
        <b>{hd.key}</b> was your longest ride: <b>{hd.stations}</b>{" "}
        {hd.stations === 1 ? "station" : "stations"} over <b>{hd.days}</b>{" "}
        {hd.days === 1 ? "day" : "days"}
      </p>
      <div className="sx-metro-scroll">
        <svg
          className="sx-metro-svg"
          viewBox={`0 0 ${W} ${H}`}
          style={{ width: "100%", minWidth: 520, maxWidth: W }}
          role="img"
          aria-label={metroAria(from, to, lines, hd)}
        >
          <defs>
            <clipPath id={`${uid}-clip`}>
              <rect x={0} y={0} width={W} height={H} />
            </clipPath>
          </defs>
          <rect
            className="sx-metro-plate"
            x={0}
            y={TOP - 4}
            width={W}
            height={H - TOP + 4}
            rx={4}
          />
          <line
            className="sx-metro-rule"
            x1={XL}
            x2={W - XR}
            y1={TOP - 8}
            y2={TOP - 8}
          />
          {dayTicks(from, n, step).map((t) => (
            <line
              key={t.i}
              className="sx-metro-tick"
              x1={xOf(t.i, n)}
              x2={xOf(t.i, n)}
              y1={TOP - 8}
              y2={TOP - (t.major ? 2 : 4)}
            />
          ))}
          {monthTicks(from, n, step).map((m) => (
            <text
              key={m.i}
              className="art-label"
              x={xOf(m.i, n)}
              y={TOP - 14}
              textAnchor={xOf(m.i, n) > W - XR - 24 ? "end" : "start"}
              style={{ fontSize: 11 }}
            >
              {m.label}
            </text>
          ))}
          <g clipPath={`url(#${uid}-clip)`}>
            {lines.map((l, k) => (
              <LineMarks key={l.t.key} l={l} k={k} step={step} />
            ))}
          </g>
        </svg>
      </div>
      <p className="sx-metro-legend">
        One line per ticket, top {MAX_LINES} by time. Thicker line = more hours,
        dot = a day worked, post = first and last day, dip = a week or more off.
      </p>
    </div>
  );
}
