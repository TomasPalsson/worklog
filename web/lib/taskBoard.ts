// Pure helpers for the My Tasks board.

import type { StatusCategory, TaskRow, Transition } from "./types";

export type Column = "new" | "indeterminate" | "done";

export const COLUMNS: { id: Column; title: string }[] = [
  { id: "new", title: "To do" },
  { id: "indeterminate", title: "In progress" },
  { id: "done", title: "Done" },
];

export const columnTitle = (c: Column) => COLUMNS.find((x) => x.id === c)?.title ?? c;

export const columnOf = (category: StatusCategory | null): Column => category ?? "new";

/** Transitions that land in `column`; a null `to_category` never matches. */
export const movesInto = (transitions: Transition[], column: Column): Transition[] =>
  transitions.filter((t) => t.to_category === column);

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
