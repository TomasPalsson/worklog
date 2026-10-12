// Turns the daemon's per-day answer into what the Owner acts on: per day, the
// ticket lines Tempo is missing, the work without a ticket, and the hours not
// yet exported to the invoice form.

import { shiftDay } from "./format";
import type { TempoGapReason, UnbilledDay, UnbilledDays } from "./time_leaks_contract";

/** The daily auto-send to Tempo runs at 17:00 Owner-local (auto_send.rs). */
export const TEMPO_SEND_HOUR = 17;

export type StepState = "done" | "needs" | "waiting";
export interface TicketGap {
  key: string;
  /** Hours Tempo is missing: the whole line, or what a short line lacks. */
  missing: number;
  inTempo: number;
  reason: TempoGapReason;
}
export interface DayWork {
  day: string;
  dismissed: string | null;
  tempo: { state: StepState; tickets: TicketGap[]; seconds: number };
  ticketless: number;
  bill: number;
}
/** A strip cell: tempo > ticket > bill > waiting, the order the Owner fixes them. */
export type CycleState = "tempo" | "ticket" | "bill" | "waiting" | "clear" | "future";
export interface CycleDay {
  day: string;
  state: CycleState;
}
type Total = { seconds: number; days: number };
export interface Unbilled {
  days: DayWork[];
  cycle: CycleDay[];
  tempo: Total;
  ticket: Total;
  bill: Total;
}

function dayWork(d: UnbilledDay, today: string, hourNow: number): DayWork {
  const tickets = d.tempo
    .map((g) => ({
      key: g.jira_issue,
      missing: g.reason === "short" ? g.seconds - g.in_tempo_seconds : g.seconds,
      inTempo: g.in_tempo_seconds,
      reason: g.reason,
    }))
    .sort((a, b) => b.missing - a.missing);
  let state: StepState = "done";
  if (tickets.length > 0) state = d.day === today && hourNow < TEMPO_SEND_HOUR ? "waiting" : "needs";
  return {
    day: d.day,
    dismissed: d.dismissed,
    tempo: { state, tickets, seconds: tickets.reduce((a, t) => a + t.missing, 0) },
    ticketless: d.ticketless_seconds,
    bill: d.export?.seconds ?? 0,
  };
}

export function summarise(data: UnbilledDays, today: string, hourNow: number): Unbilled {
  const days = data.days.map((d) => dayWork(d, today, hourNow)).sort((a, b) => a.day.localeCompare(b.day));
  const sum = (pick: (d: DayWork) => number): Total => {
    const open = days.filter((d) => pick(d) > 0);
    return { seconds: open.reduce((a, d) => a + pick(d), 0), days: open.length };
  };
  return {
    days,
    cycle: cycleDays(data.cycle_start, data.cycle_end, today, new Map(days.map((d) => [d.day, d]))),
    tempo: sum((d) => d.tempo.seconds),
    ticket: sum((d) => d.ticketless),
    bill: sum((d) => d.bill),
  };
}

function cellState(d: DayWork | undefined): CycleState | null {
  if (d?.tempo.state === "needs") return "tempo";
  if (d && d.ticketless > 0) return "ticket";
  if (d && d.bill > 0) return "bill";
  if (d?.tempo.state === "waiting") return "waiting";
  return null;
}

function cycleDays(start: string, end: string, today: string, byDay: Map<string, DayWork>): CycleDay[] {
  const out: CycleDay[] = [];
  for (let day = start; day <= end; day = shiftDay(day, 1)) {
    out.push({ day, state: cellState(byDay.get(day)) ?? (day > today ? "future" : "clear") });
  }
  return out;
}
