import "@/app/stats/city.css";
import { useId } from "react";
import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { byDepth, iso, isoBox, tones } from "./iso";
import { dayLabel, dowMon, MONTHS, parseDay, shortLabel, WEEKDAYS, WEEKDAYS_LONG } from "./time-utils";
import { tipProps, type Tip } from "./tip";
import { craneTip, flagTip, plotTip } from "./CityTips";

export { craneTip, flagTip, plotTip };

const U = 18; // px per grid unit
const P = 1.8; // plot pitch
const MAX_H = 9;
const MIN_H = 0.3;
const PLINTH = 0.12;
const MAX_WEEKS = 13;
const FLOOR_PITCH = 0.8;

export interface Plot {
  day: string;
  col: number;
  row: number;
  x: number;
  y: number;
  kind: "tower" | "park" | "lot";
  work: number;
  personal: number;
  prompts: number;
  h: number;
  share: number;
  lit: number;
  garden: number;
  busiest: boolean;
  today: boolean;
  stat: DailyStat;
}

/** Shaft height in grid units: tallest day = MAX_H, any work shows at least MIN_H. */
export function towerHeight(work: number, max: number): number {
  if (!(work > 0) || !(max > 0)) return 0;
  return Math.max(MIN_H, (work / max) * MAX_H);
}

/** Whole floors that can carry windows on a shaft of height h. */
export const floorsFor = (h: number) => Math.max(0, Math.floor((h - 0.3 - 0.4) / FLOOR_PITCH) + 1);

/** Lit windows = round(prompts / 10), capped to the 4 slots per floor. */
export const windowCount = (prompts: number, h: number) =>
  Math.max(0, Math.min(Math.round(prompts / 10), 4 * floorsFor(h)));

/** Share of the roof given to the violet garden (personal / all tracked time). */
export const gardenShare = (work: number, personal: number) =>
  personal > 0 ? personal / (work + personal) : 0;

/** Calendar plots: columns Mon..Sun, rows = weeks, oldest at the back; last MAX_WEEKS only. */
export function layoutCity(daily: DailyStat[], today: string): Plot[] {
  if (daily.length === 0) return [];
  const off = dowMon(daily[0].day);
  const lastRow = Math.floor((daily.length - 1 + off) / 7);
  const firstRow = Math.max(0, lastRow - MAX_WEEKS + 1);
  const shown = daily
    .map((d, i) => ({ d, row: Math.floor((i + off) / 7) - firstRow, col: (i + off) % 7 }))
    .filter((c) => c.row >= 0);
  const max = Math.max(0, ...shown.map((c) => c.d.work_seconds));
  const busiest = max > 0 ? shown.find((c) => c.d.work_seconds === max)!.d.day : null;
  return shown.map(({ d, row, col }) => {
    const h = towerHeight(d.work_seconds, max);
    return {
      day: d.day,
      col,
      row,
      x: col * P,
      y: row * P,
      kind: h > 0 ? "tower" : col >= 5 ? "park" : "lot",
      work: d.work_seconds,
      personal: d.personal_seconds,
      prompts: d.prompts,
      h,
      share: max > 0 ? d.work_seconds / max : 0,
      lit: windowCount(d.prompts, h),
      garden: gardenShare(d.work_seconds, d.personal_seconds),
      busiest: d.day === busiest,
      today: d.day === today,
      stat: d,
    };
  });
}

export function cityHeadline(plots: Plot[]): string {
  const towers = plots.filter((p) => p.kind === "tower");
  const top = plots.find((p) => p.busiest);
  if (!top) return "Nothing built yet.";
  const total = towers.reduce((s, p) => s + p.work, 0);
  const name = `${WEEKDAYS_LONG[dowMon(top.day)]} ${shortLabel(top.day)}`;
  return `${name} is your skyscraper: ${formatDuration(top.work)}. ${formatDuration(total)} across ${towers.length} tower${towers.length === 1 ? "" : "s"}.`;
}

