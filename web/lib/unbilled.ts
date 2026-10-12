// Turns the daemon's per-block loose ends into what the Owner acts on: one
// entry per day, each with its two steps — sent to Tempo (per ticket) and
// marked billed (one click per day on the Billing page).

import { shiftDay } from "./format";
import type { LooseEnds } from "./time_leaks_contract";

/** The daily auto-send to Tempo runs at 17:00 Owner-local (auto_send.rs). */
export const TEMPO_SEND_HOUR = 17;

export type StepState = "done" | "needs" | "waiting";
export interface TicketHours {
  key: string;
  seconds: number;
}
export interface DayWork {
  day: string;
  seconds: number;
  tempo: { state: StepState; tickets: TicketHours[] };
  bill: { state: StepState; seconds: number };
}
/** A strip cell: `tempo` wins over `bill` because Tempo comes first. */
export type CycleState = "tempo" | "bill" | "waiting" | "clear" | "future";
export interface CycleDay {
  day: string;
  state: CycleState;
}
export interface Unbilled {
  days: DayWork[];
  cycle: CycleDay[];
  tempo: { seconds: number; days: number };
  bill: { seconds: number; days: number };
}

export function summarise(data: LooseEnds, today: string, hourNow: number): Unbilled {
  const byDay = new Map<string, DayWork>();
  const tickets = new Map<string, Map<string, number>>();
  for (const it of data.items) {
    const d = byDay.get(it.day) ?? {
      day: it.day,
      seconds: 0,
      tempo: { state: "done" as StepState, tickets: [] },
      bill: { state: "done" as StepState, seconds: 0 },
    };
    byDay.set(it.day, d);
    d.seconds += it.seconds;
    if (it.missing.includes("tempo")) {
      const t = tickets.get(it.day) ?? new Map<string, number>();
      tickets.set(it.day, t);
      t.set(it.jira_issue, (t.get(it.jira_issue) ?? 0) + it.seconds);
    }
    if (it.missing.includes("export")) {
      d.bill = { state: "needs", seconds: d.bill.seconds + it.seconds };
    }
  }
  const days = [...byDay.values()].sort((a, b) => a.day.localeCompare(b.day));
  for (const d of days) {
    const t = tickets.get(d.day);
    if (!t) continue;
    d.tempo.tickets = [...t].map(([key, seconds]) => ({ key, seconds })).sort((a, b) => b.seconds - a.seconds);
    d.tempo.state = d.day === today && hourNow < TEMPO_SEND_HOUR ? "waiting" : "needs";
  }
  const sum = (pick: (d: DayWork) => number) => {
    const open = days.filter((d) => pick(d) > 0);
    return { seconds: open.reduce((a, d) => a + pick(d), 0), days: open.length };
  };
  return {
    days,
    cycle: cycleDays(data.cycle_start, data.cycle_end, today, byDay),
    tempo: sum((d) => d.tempo.tickets.reduce((a, t) => a + t.seconds, 0)),
    bill: sum((d) => d.bill.seconds),
  };
}

function cycleDays(start: string, end: string, today: string, byDay: Map<string, DayWork>): CycleDay[] {
  const out: CycleDay[] = [];
  for (let day = start; day <= end; day = shiftDay(day, 1)) {
    const d = byDay.get(day);
    let state: CycleState = day > today ? "future" : "clear";
    if (d?.tempo.state === "needs") state = "tempo";
    else if (d?.tempo.state === "waiting") state = "waiting";
    else if (d?.bill.state === "needs") state = "bill";
    out.push({ day, state });
  }
  return out;
}
