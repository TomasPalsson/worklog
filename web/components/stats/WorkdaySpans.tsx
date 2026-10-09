import "@/app/stats/charts-time.css";
import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { dayLabel, dowMon, hmToMin, minToHm, shortLabel } from "./time-utils";
import { tipProps, type Tip } from "./tip";

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

const nf = new Intl.NumberFormat("en-US");
const mins = (m: number) => formatDuration(Math.round(m) * 60);
export const ordinal = (n: number) => {
  const r = n % 100;
  const suffix = r >= 11 && r <= 13 ? "th" : (["th", "st", "nd", "rd"][n % 10] ?? "th");
  return `${n}${suffix}`;
};

/** Rich tip for one day's candle. `list` is every span, `prompts` that day's prompt count. */
export function spanTip(s: Span, list: Span[], prompts: number): Tip {
  const len = s.to - s.from;
  const longest = Math.max(...list.map((x) => x.to - x.from));
  const startRank = list.filter((x) => x.from < s.from).length + 1;
  const lenRank = list.filter((x) => x.to - x.from > len).length + 1;
  const ms = median(list.map((x) => x.from))!;
  const diff = Math.round(s.from - ms);
  const note =
    list.length < 2
      ? "Only one tracked day so far: no usual start to compare with."
      : Math.abs(diff) < 5
        ? `Right on your usual start (median ${minToHm(ms)}).`
        : `Started ${mins(Math.abs(diff))} ${diff < 0 ? "earlier" : "later"} than usual (median ${minToHm(ms)}).`;
  return {
    title: dayLabel(s.day),
    sub: `${ordinal(lenRank)} longest of ${list.length} spans${dowMon(s.day) >= 5 ? " · weekend" : ""}`,
    rows: [
      ["Started", minToHm(s.from)],
      ["Finished", minToHm(s.to)],
      ["Span length", mins(len)],
      ["Work inside it", formatDuration(s.work)],
      ["Prompts", nf.format(prompts)],
      ["Start rank", `${ordinal(startRank)} earliest of ${list.length}`],
    ],
    bar: { value: len, max: longest, label: `${mins(len)} of the longest span, ${mins(longest)}` },
    note,
    accent: "var(--sage)",
  };
}

/** Tip for a median line: kind picks start (from) or finish (to). */
export function medianTip(kind: "start" | "finish", list: Span[]): Tip {
  const xs = list.map((x) => (kind === "start" ? x.from : x.to));
  const m = median(xs)!;
  const lo = Math.min(...xs);
  const hi = Math.max(...xs);
  const atOrBefore = xs.filter((x) => x <= m).length;
  return {
    title: `Median ${kind}`,
    sub: `Across ${list.length} ${list.length === 1 ? "day" : "days"}`,
    rows: [
      ["Median", minToHm(m)],
      [kind === "start" ? "Earliest start" : "Earliest finish", minToHm(lo)],
      [kind === "start" ? "Latest start" : "Latest finish", minToHm(hi)],
      ["Spread", mins(hi - lo)],
      [`Days at or before it`, `${atOrBefore} of ${list.length}`],
    ],
    note: hi === lo ? `Every ${kind} landed on the same minute.` : `Your ${kind} wanders across a ${mins(hi - lo)} window.`,
    accent: kind === "start" ? "var(--amber)" : "var(--terracotta)",
  };
}

function Candles({ list, slot, daily }: { list: Span[]; slot: number; daily: DailyStat[] }) {
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
            className="art-grow-y st-cell st-candle"
            {...tipProps(spanTip(s, list, daily[s.i].prompts))}
            tabIndex={0}
            role="img"
            aria-label={`${dayLabel(s.day)}: ${minToHm(s.from)} to ${minToHm(s.to)}, ${formatDuration(s.work)} of work`}
            x={LEFT + s.i * slot + slot * 0.15}
            y={top}
            width={slot * 0.7}
            height={h}
            rx={Math.min(3, slot * 0.35)}
            fill={`color-mix(in oklch, var(--sage) ${pct.toFixed(0)}%, var(--bg-sunk))`}
            style={{ opacity: weekend ? 0.45 : 1, animationDelay: `${Math.min(k * 12, 600)}ms` }}
          />
        );
      })}
    </>
  );
}

function Medians({ ms, me, w, list }: { ms: number; me: number; w: number; list: Span[] }) {
  const dash = { strokeDasharray: "4 3", strokeWidth: 1.2 };
  return (
    <>
      {[
        [ms, "var(--amber)", "start"],
        [me, "var(--terracotta)", "finish"],
      ].map(([m, c, kind]) => (
        <g
          key={c as string}
          className="art-fade st-median"
          {...tipProps(medianTip(kind as "start" | "finish", list))}
          tabIndex={0}
          role="img"
          aria-label={`Median ${kind}: ${minToHm(m as number)}`}
        >
          <rect x={LEFT + w} y={y(m as number) - 8} width={RIGHT} height={16} style={{ fill: "transparent" }} />
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
          <Candles list={list} slot={slot} daily={daily} />
          <Medians ms={ms} me={me} w={n * slot} list={list} />
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
