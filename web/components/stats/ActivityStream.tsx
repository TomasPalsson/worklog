import "@/app/stats/charts-time.css";
import type { DailyStat } from "@/lib/stats_contract";
import { dayLabel, shortLabel } from "./time-utils";
import { tipProps, type Tip } from "./tip";
import { nf, ordinal, pct, times } from "./chart-tip-utils";

const W = 1000;
const H = 200;
const PAD = 6;

export const SERIES: { key: keyof DailyStat; label: string; color: string }[] = [
  { key: "prompts", label: "prompts", color: "var(--sage)" },
  { key: "tool_calls", label: "tool calls", color: "var(--amber)" },
  { key: "shell", label: "shell", color: "var(--violet)" },
  { key: "slack", label: "slack", color: "var(--slate)" },
  { key: "browser_minutes", label: "browser min", color: "var(--terracotta)" },
  { key: "commits", label: "commits", color: "oklch(var(--project-l) var(--project-c) 250)" },
];

type Pt = [number, number];

/** Fritsch-Carlson monotone tangents for x-increasing points. */
function tangents(p: Pt[]): number[] {
  const n = p.length;
  const d = p.slice(1).map((q, i) => (q[1] - p[i][1]) / (q[0] - p[i][0] || 1));
  const m = p.map((_, i) => (i === 0 ? d[0] : i === n - 1 ? d[n - 2] : d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2));
  for (let i = 0; i < n - 1; i++) {
    if (d[i] === 0) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / d[i];
    const b = m[i + 1] / d[i];
    const s = a * a + b * b;
    if (s > 9) {
      const t = 3 / Math.sqrt(s);
      m[i] = t * a * d[i];
      m[i + 1] = t * b * d[i];
    }
  }
  return m;
}

const f = (v: number) => v.toFixed(2);

/** Closed band between two x-aligned monotone-cubic curves (upper left-to-right, lower back). */
export function bandPath(upper: Pt[], lower: Pt[]): string {
  const mu = tangents(upper);
  const ml = tangents(lower);
  let d = `M${f(upper[0][0])} ${f(upper[0][1])}`;
  for (let i = 0; i < upper.length - 1; i++) {
    const [x0, y0] = upper[i];
    const [x1, y1] = upper[i + 1];
    const k = (x1 - x0) / 3;
    d += `C${f(x0 + k)} ${f(y0 + k * mu[i])} ${f(x1 - k)} ${f(y1 - k * mu[i + 1])} ${f(x1)} ${f(y1)}`;
  }
  d += `L${f(lower[lower.length - 1][0])} ${f(lower[lower.length - 1][1])}`;
  for (let i = lower.length - 1; i > 0; i--) {
    const [x0, y0] = lower[i];
    const [x1, y1] = lower[i - 1];
    const k = (x0 - x1) / 3;
    d += `C${f(x0 - k)} ${f(y0 - k * ml[i])} ${f(x1 + k)} ${f(y1 + k * ml[i - 1])} ${f(x1)} ${f(y1)}`;
  }
  return d + "Z";
}

export interface Layer {
  label: string;
  color: string;
  total: number;
  peak: number;
  peakDay: string;
  /** Days with a non-zero value. */
  active: number;
  path: string;
}

/** Each series normalised to its own max, stacked around a centred baseline. */
export function buildLayers(daily: DailyStat[]): Layer[] {
  const rows = daily.length === 1 ? [daily[0], daily[0]] : daily;
  const n = rows.length;
  const vals = SERIES.map((s) => rows.map((d) => Math.max(0, Number(d[s.key]) || 0)));
  const maxes = vals.map((v) => Math.max(...v));
  const norm = vals.map((v, si) => v.map((x) => (maxes[si] > 0 ? x / maxes[si] : 0)));
  const tot = rows.map((_, i) => norm.reduce((a, v) => a + v[i], 0));
  const peak = Math.max(...tot);
  const unit = peak > 0 ? (H / 2 - PAD) / (peak / 2) : 0;
  const xs = rows.map((_, i) => PAD + (i / (n - 1)) * (W - 2 * PAD));
  const acc = Array<number>(n).fill(0);
  return SERIES.flatMap((s, si) => {
    const total = daily.reduce((a, d) => a + (Number(d[s.key]) || 0), 0);
    if (total <= 0) return [];
    const lower: Pt[] = xs.map((x, i) => [x, H / 2 + (acc[i] - tot[i] / 2) * unit]);
    norm[si].forEach((v, i) => (acc[i] += v));
    const upper: Pt[] = xs.map((x, i) => [x, H / 2 + (acc[i] - tot[i] / 2) * unit]);
    const pi = vals[si].indexOf(maxes[si]);
    return [{ label: s.label, color: s.color, total, peak: maxes[si], peakDay: rows[pi].day, active: daily.filter((d) => Number(d[s.key]) > 0).length, path: bandPath(upper, lower) }];
  });
}

