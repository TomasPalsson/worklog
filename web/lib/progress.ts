import type { PersonHours, TicketProgress } from "@/lib/types";

const MAX_NAMED = 4;
const CHART_DAYS = 14;

export type Tone = "ok" | "low" | "over";

export interface BarSegment {
  account_id: string;
  name: string;
  seconds: number;
  is_you: boolean;
}

export interface BarModel {
  estimate: number;
  segments: BarSegment[];
  pending: number;
  used: number;
  left: number;
  over: number;
  tone: Tone;
}

export interface ChartModel {
  estimate: number;
  days: string[];
  series: Array<{ name: string; is_you: boolean; values: number[] }>;
}

export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0].toUpperCase())
    .join("");
}

export function pendingSeconds(b: { tempo_worklog_id: string | null; duration_seconds: number }): number {
  return b.tempo_worklog_id ? 0 : b.duration_seconds;
}

export function toneFor(used: number, estimate: number): Tone {
  if (used > estimate) return "over";
  return used * 10 >= estimate * 8 ? "low" : "ok";
}

/** You first, then hours desc; everyone past the 4th named person becomes "Others". */
function ordered(people: PersonHours[]): PersonHours[] {
  const sorted = [...people].sort((a, b) => Number(b.is_you) - Number(a.is_you) || b.seconds - a.seconds);
  if (sorted.length <= MAX_NAMED) return sorted;
  const rest = sorted.slice(MAX_NAMED);
  const others: PersonHours = {
    account_id: "others",
    name: "Others",
    is_you: false,
    seconds: rest.reduce((a, p) => a + p.seconds, 0),
    by_day: rest.flatMap((p) => p.by_day),
  };
  return [...sorted.slice(0, MAX_NAMED), others];
}

export function barModel(t: TicketProgress, pending: number): BarModel | null {
  const estimate = t.estimate_seconds;
  if (!estimate) return null;
  const used = t.logged_seconds + pending;
  return {
    estimate,
    segments: ordered(t.people).map(({ account_id, name, seconds, is_you }) => ({ account_id, name, seconds, is_you })),
    pending,
    used,
    left: Math.max(0, estimate - used),
    over: Math.max(0, used - estimate),
    tone: toneFor(used, estimate),
  };
}

export function chartModel(t: TicketProgress): ChartModel | null {
  const estimate = t.estimate_seconds;
  const people = ordered(t.people);
  const all = [...new Set(people.flatMap((p) => p.by_day.map(([d]) => d)))].sort();
  if (!estimate || all.length === 0) return null;
  const days = all.slice(-CHART_DAYS);
  const first = days[0];
  return {
    estimate,
    days,
    series: people.map((p) => {
      const per = new Map<string, number>();
      for (const [d, s] of p.by_day) per.set(d, (per.get(d) ?? 0) + s);
      let total = 0;
      for (const [d, s] of per) if (d < first) total += s;
      return {
        name: p.name,
        is_you: p.is_you,
        values: days.map((d) => (total += per.get(d) ?? 0)),
      };
    }),
  };
}
