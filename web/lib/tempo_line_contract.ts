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
}

/** True when the UI shows the "auto" tag: a ticket the Owner didn't set. */
export function isAutoTicket(
  jiraIssue: string | null,
  origin: TicketOrigin | null | undefined,
): boolean {
  return jiraIssue !== null && jiraIssue !== "" && origin !== "manual";
}
