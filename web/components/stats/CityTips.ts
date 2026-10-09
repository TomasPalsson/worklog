import type { DailyStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { dayLabel, hmToMin } from "./time-utils";
import type { Tip } from "./tip";
import type { Plot } from "./WorkdayCity";

const nf = new Intl.NumberFormat("en-US");
const plural = (n: number, one: string, many = `${one}s`) => `${nf.format(n)} ${n === 1 ? one : many}`;
const ordinal = (n: number) => `#${n}`;

/** Non-zero activity rows shared by every plot kind. */
function activityRows(d: DailyStat): [string, string][] {
  const rows: [string, string][] = [];
  if (d.personal_seconds > 0) rows.push(["Personal", formatDuration(d.personal_seconds)]);
  if (d.ignored_seconds > 0) rows.push(["Ignored", formatDuration(d.ignored_seconds)]);
  if (d.prompts > 0) rows.push(["Claude prompts", nf.format(d.prompts)]);
  if (d.tool_calls > 0) rows.push(["Tool calls", nf.format(d.tool_calls)]);
  if (d.shell > 0) rows.push(["Shell commands", nf.format(d.shell)]);
  if (d.slack > 0) rows.push(["Slack messages", nf.format(d.slack)]);
  if (d.commits > 0) rows.push(["Commits", nf.format(d.commits)]);
  if (d.meeting_seconds > 0) rows.push(["Meetings", formatDuration(d.meeting_seconds)]);
  return rows;
}

const spanRows = (d: DailyStat): [string, string][] => {
  const rows: [string, string][] = [];
  if (d.first_at && d.last_at) rows.push(["Started → finished", `${d.first_at} – ${d.last_at}`]);
  if (d.folders > 0) rows.push(["Projects", nf.format(d.folders)]);
  if (d.tickets > 0) rows.push(["Tickets", nf.format(d.tickets)]);
  return rows;
};

const CLICK = "Click to open the day.";

/** The most striking true fact about a tower, then the click hint. */
function towerNote(p: Plot, avg: number): string {
  const d = p.stat;
  const ratio = avg > 0 ? p.work / avg : 0;
  const first = hmToMin(d.first_at);
  let fact: string;
  if (p.today) fact = "Under construction: today so far.";
  else if (ratio >= 1.5) fact = `${ratio.toFixed(1)}× your average worked day.`;
  else if (d.prompts >= 100) fact = `Lights on: ${nf.format(d.prompts)} prompts.`;
  else if (first !== null && first < 7 * 60) fact = `Built before 07:00 (first activity ${d.first_at}).`;
  else if (ratio > 0 && ratio <= 0.5) fact = `A quick one: ${ratio.toFixed(1)}× your average worked day.`;
  else fact = `${ratio.toFixed(1)}× your average worked day.`;
  return `${fact} ${CLICK}`;
}

/** Rich tip for a plot (tower, park or empty lot) given every plot in the city. */
export function plotTip(p: Plot, plots: Plot[]): Tip {
  const d = p.stat;
  const title = dayLabel(p.day);
  if (p.kind === "tower") {
    const towers = plots.filter((q) => q.kind === "tower");
    const max = Math.max(...towers.map((q) => q.work));
    // Today's unfinished tower would drag the average down; leave it out when others exist.
    const done = towers.filter((q) => !q.today);
    const pool = done.length ? done : towers;
    const avg = pool.reduce((s, q) => s + q.work, 0) / pool.length;
    const rank = 1 + towers.filter((q) => q.work > p.work).length;
    return {
      title,
      sub: `${p.busiest ? "busiest day · " : ""}${ordinal(rank)} of ${plural(towers.length, "worked day")}`,
      rows: [["Work", formatDuration(p.work)], ...activityRows(d), ...spanRows(d)],
      bar: { value: p.work, max, label: `${Math.round((p.work / max) * 100)}% of the busiest day` },
      note: towerNote(p, avg),
      accent: "var(--sage)",
    };
  }
  const rows = activityRows(d);
  const bits = [
    d.prompts > 0 && plural(d.prompts, "prompt"),
    d.shell > 0 && plural(d.shell, "shell command"),
    d.commits > 0 && plural(d.commits, "commit"),
    d.tool_calls > 0 && plural(d.tool_calls, "tool call"),
    d.slack > 0 && plural(d.slack, "Slack message"),
    d.personal_seconds > 0 && `${formatDuration(d.personal_seconds)} personal`,
    d.ignored_seconds > 0 && `${formatDuration(d.ignored_seconds)} ignored`,
    d.meeting_seconds > 0 && `${formatDuration(d.meeting_seconds)} of meetings`,
  ].filter(Boolean);
  if (p.kind === "park") {
    return {
      title,
      sub: "day off · a park",
      rows: rows.length ? rows : [["Activity", "none"]],
      note: bits.length ? `Even on a day off: ${bits.join(", ")}.` : "Trees only: a proper day off.",
      accent: "var(--sage)",
    };
  }
  return {
    title,
    sub: "weekday · empty lot",
    rows: rows.length ? rows : [["Activity", "none"]],
    note: bits.length ? `No work tracked, but ${bits.join(", ")}.` : "Nothing built, nothing tracked.",
    accent: "var(--border-strong)",
  };
}

/** The flag on the busiest tower. */
export function flagTip(p: Plot): Tip {
  return {
    title: "Busiest day",
    sub: dayLabel(p.day),
    rows: [["Work", formatDuration(p.work)], ["Claude prompts", nf.format(p.prompts)]],
    note: "The tallest tower in the city carries the flag.",
    accent: "var(--amber)",
  };
}

/** The crane building today. */
export function craneTip(p: Plot): Tip {
  return {
    title: `Today · ${dayLabel(p.day)}`,
    sub: "under construction",
    rows: [["Work so far", formatDuration(p.work)], ["Claude prompts", nf.format(p.prompts)]],
    note: p.kind === "tower" ? `Under construction: today so far. ${CLICK}` : "Under construction: today so far.",
    accent: "var(--terracotta)",
  };
}