export function plotTitle(p: Plot): string {
  if (p.kind === "park") return `${dayLabel(p.day)} · day off`;
  if (p.kind === "lot") return `${dayLabel(p.day)} · no work`;
  const personal = p.personal > 0 ? ` · ${formatDuration(p.personal)} personal` : "";
  return `${dayLabel(p.day)} · ${formatDuration(p.work)} work · ${p.prompts} prompts${personal}`;
}

export function cityAria(plots: Plot[]): string {
  const top = plots.find((p) => p.busiest);
  if (!top) return "City of workdays: nothing built yet.";
  const towers = plots.filter((p) => p.kind === "tower");
  const parks = plots.filter((p) => p.kind === "park").length;
  const prompts = towers.reduce((s, p) => s + p.prompts, 0);
  return `City of ${plots.length} days: ${towers.length} towers, ${parks} weekend parks. Tallest is ${dayLabel(top.day)} at ${formatDuration(top.work)}. ${prompts} prompts in total.`;
}

/** Window slot s (0-based, bottom-up, alternating faces) -> face and offset in face coordinates. */
export function windowSlot(s: number) {
  const face = s % 2 === 0 ? "left" : "right";
  const k = s >> 1;
  return { face, a: 0.15 + (k % 2) * 0.35, z: 0.35 + (k >> 1) * FLOOR_PITCH } as const;
}

const pt = (ps: [number, number][]) => ps.map(([a, b]) => `${a.toFixed(2)},${b.toFixed(2)}`).join(" ");
const dash = { strokeDasharray: "3 3" } as const;

function Window({ p, s }: { p: Plot; s: number }) {
  const { face, a, z } = windowSlot(s);
  const x0 = p.x + 0.3;
  const y0 = p.y + 0.3;
  const w = 0.25;
  const zb = PLINTH + z;
  const q = (u: number, zz: number) => (face === "left" ? iso(x0 + u, y0 + 0.9, zz, U) : iso(x0 + 0.9, y0 + u, zz, U));
  return <polygon points={pt([q(a, zb), q(a + w, zb), q(a + w, zb + 0.4), q(a, zb + 0.4)])} fill="var(--amber)" opacity={0.8} />;
}

function Tree({ x, y, r, ch }: { x: number; y: number; r: number; ch: number }) {
  const t = tones("var(--sage)");
  const tt = tones("var(--slate)");
  const trunk = isoBox(x - 0.04, y - 0.04, PLINTH, 0.08, 0.08, 0.3, U);
  const apex = iso(x, y, PLINTH + 0.3 + ch, U);
  const c = (dx: number, dy: number) => iso(x + dx, y + dy, PLINTH + 0.3, U);
  return (
    <g>
      <polygon points={trunk.left} fill={tt.left} />
      <polygon points={trunk.right} fill={tt.right} />
      <polygon points={pt([apex, c(-r, r), c(r, r)])} fill={t.left} />
      <polygon points={pt([apex, c(r, r), c(r, -r)])} fill={t.right} />
    </g>
  );
}

const TREES = [
  { x: 0.3, y: 0.3, r: 0.2, ch: 0.9 },
  { x: 0.8, y: 0.35, r: 0.16, ch: 0.7 },
  { x: 0.5, y: 0.85, r: 0.18, ch: 0.8 },
];

function Flag({ cx, cy, z, tip }: { cx: number; cy: number; z: number; tip: Tip }) {
  const a = iso(cx, cy, z + 1.6, U);
  const b = iso(cx, cy, z, U);
  return (
    <g className="art-fade sx-city-mark" style={{ animationDelay: "900ms" }} {...tipProps(tip)}>
      <line x1={b[0]} y1={b[1]} x2={a[0]} y2={a[1]} stroke="var(--fg-muted)" strokeWidth={1} />
      <polygon points={pt([a, iso(cx + 0.6, cy, z + 1.4, U), iso(cx, cy, z + 1.1, U)])} fill="var(--amber)" />
    </g>
  );
}

