// Mirrors rust/crates/worklog-core/src/time_leaks_contract.rs (spec 021).
// Owned by the orchestrator: a task that needs a new shape escalates.

/** Mirrors `IgnoreReason`. */
export type IgnoreReason = "noise" | "duplicate";

/** Mirrors `TempoGapReason`. */
export type TempoGapReason = "not_sent" | "short" | "deleted";

/** Mirrors `TempoGap`. */
export interface TempoGap {
  jira_issue: string;
  seconds: number;
  in_tempo_seconds: number;
  reason: TempoGapReason;
}

/** Mirrors `ExportGap`. */
export interface ExportGap {
  seconds: number;
}

/** Mirrors `UnbilledDay`. */
export interface UnbilledDay {
  day: string;
  dismissed: string | null;
  tempo: TempoGap[];
  ticketless_seconds: number;
  export: ExportGap | null;
}

/** Mirrors `UnbilledDays`. */
export interface UnbilledDays {
  cycle_start: string;
  cycle_end: string;
  tempo_pulled_at: string | null;
  days: UnbilledDay[];
}

/** Mirrors `SourceHealth`. */
export interface SourceHealth {
  source: string;
  last_ok: string | null;
  last_err: string | null;
  events_24h: number;
}

/** Mirrors `FocusRun`. */
export interface FocusRun {
  jira_issue: string;
  started_at: string;
  ended_at: string;
  seconds: number;
}

/** Mirrors `FocusSummary`. */
export interface FocusSummary {
  day: string;
  switches: number;
  longest: FocusRun | null;
}

/** Mirrors `DeildSuggestion`. */
export interface DeildSuggestion {
  customer: string;
  deild: string;
  uses: number;
}
