import "@/app/stats/terrain.css";
import { useId } from "react";
import { byDepth, iso, isoBox, tones } from "./iso";
import { WEEKDAYS, WEEKDAYS_LONG } from "./time-utils";
import { tipProps, type Tip } from "./tip";

const U = 18; // px per grid step
const MAXH = 6; // tallest pillar, grid units
const OX = Math.round(7 * 0.866 * U + 48); // room for the weekday labels left of the field
const OY = 10 * U;
export const W = Math.round(OX + 25.5 * 0.866 * U + 8);
const H = Math.round(OY + 15.8 * U + 16);
export const PLATE_W = 118;
const PLATE_H = 20;

const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
const cell = (grid: number[][], d: number, h: number) => {
  const v = grid[d]?.[h];
  return typeof v === "number" && Number.isFinite(v) && v > 0 ? v : 0;
};

const nf = new Intl.NumberFormat("en-US");
const events = (n: number) => `${nf.format(n)} ${n === 1 ? "event" : "events"}`;
const pctOf = (n: number, total: number) => (total > 0 ? Math.round((n / total) * 100) : 0);
const hourRange = (h: number) => `${hh(h)}–${hh((h + 1) % 24)}`;

/** Hour-of-day band as a fun noun phrase. */
export function hourBand(h: number): string {
  if (h < 5) return "Night owl hour";
  if (h < 8) return "Before-coffee hour";
  if (h < 12) return "Morning grind hour";
  if (h < 13) return "Lunch hour";
  if (h < 17) return "Afternoon stretch hour";
  if (h < 20) return "After-work hour";
  return "Late-evening hour";
}

/** Rich tip for one weekday-hour slot (grid[d][h]). */
export function terrainTip(grid: number[][], d: number, h: number): Tip {
  const v = cell(grid, d, h);
  const peak = peakOf(grid);
  const total = shares(grid).total;
  const title = `${WEEKDAYS_LONG[d]}s · ${hourRange(h)}`;
  if (v === 0) return { title, sub: "Nothing happened here", rows: [["Events", "0"]], note: "An empty slot: the quiet part of the week.", accent: "var(--bg-sunk)" };
  const all: number[] = [];
  for (let i = 0; i < 7; i++) for (let j = 0; j < 24; j++) all.push(cell(grid, i, j));
  const rank = all.filter((x) => x > v).length + 1;
  const active = all.filter((x) => x > 0);
  const minActive = Math.min(...active);
  let dayTotal = 0;
  let dayBest = 0;
  for (let j = 0; j < 24; j++) {
    dayTotal += cell(grid, d, j);
    if (cell(grid, d, j) > cell(grid, d, dayBest)) dayBest = j;
  }
  const max = peak?.v ?? v;
  let note: string;
  if (peak && peak.d === d && peak.h === h) note = "Your summit: the busiest weekday-hour of them all.";
  else if (active.length > 1 && v === minActive) note = "Quietest weekday-hour you still showed up for.";
  else note = `${hourBand(h)}: ${pctOf(v, max)}% of your peak.`;
  return {
    title,
    sub: `#${rank} of 168 weekday-hours`,
    rows: [
      ["Events this hour", nf.format(v)],
      ["Share of all activity", `${pctOf(v, total)}%`],
      ["Rank", `${rank} of 168`],
      [`Busiest hour on ${WEEKDAYS_LONG[d]}s`, `${hh(dayBest)} (${nf.format(cell(grid, d, dayBest))})`],
      [`Avg hour on ${WEEKDAYS_LONG[d]}s`, `${(dayTotal / 24).toFixed(1)} events`],
    ],
    bar: { value: v, max, label: `${pctOf(v, max)}% of the peak slot` },
    note,
    accent: rampColor(v / max),
  };
}

