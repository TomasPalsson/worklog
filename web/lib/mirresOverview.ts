// Pure aggregation behind the /mirres page: Mirres facts grouped per
// project account, and per-day match / billable summaries.

import { billablePercent } from "./tempo_line_contract";
import { statusKind } from "./mirresStatus";
import type { StatusKind } from "./mirresStatus";
import type { BillingClass, MirresDay, MirresDetails } from "./tempo_line_contract";

export interface ProjectRow {
  account_key: string;
  project: string | null;
  project_type: string | null;
  class: BillingClass;
  warning: string | null;
  customer: string | null;
  details: MirresDetails | null;
  /** Unique, sorted. */
  tickets: string[];
  /** Unique, newest first. */
  days: string[];
  seconds: number;
}

/** Groups every line with Mirres data by account; the newest day's facts win. */
export function projectsFrom(days: MirresDay[]): ProjectRow[] {
  const newestFirst = [...days].sort((a, b) => b.day.localeCompare(a.day));
  const rows = new Map<string, ProjectRow>();
  for (const d of newestFirst) {
    for (const l of d.lines) {
      const b = l.billing;
      if (!b) continue;
      let row = rows.get(b.account_key);
      if (!row) {
        row = {
          account_key: b.account_key,
          project: b.project,
          project_type: b.project_type,
          class: b.class,
          warning: b.warning,
          customer: b.customer,
          details: b.details,
          tickets: [],
          days: [],
          seconds: 0,
        };
        rows.set(b.account_key, row);
      }
      if (!row.tickets.includes(l.jira_issue)) row.tickets.push(l.jira_issue);
      if (!row.days.includes(d.day)) row.days.push(d.day);
      row.seconds += l.effective_seconds;
    }
  }
  const out = [...rows.values()];
  for (const r of out) r.tickets.sort();
  return out.sort(
    (a, b) =>
      Number(Boolean(b.warning)) - Number(Boolean(a.warning)) ||
      b.seconds - a.seconds ||
      (a.project ?? a.account_key).localeCompare(b.project ?? b.account_key),
  );
}

export function daySummary(day: MirresDay): {
  matched: number;
  total: number;
  billablePercent: number | null;
} {
  return {
    matched: day.lines.filter((l) => l.billing).length,
    total: day.lines.length,
    billablePercent: billablePercent(day.lines),
  };
}

export function totals(days: MirresDay[]) {
  const projects = projectsFrom(days);
  return {
    projects: projects.length,
    tickets: new Set(projects.flatMap((p) => p.tickets)).size,
    days: days.length,
    attention: projects.filter((p) => p.warning).length,
    seconds: days.reduce((a, d) => a + d.lines.reduce((b, l) => b + l.effective_seconds, 0), 0),
    billablePercent: billablePercent(days.flatMap((d) => d.lines)),
    billedSeconds: days.reduce(
      (a, d) =>
        a + d.lines.reduce((b, l) => (l.billing && l.billing.class !== "not_billable" ? b + l.effective_seconds : b), 0),
      0,
    ),
  };
}

/** Compact hours, the app's style: "3.5h". */
export const hrs = (seconds: number) => `${(seconds / 3600).toFixed(1)}h`;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const dayParts = (d: string) => ({ y: Number(d.slice(0, 4)), m: Number(d.slice(5, 7)) - 1, d: Number(d.slice(8, 10)) });

/** "Tue 6 Oct" from YYYY-MM-DD. */
export function shortDay(day: string): string {
  const { y, m, d } = dayParts(day);
  return `${WEEKDAYS[new Date(Date.UTC(y, m, d)).getUTCDay()]} ${d} ${MONTHS[m]}`;
}

/** "Tue 6 Oct", "5–6 Oct" or "30 Sep – 2 Oct" from YYYY-MM-DD days; "" when none. */
export function dateRangeLabel(dayList: string[]): string {
  if (dayList.length === 0) return "";
  const sorted = [...dayList].sort();
  const a = dayParts(sorted[0]);
  const b = dayParts(sorted[sorted.length - 1]);
  if (a.m === b.m && a.d === b.d) return shortDay(sorted[0]);
  return a.m === b.m ? `${a.d}–${b.d} ${MONTHS[b.m]}` : `${a.d} ${MONTHS[a.m]} – ${b.d} ${MONTHS[b.m]}`;
}

/** Seconds of extra billable time needed to reach `goal`% of logged; never negative. */
export function goalGap(billedSeconds: number, totalSeconds: number, goal: number): number {
  return Math.max(0, (goal / 100) * totalSeconds - billedSeconds);
}

/** Project text without its "<customer> · " prefix (the customer has its own column). */
export function stripCustomer(project: string | null, customer: string | null): string | null {
  const prefix = customer ? `${customer} · ` : null;
  return prefix && project?.startsWith(prefix) ? project.slice(prefix.length) : project;
}

/** Short muted reason a line is not billed; null when nothing needs saying. */
export function statusReason(
  cls: BillingClass,
  projectType: string | null,
  _warning: string | null,
): string | null {
  if (cls === "included") return "covered by contract";
  if (cls !== "not_billable" || !projectType) return null;
  if (projectType.includes("Fast verð")) return "fixed price";
  if (projectType.includes("Innifalið")) return "subscription";
  if (projectType.includes("(Apró)")) return "internal";
  return null;
}

