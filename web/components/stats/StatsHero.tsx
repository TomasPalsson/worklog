import { formatDuration, shortMonthDay, shortWeekday } from "@/lib/format";
import type { StatsReport } from "@/lib/stats_contract";
import { tipProps, type Tip } from "./tip";

const nf = new Intl.NumberFormat("en-US");

const cnt = (noun: string) => (n: number) => {
  const num = n >= 10 || Number.isInteger(n) ? nf.format(Math.round(n)) : n.toFixed(1);
  return `${num} ${num === "1" ? noun.replace(/s$/, "") : noun}`; // "1 prompt", not "1 prompts"
};
const dur = (sec: number) => formatDuration(sec);
const dayName = (d: string) => `${shortWeekday(d)} ${shortMonthDay(d)}`;

/** Sum a series into at most `n` buckets so a year of days stays a readable line. */
export function bucket(values: number[], n: number): number[] {
  if (values.length <= n) return values;
  const out = new Array<number>(n).fill(0);
  values.forEach((v, i) => {
    out[Math.min(n - 1, Math.floor((i * n) / values.length))] += v;
  });
  return out;
}

const W = 100;
const H = 26;

/** Polyline points for a series in a 100x26 box; flat (never NaN) when empty or all zero. */
export function sparkPoints(values: number[]): string {
  const v = bucket(values, 60);
  if (v.length === 0) return "";
  const max = Math.max(...v);
  const x = (i: number) => (v.length === 1 ? W / 2 : (i * W) / (v.length - 1));
  const y = (val: number) => (max > 0 ? H - 2 - (val / max) * (H - 4) : H - 2);
  return v.map((val, i) => `${x(i).toFixed(1)},${y(val).toFixed(1)}`).join(" ");
}

/** Indices of the days behind each drawn point (one each, unless bucketed). */
export function groupsOf(len: number, n = 60): number[][] {
  if (len <= n) return Array.from({ length: len }, (_, i) => [i]);
  const out: number[][] = Array.from({ length: n }, () => []);
  for (let i = 0; i < len; i++) out[Math.min(n - 1, Math.floor((i * n) / len))].push(i);
  return out;
}

const sum = (v: number[]) => v.reduce((a, x) => a + x, 0);

function vsAverage(perDay: number, avg: number, peak: boolean): string {
  if (perDay <= 0) return "A quiet one.";
  if (peak) return "Your peak of the range.";
  if (avg <= 0) return "";
  const x = perDay / avg;
  if (x >= 1.05) return `${x.toFixed(1)}\u00d7 your average active day.`;
  if (x <= 0.95) return `${Math.round(x * 100)}% of your average active day.`;
  return "Right on your average.";
}

/** One tip per drawn point: the day (or span of days), its value, and how it compares to the average. */
export function sparkTips(series: number[], days: string[], fmt: (n: number) => string, noun: string): Tip[] {
  const active = series.filter((v) => v > 0);
  const avg = active.length ? sum(active) / active.length : 0;
  const peak = Math.max(0, ...series);
  return groupsOf(series.length).map((g) => {
    const total = sum(g.map((i) => series[i]));
    const per = total / g.length;
    const first = days[g[0]];
    const last = days[g[g.length - 1]];
    const title = g.length === 1 ? dayName(first) : `${dayName(first)} \u2013 ${dayName(last)}`;
    const rows: [string, string][] = [[g.length === 1 ? noun : `${noun} (${g.length} days)`, fmt(total)]];
    if (g.length > 1) rows.push(["Per day", fmt(per)]);
    rows.push(["Average active day", fmt(avg)], ["Peak day", fmt(peak)]);
    return {
      title,
      sub: g.length === 1 ? undefined : `${g.length} days, summed to keep the line readable`,
      rows,
      bar: { value: per, max: peak, label: peak > 0 ? `${Math.round((per / peak) * 100)}% of the peak day` : undefined },
      note: vsAverage(per, avg, g.length === 1 && per === peak),
    };
  });
}