/** Tip for a weekday label: that weekday's total and peak hour. */
export function weekdayTip(grid: number[][], d: number): Tip {
  const total = shares(grid).total;
  let dayTotal = 0;
  let best = 0;
  for (let h = 0; h < 24; h++) {
    dayTotal += cell(grid, d, h);
    if (cell(grid, d, h) > cell(grid, d, best)) best = h;
  }
  const rows: [string, string][] = [
    ["Events", nf.format(dayTotal)],
    ["Share of all activity", `${pctOf(dayTotal, total)}%`],
  ];
  if (dayTotal === 0) return { title: `${WEEKDAYS_LONG[d]}s`, sub: "No activity", rows, note: "A day off, as far as the data knows." };
  rows.push(["Peak hour", `${hh(best)} (${events(cell(grid, d, best))})`]);
  return {
    title: `${WEEKDAYS_LONG[d]}s`,
    sub: "Whole weekday, all hours",
    rows,
    bar: { value: dayTotal, max: total, label: `${pctOf(dayTotal, total)}% of the week's activity` },
    note: `${hourBand(best)} is when ${WEEKDAYS_LONG[d]}s peak.`,
  };
}

/** Tip for the sun (06-12) and moon (20-24) glyphs. */
export function bandTip(grid: number[][], kind: "sun" | "moon"): Tip {
  const [from, to] = kind === "sun" ? [6, 12] : [20, 24];
  let n = 0;
  for (let d = 0; d < 7; d++) for (let h = from; h < to; h++) n += cell(grid, d, h);
  const total = shares(grid).total;
  const p = pctOf(n, total);
  return {
    title: kind === "sun" ? "Daytime" : "Night",
    sub: `${hh(from)} to ${hh(to % 24)}`,
    rows: [
      ["Events", nf.format(n)],
      ["Share of all activity", `${p}%`],
    ],
    bar: { value: n, max: total, label: `${p}% of everything` },
    note: total === 0 ? "Nothing to compare yet." : kind === "sun" ? `${p}% of your activity happens in the morning light.` : `${p}% of your activity happens after dark.`,
    accent: kind === "sun" ? "var(--amber)" : "var(--violet)",
  };
}

export interface Peak {
  d: number;
  h: number;
  v: number;
}

/** Busiest weekday/hour cell (first one wins ties); null when the grid is empty. */
export function peakOf(grid: number[][]): Peak | null {
  let best: Peak | null = null;
  for (let d = 0; d < 7; d++)
    for (let h = 0; h < 24; h++) {
      const v = cell(grid, d, h);
      if (v > 0 && (!best || v > best.v)) best = { d, h, v };
    }
  return best;
}

export interface Shares {
  total: number;
  /** Whole percent of events at 20:00 or later. */
  nightPct: number;
  /** Whole percent of events before 08:00. */
  earlyPct: number;
}

export function shares(grid: number[][]): Shares {
  let total = 0;
  let night = 0;
  let early = 0;
  for (let d = 0; d < 7; d++)
    for (let h = 0; h < 24; h++) {
      const v = cell(grid, d, h);
      total += v;
      if (h >= 20) night += v;
      if (h < 8) early += v;
    }
  const pct = (n: number) => (total > 0 ? Math.round((n / total) * 100) : 0);
  return { total, nightPct: pct(night), earlyPct: pct(early) };
}

/** Pillar height in grid units: sqrt scale for drama, capped at MAXH. */
export const heightOf = (v: number, max: number) => (v > 0 && max > 0 ? Math.min(MAXH, MAXH * Math.sqrt(v / max)) : 0);

/** slate (quiet) -> sage (steady) -> amber (peak) by share of the peak. */
export function rampColor(share: number): string {
  const s = Math.min(1, Math.max(0, share));
  return s < 0.5
    ? `color-mix(in oklch, var(--sage) ${Math.round(s * 200)}%, var(--slate))`
    : `color-mix(in oklch, var(--amber) ${Math.round((s - 0.5) * 200)}%, var(--sage))`;
}

/** Plate for the peak label: x clamped inside the viewBox, y kept above the flag. */
export function labelPlate(px: number, py: number) {
  const x = Math.min(W - PLATE_W - 4, Math.max(4, px + 24 - PLATE_W / 2));
  return { x, y: Math.max(4, py - 46) };
}

const P = (x: number, y: number, z = 0): [number, number] => {
  const [a, b] = iso(x, y, z, U);
  return [OX + a, OY + b];
};
const quad = (x0: number, x1: number, y0: number, y1: number, z = 0) =>
  [P(x0, y0, z), P(x1, y0, z), P(x1, y1, z), P(x0, y1, z)].map((p) => p.map((n) => n.toFixed(1)).join(",")).join(" ");

