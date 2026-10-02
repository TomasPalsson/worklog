import { mondayOf } from "./format";
import type { RawBlock, TicketBlocks } from "./types";

export type Chip = "Sent" | "Changed since sent" | "Not sent";

const synced = (b: RawBlock) => !!b.tempo_worklog_id;

/** A day's Tempo state: any synced block edited since is "Changed since sent"; all synced is "Sent". */
export function chipOf(blocks: RawBlock[]): Chip {
  if (blocks.some((b) => synced(b) && b.dirty)) return "Changed since sent";
  return blocks.every(synced) ? "Sent" : "Not sent";
}

export interface TicketHours {
  total: number;
  week: number;
  month: number;
  days: number;
  first: string | null;
  last: string | null;
  /** Seconds on days not (fully) in Tempo. */
  unsent: number;
  /** Seconds on days with any block not yet exported to the invoice. */
  uninvoiced: number;
}

/** Billed (half-hour-rounded) line hours across every day the ticket was worked, split by week, month and state. */
export function ticketHours(data: TicketBlocks, today: string): TicketHours {
  const monday = mondayOf(today);
  const month = today.slice(0, 7);
  const out: TicketHours = { total: 0, week: 0, month: 0, days: data.days.length, first: null, last: null, unsent: 0, uninvoiced: 0 };
  for (const d of data.days) {
    const s = d.line_seconds;
    out.total += s;
    if (d.day >= monday && d.day <= today) out.week += s;
    if (d.day.slice(0, 7) === month) out.month += s;
    if (chipOf(d.blocks) !== "Sent") out.unsent += s;
    if (d.blocks.some((b) => !b.exported_at)) out.uninvoiced += s;
    if (out.first === null || d.day < out.first) out.first = d.day;
    if (out.last === null || d.day > out.last) out.last = d.day;
  }
  return out;
}

const utc = (ymd: string) => Date.UTC(Number(ymd.slice(0, 4)), Number(ymd.slice(5, 7)) - 1, Number(ymd.slice(8, 10)));

/** `today` / `yesterday` / `N days ago`. */
export function relativeDay(day: string, today: string): string {
  const n = Math.round((utc(today) - utc(day)) / 86_400_000);
  return n <= 0 ? "today" : n === 1 ? "yesterday" : `${n} days ago`;
}
