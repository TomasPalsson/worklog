import type { StatsReport } from "@/lib/stats_contract";

const nf = new Intl.NumberFormat("en-US");

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

function Sparkline({ values }: { values: number[] }) {
  const pts = sparkPoints(values);
  const flat = values.every((v) => v === 0);
  return (
    <svg className="stats-spark" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
      {values.length === 1 ? (
        <circle cx={W / 2} cy={flat ? H - 2 : 2} r="2" style={{ fill: "currentColor" }} />
      ) : (
        <polyline
          className="art-draw"
          pathLength={1}
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
    </svg>
  );
}

type Tile = { key: string; label: string; value: string; unit?: string; caption: string; series?: number[]; tone: string };

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
    },
    {
      key: "prompts",
      label: "Claude prompts",
      value: nf.format(t.prompts),
      caption: t.prompts > 0 ? `You talk to Claude ${avg(t.prompts)} times a day.` : "Claude has not heard from you.",
      series: d.map((x) => x.prompts),
      tone: "--violet",
    },
    {
      key: "tools",
      label: "tool calls",
      value: nf.format(t.tool_calls),
      caption: t.tool_calls > 0 ? `Claude did ${avg(t.tool_calls)} things a day on your behalf.` : "No tools touched.",
      series: d.map((x) => x.tool_calls),
      tone: "--slate",
    },
    {
      key: "helpers",
      label: "subagents spawned",
      value: nf.format(t.helpers),
      caption: t.helpers > 0 ? "Small helpers, sent off to fetch." : "Nobody delegated yet.",
      tone: "--amber",
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
    },
    {
      key: "shipped",
      label: "commits + PRs",
      value: nf.format(t.commits + t.prs),
      caption: t.commits + t.prs > 0 ? `${nf.format(t.commits)} commits, ${nf.format(t.prs)} pull requests.` : "Nothing shipped yet.",
      series: d.map((x) => x.commits),
      tone: "--sage",
    },
    {
      key: "slack",
      label: "Slack messages",
      value: nf.format(t.slack_messages),
      caption: t.slack_messages > 0 ? "Words sent to colleagues." : "Radio silence on Slack.",
      series: d.map((x) => x.slack),
      tone: "--amber",
    },
    {
      key: "browser",
      label: "hours in the browser",
      value: (t.browser_minutes / 60).toFixed(1),
      unit: "h",
      caption: t.browser_minutes > 0 ? "Tabs, mostly." : "No browsing recorded.",
      series: d.map((x) => x.browser_minutes),
      tone: "--slate",
    },
    {
      key: "meetings",
      label: "meetings",
      value: nf.format(t.meetings),
      caption:
        t.meetings > 0 ? `${(t.meeting_seconds / 3600).toFixed(1)}h of other people's calendars.` : "A meeting-free stretch.",
      series: d.map((x) => x.meeting_seconds),
      tone: "--terracotta",
    },
  ];
}

export function StatsHero({ report }: { report: StatsReport }) {
  return (
    <ul className="stats-hero" aria-label="Totals">
      {heroTiles(report).map((t, i) => (
        <li
          key={t.key}
          className="stats-tile art-fade"
          style={{ animationDelay: `${i * 60}ms`, color: `var(${t.tone}-ink, var(${t.tone}))` }}
        >
          <span className="stats-tile-num">
            {t.value}
            {t.unit && <small>{t.unit}</small>}
          </span>
          <span className="stats-tile-label">{t.label}</span>
          {t.series && <Sparkline values={t.series} />}
          <span className="stats-tile-cap">{t.caption}</span>
        </li>
      ))}
    </ul>
  );
}