function Sun({ x, tip }: { x: number; tip: Tip }) {
  const [cx, cy] = P(x, -1, 7);
  const rays = Array.from({ length: 8 }, (_, i) => (i * Math.PI) / 4);
  return (
    <g className="sx-terrain-ink art-fade sx-terrain-tipped" style={{ stroke: "var(--amber)", animationDelay: "900ms" }} {...tipProps(tip)} tabIndex={0} role="img" aria-label={`${tip.title}: ${tip.rows?.[1]?.[1]} of activity`}>
      <circle cx={cx} cy={cy} r={10} style={{ fill: "transparent", stroke: "none" }} />
      <circle cx={cx} cy={cy} r={3.2} />
      {rays.map((a, i) => (
        <line key={i} x1={cx + Math.cos(a) * 5.4} y1={cy + Math.sin(a) * 5.4} x2={cx + Math.cos(a) * 7.4} y2={cy + Math.sin(a) * 7.4} />
      ))}
    </g>
  );
}

function Moon({ x, tip }: { x: number; tip: Tip }) {
  const [cx, cy] = P(x, -1, 7);
  const q = (dx: number, dy: number) => `${(cx + dx).toFixed(1)},${(cy + dy).toFixed(1)}`;
  return (
    <g className="sx-terrain-ink art-fade sx-terrain-tipped" style={{ stroke: "var(--violet)", animationDelay: "900ms" }} {...tipProps(tip)} tabIndex={0} role="img" aria-label={`${tip.title}: ${tip.rows?.[1]?.[1]} of activity`}>
      <circle cx={cx} cy={cy} r={10} style={{ fill: "transparent", stroke: "none" }} />
      <path d={`M${q(2, -5)} A5.4 5.4 0 1 0 ${q(5, 2)} A4.2 4.2 0 0 1 ${q(2, -5)}Z`} />
    </g>
  );
}

function Climber({ x, y, z, delay }: { x: number; y: number; z: number; delay: number }) {
  const [px, py] = P(x, y, z);
  return (
    <g className="art-fade" style={{ animationDelay: `${delay}ms` }}>
      <line x1={px + 3.5} y1={py - 3} x2={px + 3.5} y2={py - 15} style={{ stroke: "var(--fg)", strokeWidth: 1.2 }} />
      <polygon points={`${px + 3.5},${py - 15} ${px + 10},${py - 12.5} ${px + 3.5},${py - 10}`} style={{ fill: "var(--terracotta)" }} />
      <rect x={px - 2.2} y={py - 6.2} width={4.4} height={6.2} rx={2.2} style={{ fill: "var(--fg)" }} />
      <circle cx={px} cy={py - 9} r={2.3} style={{ fill: "var(--fg)" }} />
    </g>
  );
}

function PeakLabel({ peak }: { peak: Peak }) {
  const [px, py] = P(peak.h + 0.5, peak.d + 0.5, heightOf(peak.v, peak.v));
  const pl = labelPlate(px, py);
  const tx = Math.min(pl.x + PLATE_W - 6, Math.max(pl.x + 6, px + 3.5));
  return (
    <g className="art-fade" style={{ animationDelay: "800ms" }}>
      <path className="sx-terrain-leader" d={`M${(px + 3.5).toFixed(1)},${(py - 15).toFixed(1)} L${tx.toFixed(1)},${pl.y + PLATE_H}`} />
      <rect className="sx-terrain-plate" x={pl.x} y={pl.y} width={PLATE_W} height={PLATE_H} rx={4} />
      <text className="art-label sx-terrain-plate-text" x={pl.x + PLATE_W / 2} y={pl.y + 14} textAnchor="middle" style={{ fill: "var(--fg)" }}>
        {`${WEEKDAYS[peak.d]} ${hh(peak.h)} · ${peak.v}`}
      </text>
    </g>
  );
}

