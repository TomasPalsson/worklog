// Pure helpers for TicketTimeline (server-safe, unit-tested).
import type { TicketStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import type { Tip } from "./tip";
import { dayLabel, dowMon, MONTHS, parseDay } from "./time-utils";

const DAY_MS = 86400000;
/** Hard guard so a bogus range can't build a million columns. */
const MAX_COLS = 800;

export type SortMode = "hours" | "start";
export const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

/** Days in the inclusive range; 0 when unparsable. */
export function rangeDays(from: string, to: string): number {
  const n = Math.round((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / DAY_MS) + 1;
  return Number.isFinite(n) && n > 0 ? n : 0;
}

/** Hours with one decimal, trailing ".0" dropped: 2.5, 1, 0.3; 10h+ whole: 11. */
export function fmtHours(seconds: number): string {
  // 10h+ drops the decimal so "10.5" doesn't spill out of a 20px square
  if (seconds >= 36000) return String(Math.round(seconds / 3600));
  const s = (seconds / 3600).toFixed(1);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/** 0 = none, 1..4 = quartiles of the max single ticket-day. */
export function shadeBucket(seconds: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (!(seconds > 0) || !(max > 0)) return 0;
  const r = seconds / max;
  return r <= 0.25 ? 1 : r <= 0.5 ? 2 : r <= 0.75 ? 3 : 4;
}

/** Legend text for the 4 shades, e.g. "0–0.6h". */
export function shadeRanges(max: number): string[] {
  // Under an hour, tenths of an hour collapse ("0–0h"): show minutes instead.
  const mins = max < 3600;
  const u = mins ? "m" : "h";
  const q = [0.25, 0.5, 0.75, 1].map((f) => (mins ? String(Math.round((max * f) / 60)) : fmtHours(max * f)));
  return [`0–${q[0]}${u}`, `${q[0]}–${q[1]}${u}`, `${q[1]}–${q[2]}${u}`, `${q[2]}–${q[3]}${u}`];
}

export interface Col {
  day: string;
  dom: number;
  /** One-letter weekday initial. */
  initial: string;
  weekend: boolean;
  monday: boolean;
  today: boolean;
  /** "Oct" on the first column and whenever the month changes. */
  month: string | null;
}

export function buildCols(from: string, to: string, today: string): Col[] {
  const n = Math.min(rangeDays(from, to), MAX_COLS);
  const start = parseDay(from).getTime();
  const cols: Col[] = [];
  let prev = -1;
  for (let i = 0; i < n; i++) {
    const d = new Date(start + i * DAY_MS);
    const day = d.toISOString().slice(0, 10);
    const dow = dowMon(day);
    const m = d.getUTCMonth();
    cols.push({
      day,
      dom: d.getUTCDate(),
      initial: "MTWTFSS"[dow],
      weekend: dow >= 5,
      monday: dow === 0,
      today: day === today,
      month: m !== prev ? MONTHS[m] : null,
    });
    prev = m;
  }
  return cols;
}

/** Worked days with seconds each. Falls back to [first,last] and an even split for old daemons. */
export function ticketDays(t: TicketStat): { day: string; seconds: number }[] {
  const days = t.days?.length ? t.days : [...new Set([t.first_day, t.last_day])];
  const ds = t.day_seconds;
  const ok = Array.isArray(ds) && ds.length === days.length;
  return days.map((day, i) => ({ day, seconds: ok ? ds[i] : t.seconds / days.length }));
}

export function sortTickets(tickets: TicketStat[], mode: SortMode): TicketStat[] {
  const out = [...tickets];
  out.sort((a, b) =>
    mode === "start"
      ? a.first_day.localeCompare(b.first_day) || b.seconds - a.seconds
      : b.seconds - a.seconds || a.first_day.localeCompare(b.first_day),
  );
  return out;
}

/** Biggest single ticket-day across all tickets (shade scale). */
export function maxTicketDay(tickets: TicketStat[]): number {
  let m = 0;
  for (const t of tickets) for (const d of ticketDays(t)) m = Math.max(m, d.seconds);
  return m;
}

/** Per-column totals over the given tickets. */
export function dayTotals(tickets: TicketStat[], cols: Col[]): number[] {
  const idx = new Map(cols.map((c, i) => [c.day, i]));
  const out = cols.map(() => 0);
  for (const t of tickets)
    for (const d of ticketDays(t)) {
      const i = idx.get(d.day);
      if (i !== undefined) out[i] += d.seconds;
    }
  return out;
}

const spanDays = (a: string, b: string) =>
  Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / DAY_MS) + 1;
const pct = (n: number, total: number) => (total > 0 ? `${Math.round((n / total) * 100)}%` : "–");

export function ticketStatsLine(t: TicketStat, totalWork: number): string {
  const n = ticketDays(t).length;
  return [
    formatDuration(t.seconds),
    plural(t.blocks, "block"),
    plural(n, "day"),
    `${formatDuration(t.seconds / Math.max(1, n))}/day`,
    `${pct(t.seconds, totalWork)} of work`,
  ].join(" · ");
}

export function headline(tickets: TicketStat[]): string {
  if (tickets.length === 0) return "";
  const total = tickets.reduce((s, t) => s + t.seconds, 0);
  const top = sortTickets(tickets, "hours")[0];
  const n = ticketDays(top).length;
  return `${plural(tickets.length, "ticket")}, ${formatDuration(total)} — ${top.key} took the most: ${formatDuration(top.seconds)} over ${plural(n, "day")}`;
}

export function rowTip(t: TicketStat, totalWork: number): Tip {
  const days = ticketDays(t);
  const best = days.reduce((a, b) => (b.seconds > a.seconds ? b : a), days[0]);
  const span = spanDays(t.first_day, t.last_day);
  const rows: [string, string][] = [
    ["Status", t.status ?? "unknown"],
    ["Hours", formatDuration(t.seconds)],
    ["Blocks", String(t.blocks)],
    ["Days active", String(days.length)],
    ["First → last", `${dayLabel(t.first_day)} → ${dayLabel(t.last_day)}`],
    ["Span", plural(span, "day")],
    ["Per active day", formatDuration(t.seconds / Math.max(1, days.length))],
    ["Share of all work", pct(t.seconds, totalWork)],
  ];
  if (best) rows.push(["Busiest day", `${dayLabel(best.day)} · ${formatDuration(best.seconds)}`]);
  return { title: t.key, sub: t.summary ?? undefined, rows, accent: "var(--sage)" };
}

export function cellTip(t: TicketStat, day: string, seconds: number): Tip {
  const days = ticketDays(t);
  const k = days.findIndex((d) => d.day === day);
  const prev = k > 0 ? spanDays(days[k - 1].day, day) - 1 : null;
  const rows: [string, string][] = [
    ["Hours", formatDuration(seconds)],
    ["Share of ticket", pct(seconds, t.seconds)],
    ["Day", `${k + 1} of ${days.length} on this ticket`],
    ["Since previous", prev === null ? "first day on it" : prev === 1 ? "the day before" : `${prev} days earlier`],
  ];
  return { title: `${t.key} · ${dayLabel(day)}`, sub: t.summary ?? undefined, rows, accent: "var(--sage)" };
}