/** Hover card for one series layer; rank = position by total among layers. */
export function layerTip(l: Layer, layers: Layer[], days: number): Tip {
  const grand = layers.reduce((a, x) => a + x.total, 0);
  const avg = l.total / Math.max(1, l.active);
  const rank = layers.filter((x) => x.total > l.total).length + 1;
  return {
    title: l.label,
    sub: `${ordinal(rank)} biggest of ${layers.length} ${layers.length === 1 ? "stream" : "streams"}`,
    rows: [
      ["Total", nf.format(l.total)],
      ["Peak day", `${nf.format(l.peak)} on ${shortLabel(l.peakDay)}`],
      ["Active days", `${l.active} of ${days}`],
      ["Avg per active day", nf.format(Math.round(avg))],
      ["Share of all counts", pct(l.total / grand)],
    ],
    bar: { value: l.total, max: grand, label: `${pct(l.total / grand)} of everything counted here` },
    note: l.peak > avg * 1.05 ? `Peak day was ${times(l.peak / avg)} its average active day.` : "Steady: no day stood far above the rest.",
    accent: l.color,
  };
}

/** Hover card for one day's column: every series' raw value. */
export function columnTip(d: DailyStat, daily: DailyStat[]): Tip {
  const val = (x: DailyStat, k: keyof DailyStat) => Math.max(0, Number(x[k]) || 0);
  const peaked = SERIES.filter((s) => {
    const all = daily.map((x) => val(x, s.key));
    return val(d, s.key) > 0 && val(d, s.key) === Math.max(...all) && Math.min(...all) !== Math.max(...all);
  }).map((s) => s.label);
  return {
    title: dayLabel(d.day),
    sub: "every stream, raw values",
    rows: SERIES.map((s) => [s.label, nf.format(val(d, s.key))]),
    note: peaked.length ? `Peak day for ${peaked.join(" and ")}.` : "No stream peaked this day.",
  };
}

/** Invisible full-height hover column per day, centred on the day's x. */
function Columns({ daily }: { daily: DailyStat[] }) {
  const n = daily.length;
  const step = n > 1 ? (W - 2 * PAD) / (n - 1) : W;
  return (
    <>
      {daily.map((d, i) => {
        const cx = n > 1 ? PAD + i * step : W / 2;
        const x0 = Math.max(0, cx - step / 2);
        const x1 = Math.min(W, cx + step / 2);
        return <rect key={d.day} className="st-col" x={x0} y={0} width={x1 - x0} height={H} {...tipProps(columnTip(d, daily))} />;
      })}
    </>
  );
}

export function ActivityStream({ daily }: { daily: DailyStat[] }) {
  const layers = buildLayers(daily);
  if (layers.length === 0) {
    return (
      <div className="st-chart">
        <div className="st-empty">No data yet. The stream starts flowing with your first prompts and commands.</div>
      </div>
    );
  }
  const nf = new Intl.NumberFormat("en-US");
  const aria = `Activity stream over ${daily.length} days, each series scaled to its own peak. ${layers.map((l) => `${l.label} ${nf.format(l.total)}`).join(", ")}.`;
  return (
    <div className="st-chart">
      <div className="st-scroll">
        <svg className="st-svg" viewBox={`0 0 ${W} ${H}`} style={{ width: "100%", minWidth: 600, maxHeight: 260 }} role="img" aria-label={aria}>
          {layers.map((l, i) => (
            <path
              key={l.label}
              className="st-layer art-fade"
              d={l.path}
              fill={l.color}
              style={{ fillOpacity: 0.85, animationDelay: `${i * 120}ms` }}
              tabIndex={0}
              aria-label={`${l.label}: ${nf.format(l.total)} total`}
              {...tipProps(layerTip(l, layers, daily.length))}
            />
          ))}
          <Columns daily={daily} />
          <text x={PAD} y={H - 4} className="art-label">{shortLabel(daily[0].day)}</text>
          <text x={W - PAD} y={H - 4} textAnchor="end" className="art-label">{shortLabel(daily[daily.length - 1].day)}</text>
        </svg>
      </div>
      <ul className="st-legend">
        {layers.map((l) => (
          <li key={l.label} className="st-legtip" {...tipProps(layerTip(l, layers, daily.length))}>
            <span className="st-swatch" style={{ background: l.color }} />
            {l.label} <span className="st-num">{nf.format(l.total)}</span>
          </li>
        ))}
      </ul>
      <p className="st-caption">Shape, not size: every stream is scaled to its own busiest day, so you can compare rhythms rather than volume.</p>
    </div>
  );
}