/** One plain-English line under an Icelandic Mirres warning; null if unknown. */
/** One instruction for a list of warnings when they are all the same kind; else null. */
export function sharedHelp(warnings: (string | null)[], count: number): string | null {
  if (warnings.length === 0 || new Set(warnings).size !== 1) return null;
  if (warnings[0] === "Samning vantar í Mirres")
    return `${count === 1 ? "This project has" : `These ${count} projects have`} no contract in Mirres, so their hours don't count as billable. Ask the owner to add one, then fetch again.`;
  return warningHelp(warnings[0]);
}

export function warningHelp(warning: string | null): string | null {
  if (!warning) return null;
  if (warning === "Samning vantar í Mirres")
    return "No contract in Mirres, so these hours can't count as billable. Ask the owner to add it, then fetch again.";
  if (warning.startsWith("Tímafjölda vantar")) return "The contract has no hours set in Mirres.";
  if (warning === "Innifaldir tímar uppurnir") return "Included hours are used up for this period.";
  if (warning.startsWith("Innifaldir tímar að klárast")) return "Included hours are almost used up.";
  if (warning === "Ekki virkt Mirres-verkefni")
    return "This Tempo account is not an active Mirres project — the ticket may be on the wrong account.";
  return null;
}

/** "fetched 17:13" for today, else "fetched 5 Oct 17:13": 24h local time. */
export function formatFetchedAt(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  const time = `${p(d.getHours())}:${p(d.getMinutes())}`;
  const today = d.toDateString() === now.toDateString();
  return `fetched ${today ? "" : `${d.getDate()} ${MONTHS[d.getMonth()]} `}${time}`;
}

export interface CustomerGroup {
  /** Display name; "Unknown customer" when Mirres named none. */
  name: string;
  known: boolean;
  seconds: number;
  /** Status of the project with the most hours. */
  kind: StatusKind;
  /** Hours desc. */
  projects: ProjectRow[];
}

export const projectKind = (p: ProjectRow): StatusKind => statusKind(p);

/** Projects grouped by customer, most hours first; the unknown customer last. */
export function customersFrom(days: MirresDay[]): CustomerGroup[] {
  const groups = new Map<string, CustomerGroup>();
  for (const p of projectsFrom(days)) {
    const key = p.customer ?? "";
    let g = groups.get(key);
    if (!g) {
      g = { name: p.customer ?? "Unknown customer", known: p.customer !== null, seconds: 0, kind: "not_billable", projects: [] };
      groups.set(key, g);
    }
    g.projects.push(p);
    g.seconds += p.seconds;
  }
  const out = [...groups.values()];
  for (const g of out) {
    g.projects.sort((a, b) => b.seconds - a.seconds);
    g.kind = projectKind(g.projects[0]);
  }
  return out.sort((a, b) => Number(b.known) - Number(a.known) || b.seconds - a.seconds || a.name.localeCompare(b.name));
}

const KIND_RANK: Record<StatusKind, number> = { billable: 0, included: 1, fixed: 2, internal: 2, not_billable: 2, missing: 3 };

export interface LedgerSegment {
  customer: string;
  kind: StatusKind;
  seconds: number;
}

/** Bar segments: one per customer and status, so the sage edge is the billable share. */
export function ledgerSegments(customers: CustomerGroup[]): LedgerSegment[] {
  const segs = new Map<string, LedgerSegment>();
  for (const c of customers) {
    for (const p of c.projects) {
      const kind = projectKind(p);
      const key = `${c.name}\u0000${kind}`;
      const s = segs.get(key) ?? { customer: c.name, kind, seconds: 0 };
      s.seconds += p.seconds;
      segs.set(key, s);
    }
  }
  return [...segs.values()]
    .filter((s) => s.seconds > 0)
    .sort((a, b) => KIND_RANK[a.kind] - KIND_RANK[b.kind] || b.seconds - a.seconds);
}

/** Ledger bar order: billable, included, the rest, missing; hours desc within a group. */
export function ledgerOrder(customers: CustomerGroup[]): CustomerGroup[] {
  return customers
    .filter((c) => c.seconds > 0)
    .sort((a, b) => KIND_RANK[a.kind] - KIND_RANK[b.kind] || b.seconds - a.seconds);
}

/** Hours per status for one customer, in ledger kind order (billable first, missing last). */
export function customerStatusSplit(c: CustomerGroup): { kind: StatusKind; seconds: number }[] {
  const by = new Map<StatusKind, number>();
  for (const p of c.projects) by.set(projectKind(p), (by.get(projectKind(p)) ?? 0) + p.seconds);
  return [...by]
    .map(([kind, seconds]) => ({ kind, seconds }))
    .filter((s) => s.seconds > 0)
    .sort((a, b) => KIND_RANK[a.kind] - KIND_RANK[b.kind] || b.seconds - a.seconds);
}

/** Screen-reader summary of the ledger bar. */
export function ledgerSummary(percent: number | null, totalSeconds: number, segments: LedgerSegment[], goal: number): string {
  const by = new Map<StatusKind, number>();
  for (const s of segments) by.set(s.kind, (by.get(s.kind) ?? 0) + s.seconds);
  const other = [...by].filter(([k]) => k !== "billable" && k !== "missing").reduce((a, [, v]) => a + v, 0);
  const parts = [
    ["billable", by.get("billable") ?? 0],
    ["other", other],
    ["contract missing", by.get("missing") ?? 0],
  ].filter(([, v]) => (v as number) > 0).map(([l, v]) => `${hrs(v as number)} ${l}`);
  const head = percent === null ? `Billable share unknown of ${hrs(totalSeconds)}` : `${percent}% billable of ${hrs(totalSeconds)}`;
  return `${head}: ${parts.join(", ")}; goal ${goal}%`;
}