/** The tip on a tile's big number. */
export function tileTip(t: Tile, days: string[], daysWorked: number): Tip {
  const rows: [string, string][] = [["Total", `${t.value}${t.unit ?? ""}`]];
  const s = t.series ?? [];
  const per = daysWorked > 0 ? sum(s) / daysWorked : 0;
  if (t.series) rows.push(["Per worked day", t.fmt(per)]);
  const peak = Math.max(0, ...s);
  const worked = s.map((v, i) => [v, i] as const).filter(([v]) => v > 0);
  if (peak > 0) rows.push(["Best day", `${dayName(days[s.indexOf(peak)])} \u00b7 ${t.fmt(peak)}`]);
  if (worked.length > 1) {
    const low = worked.reduce((a, b) => (b[0] < a[0] ? b : a));
    rows.push(["Quietest active day", `${dayName(days[low[1]])} \u00b7 ${t.fmt(low[0])}`]);
  }
  rows.push(["Days worked", nf.format(daysWorked)]);
  const ok = peak > 0 && per > 0;
  return {
    title: t.label,
    sub: t.caption,
    rows,
    bar: ok ? { value: per, max: peak, label: "Average worked day vs your best day" } : undefined,
    note: ok ? `Your best day was ${(peak / per).toFixed(1)}\u00d7 the average.` : undefined,
    accent: `var(${t.tone})`,
  };
}

function Sparkline({ values, tips }: { values: number[]; tips: Tip[] }) {
  const pts = sparkPoints(values);
  const flat = values.every((v) => v === 0);
  return (
    <svg className="stats-spark" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
      {values.length === 1 ? (
        <circle cx={W / 2} cy={flat ? H - 2 : 2} r="2" style={{ fill: "currentColor" }} />
      ) : (
        // art-fade, not art-draw: with non-scaling-stroke Chrome measures the
        // pathLength=1 dash in screen px, so the draw-in stopped part-way.
        <polyline
          className="art-fade"
          points={pts}
          style={{
            fill: "none",
            stroke: "currentColor",
            strokeWidth: 1.6,
            strokeLinecap: "round",
            strokeLinejoin: "round",
            vectorEffect: "non-scaling-stroke",
            opacity: flat ? 0.35 : 1,
          }}
        />
      )}
      {tips.map((tip, i) => {
        const w = W / tips.length;
        return (
          <rect key={i} className="stats-spark-hit" x={i * w} y={0} width={w} height={H} style={{ fill: "transparent" }} {...tipProps(tip)} />
        );
      })}
    </svg>
  );
}

type Tile = {
  key: string; label: string; value: string; unit?: string; caption: string; series?: number[]; tone: string;
  /** Formats one series value (or an average of them) with its unit. */
  fmt: (n: number) => string;
  /** Row label for the series, e.g. "Prompts". */
  noun: string;
};

function perDay(total: number, days: number): number {
  return days > 0 ? total / days : 0;
}

export function heroTiles(r: StatsReport): Tile[] {
  return [...claudeTiles(r), ...outsideTiles(r)];
}

function claudeTiles(r: StatsReport): Tile[] {
  const t = r.totals;
  const d = r.daily;
  const days = t.days_worked;
  const hours = t.work_seconds / 3600;
  const avg = (n: number, digits = 0) => perDay(n, days).toFixed(digits);
  return [
    {
      key: "hours",
      label: "hours worked",
      value: hours.toFixed(1),
      unit: "h",
      caption: days > 0 ? `About ${avg(hours, 1)}h on a day you show up.` : "No hours logged yet.",
      series: d.map((x) => x.work_seconds),
      tone: "--sage",
      fmt: dur,
      noun: "Worked",
    },
    {
      key: "prompts",
      label: "Claude prompts",
      value: nf.format(t.prompts),
      caption: t.prompts > 0 ? `You talk to Claude ${avg(t.prompts)} times a day.` : "Claude has not heard from you.",
      series: d.map((x) => x.prompts),
      tone: "--violet",
      fmt: cnt("prompts"),
      noun: "Prompts",
    },
    {
      key: "tools",
      label: "tool calls",
      value: nf.format(t.tool_calls),
      caption: t.tool_calls > 0 ? `Claude did ${avg(t.tool_calls)} things a day on your behalf.` : "No tools touched.",
      series: d.map((x) => x.tool_calls),
      tone: "--slate",
      fmt: cnt("tool calls"),
      noun: "Tool calls",
    },
    {
      key: "helpers",
      label: "subagents spawned",
      value: nf.format(t.helpers),
      caption: t.helpers > 0 ? "Small helpers, sent off to fetch." : "Nobody delegated yet.",
      tone: "--amber",
      fmt: cnt("subagents"),
      noun: "Subagents",
    },
  ];
}