function Crane({ x, y, z, tip }: { x: number; y: number; z: number; tip: Tip }) {
  const m0 = iso(x, y, z, U);
  const m1 = iso(x, y, z + 2.2, U);
  const a0 = iso(x - 0.4, y, z + 2.2, U);
  const a1 = iso(x + 1.1, y, z + 2.2, U);
  const h1 = iso(x + 1.1, y, z + 1.5, U);
  const s = { stroke: "var(--fg-muted)", strokeWidth: 1 };
  return (
    <g className="art-fade sx-city-mark" style={{ animationDelay: "900ms" }} {...tipProps(tip)}>
      <rect x={Math.min(m0[0], a0[0]) - 3} y={m1[1] - 3} width={Math.abs(a1[0] - a0[0]) + 6} height={m0[1] - m1[1] + 6} fill="transparent" />
      <line x1={m0[0]} y1={m0[1]} x2={m1[0]} y2={m1[1]} style={s} />
      <line x1={a0[0]} y1={a0[1]} x2={a1[0]} y2={a1[1]} style={s} />
      <line x1={a1[0]} y1={a1[1]} x2={h1[0]} y2={h1[1]} style={{ ...s, strokeWidth: 0.75 }} />
      <circle cx={h1[0]} cy={h1[1]} r={1.5} fill="var(--terracotta)" />
    </g>
  );
}

function PlotView({ p, k, plots }: { p: Plot; k: number; plots: Plot[] }) {
  const base = isoBox(p.x + 0.15, p.y + 0.15, 0, 1.2, 1.2, PLINTH, U);
  const pl = tones("var(--bg-sunk)");
  const delay = `${Math.min(k * 14, 700)}ms`;
  const x0 = p.x + 0.3;
  const y0 = p.y + 0.3;
  const top = PLINTH + p.h;
  const lotStroke = p.kind === "lot" ? { ...dash, stroke: "var(--border-strong)", strokeWidth: 1 } : undefined;
  let body = null;
  if (p.kind === "tower") {
    const sage = `color-mix(in oklch, var(--sage) ${Math.round(100 - p.share * 60)}%, var(--sx-city-deep))`;
    const t = tones(sage);
    const b = isoBox(x0, y0, PLINTH, 0.9, 0.9, p.h, U);
    const g = Math.max(0.15, 0.9 * p.garden);
    const roof = isoBox(x0, y0 + 0.9 - g, top, 0.9, g, 0.12, U);
    const vt = tones("var(--violet)");
    body = (
      <>
        <polygon points={b.left} fill={t.left} />
        <polygon points={b.right} fill={t.right} />
        <polygon points={b.top} fill={t.top} />
        {Array.from({ length: p.lit }, (_, s) => (
          <Window key={s} p={p} s={s} />
        ))}
        {p.garden > 0 && (
          <>
            <polygon points={roof.left} fill={vt.left} />
            <polygon points={roof.right} fill={vt.right} />
            <polygon points={roof.top} fill={vt.top} />
          </>
        )}
        {p.busiest && <Flag cx={x0 + 0.45} cy={y0 + 0.4} z={top} tip={flagTip(p)} />}
      </>
    );
  } else if (p.kind === "park") {
    body = [...TREES].sort(byDepth).map((t, i) => <Tree key={i} x={x0 - 0.15 + t.x} y={y0 - 0.15 + t.y} r={t.r} ch={t.ch} />);
  }
  const tip = plotTip(p, plots);
  const inner = (
    <>
      <g className={p.kind === "tower" ? "sx-city-rise" : "art-fade"} style={{ animationDelay: delay }}>
        <polygon points={base.left} fill={pl.left} />
        <polygon points={base.right} fill={pl.right} />
        <polygon points={base.top} fill={pl.top} className="sx-city-hot" style={lotStroke} />
        {body}
      </g>
      {p.today && <Crane x={x0 + 0.15} y={y0 + 0.15} z={p.kind === "tower" ? top : PLINTH} tip={craneTip(p)} />}
    </>
  );
  if (p.kind === "tower") {
    return (
      <g className="sx-city-plot">
        <a href={`/${p.day}`} className="sx-city-link" tabIndex={0} aria-label={plotTitle(p)} {...tipProps(tip)}>
          {inner}
        </a>
      </g>
    );
  }
  // Parks with some activity are worth a tab stop; empty lots and quiet parks are not.
  const focusable = p.kind === "park" && p.stat.prompts + p.stat.tool_calls + p.stat.shell + p.stat.slack + p.stat.commits > 0;
  return (
    <g className="sx-city-plot" {...tipProps(tip)} {...(focusable ? { tabIndex: 0, role: "img", "aria-label": plotTitle(p) } : {})}>
      {inner}
    </g>
  );
}

