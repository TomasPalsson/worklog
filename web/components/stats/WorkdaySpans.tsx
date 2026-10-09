import "@/app/stats/charts-time.css";
import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { dayLabel, dowMon, hmToMin, minToHm, shortLabel } from "./time-utils";

const LEFT = 34;
const RIGHT = 46;
const TOP = 14;
const PLOT = 240;
const BOT = 18;
const H = TOP + PLOT + BOT;
const DAY_START = 360; // 06:00
const DAY_END = 1440; // 24:00

const y = (min: number) => TOP + ((Math.min(DAY_END, Math.max(DAY_START, min)) - DAY_START) / (DAY_END - DAY_START)) * PLOT;

export function median(xs: number[]): number | null {
  if (xs.length === 0) return null;
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

export interface Span {
  i: number;
  day: string;
  from: number;
  to: number;
  work: number;
}

export function spans(daily: DailyStat[]): Span[] {
  const out: Span[] = [];
  daily.forEach((d, i) => {
    const from = hmToMin(d.first_at);
    const to = hmToMin(d.last_at);
    if (from !== null && to !== null) out.push({ i, day: d.day, from, to: Math.max(from, to), work: d.work_seconds });
  });
  return out;
}

function Candles({ list, slot }: { list: Span[]; slot: number }) {
  return (
    <>
      {list.map((s, k) => {
        const top = y(s.from);
        const h = Math.max(3, y(s.to) - top);
        const pct = 25 + 75 * Math.min(1, s.work / (8 * 3600));
        const weekend = dowMon(s.day) >= 5;
        return (
          <rect
            key={s.day}
            className="art-grow-y st-cell"
            x={LEFT + s.i * slot + slot * 0.15}
            y={top}
            width={slot * 0.7}
            height={h}
            rx={Math.min(3, slot * 0.35)}
            fill={`color-mix(in oklch, var(--sage) ${pct.toFixed(0)}%, var(--bg-sunk))`}
            style={{ opacity: weekend ? 0.45 : 1, animationDelay: `${Math.min(k * 12, 600)}ms` }}
          >
            <title>{`${dayLabel(s.day)} · ${minToHm(s.from)} to ${minToHm(s.to)} · ${formatDuration(s.work)} of work${weekend ? " (weekend)" : ""}`}</title>
          </rect>
        );
      })}
    </>
  );
}

function Medians({ ms, me, w }: { ms: number; me: number; w: number }) {
  const dash = { strokeDasharray: "4 3", strokeWidth: 1.2 };
  return (
    <>
      {[
        [ms, "var(--amber)"],
        [me, "var(--terracotta)"],
      ].map(([m, c]) => (
        <g key={c as string} className="art-fade">
          <line x1={LEFT} x2={LEFT + w} y1={y(m as number)} y2={y(m as number)} stroke={c as string} style={dash} />
          <text x={LEFT + w + 4} y={y(m as number) + 3} className="art-label" style={{ fill: "var(--fg)" }}>
            {minToHm(m as number)}
          </text>
        </g>
      ))}
    </>
  );
}

export function WorkdaySpans({ daily }: { daily: DailyStat[] }) {
  const list = spans(daily);
  if (list.length === 0) {
    return (
      <div className="st-chart">
        <div className="st-empty">No data yet. Your workday shape shows up after the first tracked day.</div>
      </div>
    );
  }
  const n = daily.length;
  const slot = Math.min(60, Math.max(4, 1000 / n)); // ~1000 wide: text stays ~12px on desktop
  const W = LEFT + RIGHT + n * slot;
  const ms = median(list.map((s) => s.from))!;
  const me = median(list.map((s) => s.to))!;
  const aria = `Workday spans over ${list.length} days. Median start ${minToHm(ms)}, median finish ${minToHm(me)}.`;
  return (
    <div className="st-chart">
      <div className="st-big">
        {minToHm(ms)} to {minToHm(me)}
      </div>
      <div className="st-scroll" style={{ marginTop: 12 }}>
        <svg
          className="st-svg"
          viewBox={`0 0 ${W} ${H}`}
          style={{ width: "100%", minWidth: Math.round(W * 0.75) }}
          role="img"
          aria-label={aria}
        >
          {[6, 9, 12, 15, 18, 21, 24].map((h) => (
            <g key={h}>
              <line x1={LEFT} x2={LEFT + n * slot} y1={y(h * 60)} y2={y(h * 60)} className="st-grid" />
              <text x={LEFT - 6} y={y(h * 60) + 3} textAnchor="end" className="art-label">
                {String(h).padStart(2, "0")}
              </text>
            </g>
          ))}
          <Candles list={list} slot={slot} />
          <Medians ms={ms} me={me} w={n * slot} />
          <text x={LEFT} y={H - 3} className="art-label">
            {shortLabel(daily[0].day)}
          </text>
          <text x={LEFT + n * slot} y={H - 3} textAnchor="end" className="art-label">
            {shortLabel(daily[n - 1].day)}
          </text>
        </svg>
      </div>
      <p className="st-caption">
        You usually start at <b>{minToHm(ms)}</b> and stop at <b>{minToHm(me)}</b>. Dashed lines are the medians; darker candles are longer work days, faded ones are weekends.
      </p>
    </div>
  );
}