function outsideTiles(r: StatsReport): Tile[] {
  const t = r.totals;
  const d = r.daily;
  const avg = (n: number) => perDay(n, t.days_worked).toFixed(0);
  return [
    {
      key: "shell",
      label: "shell commands",
      value: nf.format(t.shell_commands),
      caption: t.shell_commands > 0 ? `${avg(t.shell_commands)} a day typed at a prompt.` : "The terminal is quiet.",
      series: d.map((x) => x.shell),
      tone: "--terracotta",
      fmt: cnt("commands"),
      noun: "Commands",
    },
    {
      key: "shipped",
      label: "commits + PRs",
      value: nf.format(t.commits + t.prs),
      caption: t.commits + t.prs > 0 ? `${nf.format(t.commits)} commits, ${nf.format(t.prs)} pull requests.` : "Nothing shipped yet.",
      series: d.map((x) => x.commits),
      tone: "--sage",
      fmt: cnt("commits"),
      noun: "Commits",
    },
    {
      key: "slack",
      label: "Slack messages",
      value: nf.format(t.slack_messages),
      caption: t.slack_messages > 0 ? "Words sent to colleagues." : "Radio silence on Slack.",
      series: d.map((x) => x.slack),
      tone: "--amber",
      fmt: cnt("messages"),
      noun: "Messages",
    },
    {
      key: "browser",
      label: "hours in the browser",
      value: (t.browser_minutes / 60).toFixed(1),
      unit: "h",
      caption: t.browser_minutes > 0 ? "Tabs, mostly." : "No browsing recorded.",
      series: d.map((x) => x.browser_minutes),
      tone: "--slate",
      fmt: (n) => dur(n * 60),
      noun: "In the browser",
    },
    {
      key: "meetings",
      label: "meetings",
      value: nf.format(t.meetings),
      caption:
        t.meetings > 0 ? `${(t.meeting_seconds / 3600).toFixed(1)}h of other people's calendars.` : "A meeting-free stretch.",
      series: d.map((x) => x.meeting_seconds),
      tone: "--terracotta",
      fmt: dur,
      noun: "In meetings",
    },
  ];
}

export function StatsHero({ report }: { report: StatsReport }) {
  const days = report.daily.map((x) => x.day);
  return (
    <ul className="stats-hero" aria-label="Totals">
      {heroTiles(report).map((t, i) => (
        <li
          key={t.key}
          className="stats-tile art-fade"
          style={{ animationDelay: `${i * 60}ms`, color: `var(${t.tone}-ink, var(${t.tone}))` }}
        >
          <span
            className="stats-tile-num"
            tabIndex={0}
            aria-label={`${t.label}: ${t.value}${t.unit ?? ""}`}
            {...tipProps(tileTip(t, days, report.totals.days_worked))}
          >
            {t.value}
            {t.unit && <small>{t.unit}</small>}
          </span>
          <span className="stats-tile-label">{t.label}</span>
          {t.series && <Sparkline values={t.series} tips={sparkTips(t.series, days, t.fmt, t.noun)} />}
          <span className="stats-tile-cap">{t.caption}</span>
        </li>
      ))}
    </ul>
  );
}