const M = -0.6;
const X1 = 7 * P + 0.3;

/** Ground slab, week streets, weekday and month labels. */
function Ground({ rows, plots }: { rows: number; plots: Plot[] }) {
  const Y1 = rows * P + 0.3;
  const ground = isoBox(M, M, -0.3, X1 - M, Y1 - M, 0.3, U);
  const gc = "color-mix(in oklch, var(--bg-sunk), var(--border) 55%)";
  const gt = tones(gc);
  // Each week row is named by its first plot (a range can start mid-week).
  const firsts = new Map<number, Plot>();
  for (const p of plots) if (!firsts.has(p.row) || p.col < firsts.get(p.row)!.col) firsts.set(p.row, p);
  const labels = [...firsts.values()]
    .sort((a, b) => a.row - b.row)
    .map((p) => ({ row: p.row, month: MONTHS[parseDay(p.day).getUTCMonth()] }))
    .filter((l, i, a) => i === 0 || l.month !== a[i - 1].month);
  return (
    <>
      <polygon points={ground.left} fill={gt.left} />
      <polygon points={ground.right} fill={gt.right} />
      <polygon points={ground.top} fill={gc} />
      {Array.from({ length: rows - 1 }, (_, r) => {
        const a = iso(M + 0.2, (r + 1) * P, 0, U);
        const b = iso(X1 - 0.2, (r + 1) * P, 0, U);
        return <line key={r} x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} stroke="var(--border-strong)" strokeWidth={1} style={dash} />;
      })}
      {WEEKDAYS.map((w, c) => {
        const [lx, ly] = iso(c * P + P / 2 + 0.1, Y1 + 0.55, 0, U);
        return (
          <text key={w} x={lx} y={ly} textAnchor="middle" className="art-label" style={{ fill: c >= 5 ? "var(--fg-subtle)" : "var(--fg-muted)" }}>
            {w}
          </text>
        );
      })}
      {labels.map((l) => {
        const [lx, ly] = iso(M - 0.3, l.row * P + P / 2, 0, U);
        return (
          <text key={l.row} x={lx} y={ly + 3} textAnchor="end" className="art-label" style={{ fill: "var(--fg)" }}>
            {l.month}
          </text>
        );
      })}
    </>
  );
}

export function WorkdayCity({ daily, today }: { daily: DailyStat[]; today: string }) {
  const titleId = useId();
  const plots = layoutCity(daily, today);
  const rows = plots.length ? Math.max(...plots.map((p) => p.row)) + 1 : 1;
  const Y1 = rows * P + 0.3;
  const x0 = iso(M, Y1, 0, U)[0] - 30;
  const x1 = iso(X1, M, 0, U)[0] + 6;
  const y0 = iso(M, M, MAX_H + PLINTH + 2.6, U)[1] - 4;
  const y1 = iso(X1, Y1, -0.3, U)[1] + 20;
  const W = x1 - x0;
  const empty = plots.every((p) => p.kind !== "tower");
  const sorted = [...plots].sort((a, b) => byDepth({ x: a.col, y: a.row }, { x: b.col, y: b.row }));
  return (
    <div className="sx-city-card">
      <p className="sx-city-head" id={titleId}>{cityHeadline(plots)}</p>
      <svg
        className="sx-city-svg"
        viewBox={`${x0.toFixed(1)} ${y0.toFixed(1)} ${W.toFixed(1)} ${(y1 - y0).toFixed(1)}`}
        style={{ maxWidth: Math.round(W * 1.1) }}
        role="group"
        aria-label={cityAria(plots)}
      >
        <Ground rows={rows} plots={plots} />
        {sorted.map((p, k) => (
          <PlotView key={p.day} p={p} k={k} plots={plots} />
        ))}
      </svg>
      {empty && <p className="sx-city-empty">nothing built yet</p>}
      <p className="sx-city-legend">height = hours · lit windows = prompts · trees = days off</p>
    </div>
  );
}
