// Mirrors rust/crates/worklog-core/src/tempo_line_contract.rs (spec 011).
// Kept out of web/lib/types.ts, which is already over the 400-line guard.

import type { LineTextOrigin } from "./clues_contract";

/** Mirrors `tempo_line_contract::HALF_HOUR_SECONDS`. */
export const HALF_HOUR_SECONDS = 1800;

/** Mirrors `tempo_line_contract::TicketOrigin`. `null` = pre-spec row. */
export type TicketOrigin = "event" | "auto" | "manual";

/** Mirrors `tempo_line_contract::TempoLineKey`. */
export interface TempoLineKey {
  day: string;
  jira_issue: string;
}

/** Mirrors `tempo_line_contract::TempoLine`. */
export interface TempoLine {
  day: string;
  jira_issue: string;
  text: string | null;
  text_origin: LineTextOrigin | null;
  /** Shown when `text` is null (not generated yet). */
  fallback_text: string;
  union_seconds: number;
  hours_override_seconds: number | null;
  effective_seconds: number;
  billing: LineBilling | null;
}

/** Mirrors `tempo_line_contract::BillingClass`. */
export type BillingClass = "billable" | "included" | "not_billable";

/** Mirrors `tempo_line_contract::LineBilling` (Mirres facts for one line). */
export interface LineBilling {
  account_key: string;
  project: string | null;
  project_type: string | null;
  class: BillingClass;
  warning: string | null;
  /** `customer.short_name`, else `customer.name`. */
  customer: string | null;
  details: MirresDetails | null;
}

/** Mirrors `tempo_line_contract::MirresPerson`. */
export interface MirresPerson {
  name: string;
  email: string | null;
}

/** Mirrors `tempo_line_contract::MirresDetails`. Never prices. */
export interface MirresDetails {
  customer_name: string | null;
  owner: MirresPerson | null;
  responsible: MirresPerson | null;
  team_lead: MirresPerson | null;
  period: string | null;
  allowance_hours: number | null;
  /** Null when Mirres hides it (fewer than 3 people). */
  used_hours: number | null;
  remaining_hours: number | null;
  usage_status: string | null;
  due_date: string | null;
  contract_url: string | null;
}

/** Mirrors `tempo_line_contract::MirresDay` (`GET /mirres/overview`). */
export interface MirresDay {
  day: string;
  fetched_at: string;
  /** ALL of the day's lines; `billing` is null where Mirres had no row. */
  lines: TempoLine[];
}

export const BILLING_LABEL: Record<BillingClass, string> = {
  billable: "billable",
  included: "included",
  not_billable: "not billable",
};

export const BILLING_HINT: Record<BillingClass, string> = {
  billable: "Mirres: billed to the customer",
  included: "Mirres: covered by the contract's included hours — counts toward the 70% goal",
  not_billable: "Mirres: not billed (internal, or contract missing / used up)",
};

/**
 * Share of the day's line hours that count as billed (billable or included),
 * 0-100. `null` when no line has Mirres data or there are no hours.
 */
export function billablePercent(lines: TempoLine[]): number | null {
  if (!lines.some((l) => l.billing)) return null;
  const total = lines.reduce((a, l) => a + l.effective_seconds, 0);
  if (total <= 0) return null;
  const billed = lines.reduce(
    (a, l) =>
      l.billing && l.billing.class !== "not_billable" ? a + l.effective_seconds : a,
    0,
  );
  return Math.round((100 * billed) / total);
}

/** True when the UI shows the "auto" tag: a ticket the Owner didn't set. */
export function isAutoTicket(
  jiraIssue: string | null,
  origin: TicketOrigin | null | undefined,
): boolean {
  return jiraIssue !== null && jiraIssue !== "" && origin !== "manual";
}
