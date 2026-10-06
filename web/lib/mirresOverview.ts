// Pure aggregation behind the /mirres page: Mirres facts grouped per
// project account, and per-day match / billable summaries.

import { billablePercent } from "./tempo_line_contract";
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
  };
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

/** "06.10.2026 15:59": 24h, local time, fixed shape (not locale-dependent). */
export function formatFetchedAt(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
