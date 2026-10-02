// Pure helpers for the My Tasks board.

import type { StatusCategory, TaskRow, Transition } from "./types";

export type Column = "new" | "indeterminate" | "done";

export const COLUMNS: { id: Column; title: string }[] = [
  { id: "new", title: "To Do" },
  { id: "indeterminate", title: "In Progress" },
  { id: "done", title: "Done" },
];

export const columnTitle = (c: Column) => COLUMNS.find((x) => x.id === c)?.title ?? c;

export const columnOf = (category: StatusCategory | null): Column => category ?? "new";

/** Transitions that land in `column`; a null `to_category` never matches. */
export const movesInto = (transitions: Transition[], column: Column): Transition[] =>
  transitions.filter((t) => t.to_category === column);

/** `{name} → {to_status}`, or just the name when both say the same thing. */
export const transitionLabel = (t: Transition): string =>
  t.name.toLowerCase() === t.to_status.toLowerCase() ? t.name : `${t.name} → ${t.to_status}`;

export const ticketCount = (n: number): string => `${n} ${n === 1 ? "ticket" : "tickets"}`;

export const weekTotal = (tasks: TaskRow[]): number =>
  tasks.reduce((sum, t) => sum + t.week_seconds, 0);

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number) => String(n).padStart(2, "0");

/** `d MMM, HH:MM` in local time. Jira sends `+0000` offsets; normalise to `+00:00`. */
export function formatStamp(iso: string): string {
  const d = new Date(iso.replace(/([+-]\d\d)(\d\d)$/, "$1:$2"));
  if (Number.isNaN(d.getTime())) return iso;
  return `${d.getDate()} ${MONTHS[d.getMonth()]}, ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const dayNum = (ymd: string): number => {
  const [y, m, d] = ymd.split("-").map(Number);
  return Date.UTC(y, m - 1, d) / 86_400_000;
};

/** Where `due` sits relative to `today` (both YYYY-MM-DD); `days` is due minus today. */
export function dueState(due: string, today: string): { state: "overdue" | "today" | "soon" | "later"; days: number } {
  const days = dayNum(due) - dayNum(today);
  return { state: days < 0 ? "overdue" : days === 0 ? "today" : days <= 2 ? "soon" : "later", days };
}

/** The local calendar date as YYYY-MM-DD, for callers the board didn't hand a `today`. */
export const localToday = (): string => {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

/** `d MMM` for a YYYY-MM-DD date. */
export const shortDate = (ymd: string): string => `${Number(ymd.slice(8, 10))} ${MONTHS[Number(ymd.slice(5, 7)) - 1]}`;

/** `just now` / `5m ago` / `3h ago` / `2d ago`; empty when `iso` doesn't parse. */
export function relativeAge(iso: string, now: Date): string {
  const t = new Date(iso.replace(/([+-]\d\d)(\d\d)$/, "$1:$2")).getTime();
  if (Number.isNaN(t)) return "";
  const min = Math.floor((now.getTime() - t) / 60_000);
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  if (min < 1440) return `${Math.floor(min / 60)}h ago`;
  return `${Math.floor(min / 1440)}d ago`;
}

/** Bar heights in px: scaled to `maxSeconds`, at least 2px, 2px stub for an idle day. */
export const sparkHeights = (daySeconds: number[], maxSeconds: number, maxPx = 16): number[] =>
  daySeconds.map((s) => (s > 0 ? Math.max(2, Math.round((s / Math.max(maxSeconds, 1)) * maxPx)) : 2));

/** Largest single-day seconds on the board; never below 1 so it is always a safe divisor. */
export const weekMax = (tasks: TaskRow[]): number => Math.max(1, ...tasks.flatMap((t) => t.day_seconds));