function Pillars({ grid, max }: { grid: number[][]; max: number }) {
  const uid = useId();
  const cells = Array.from({ length: 7 * 24 }, (_, i) => ({ x: i % 24, y: Math.floor(i / 24) })).sort(byDepth);
  return (
    <>
      {cells.map(({ x, y }) => {
        const v = cell(grid, y, x);
        const key = `${uid}${x}-${y}`;
        const label = `${WEEKDAYS[y]} ${hh(x)}`;
        if (v === 0)
          return (
            <polygon key={key} className="sx-terrain-face sx-terrain-tipped" points={quad(x + 0.06, x + 0.94, y + 0.06, y + 0.94)} style={{ fill: "var(--bg-sunk)" }} {...tipProps(terrainTip(grid, y, x))} />
          );
        const t = tones(rampColor(v / max));
        const b = isoBox(x + 0.06, y + 0.06, 0, 0.88, 0.88, heightOf(v, max), U);
        return (
          <g key={key} transform={`translate(${OX} ${OY})`} className="sx-terrain-tipped" {...tipProps(terrainTip(grid, y, x))} tabIndex={0} role="img" aria-label={`${label} · ${events(v)}`}>
            <g className="art-grow-y sx-terrain-pillar" style={{ animationDelay: `${x * 28 + y * 14}ms` }}>
              <polygon className="sx-terrain-face" points={b.left} style={{ fill: t.left }} />
              <polygon className="sx-terrain-face" points={b.right} style={{ fill: t.right }} />
              <polygon className="sx-terrain-face" points={b.top} style={{ fill: t.top }} />
            </g>
          </g>
        );
      })}
    </>
  );
}

function Axes({ peakDay, grid }: { peakDay: number | undefined; grid: number[][] }) {
  return (
    <>
      {[0, 3, 6, 9, 12, 15, 18, 21].map((h) => {
        const [lx, ly] = P(h + 0.5, 7.9);
        return (
          <text key={h} className="art-label" x={lx} y={ly + 8} textAnchor="middle">
            {String(h).padStart(2, "0")}
          </text>
        );
      })}
      {WEEKDAYS.map((d, i) => {
        const [lx, ly] = P(-1.6, i + 0.5);
        return (
          <text key={d} className="art-label sx-terrain-tipped" x={lx} y={ly + 3} textAnchor="end" style={peakDay === i ? { fill: "var(--fg)" } : undefined} {...tipProps(weekdayTip(grid, i))} tabIndex={0} aria-label={`${WEEKDAYS_LONG[i]}s`}>
            {d}
          </text>
        );
      })}
    </>
  );
}

export function WeekTerrain({ grid }: { grid: number[][] }) {
  const peak = peakOf(grid);
  const sh = shares(grid);
  const aria = peak
    ? `Terrain of activity by weekday and hour. Peak ${WEEKDAYS[peak.d]} ${hh(peak.h)} with ${peak.v} events; ${sh.nightPct}% after 20:00, ${sh.earlyPct}% before 08:00.`
    : "Terrain of activity by weekday and hour: nothing built yet.";

  return (
    <div className="sx-terrain-card">
      <p className="sx-terrain-big">{peak ? `Your peak: ${WEEKDAYS_LONG[peak.d]}s at ${hh(peak.h)}` : "Nothing built yet"}</p>
      {peak && (
        <p className="sx-terrain-sub">
          Night owl: <b>{sh.nightPct}%</b> of activity after 20:00; early bird: <b>{sh.earlyPct}%</b> before 08:00
        </p>
      )}
      <svg className="sx-terrain-svg" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={aria}>
        <polygon className="sx-terrain-ground" points={quad(-0.4, 24.4, -0.4, 7.4)} />
        <polygon points={quad(6, 12, -1.1, -0.1)} style={{ fill: "var(--amber)", opacity: 0.22 }} />
        <polygon points={quad(20, 24, -1.1, -0.1)} style={{ fill: "var(--violet)", opacity: 0.22 }} />
        <Pillars grid={grid} max={peak?.v ?? 0} />
        <Axes peakDay={peak?.d} grid={grid} />
        <Sun x={9} tip={bandTip(grid, "sun")} />
        <Moon x={22} tip={bandTip(grid, "moon")} />
        {peak && <Climber x={peak.h + 0.5} y={peak.d + 0.5} z={heightOf(peak.v, peak.v)} delay={700} />}
        {peak && <PeakLabel peak={peak} />}
        {!peak && (
          <text className="sx-terrain-empty" x={W / 2} y={OY + 7.5 * U}>
            nothing built yet
          </text>
        )}
      </svg>
      <p className="sx-terrain-legend">
        <span>Height = events per hour</span>
        <span><i style={{ background: "var(--slate)" }} />quiet</span>
        <span><i style={{ background: "var(--sage)" }} />steady</span>
        <span><i style={{ background: "var(--amber)" }} />peak</span>
      </p>
    </div>
  );
}
