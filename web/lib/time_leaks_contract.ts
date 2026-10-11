// Mirrors rust/crates/worklog-core/src/time_leaks_contract.rs (spec 021).
// Owned by the orchestrator: a task that needs a new shape escalates.

/** Mirrors `IgnoreReason`. */
export type IgnoreReason = "noise" | "duplicate";

/** Mirrors `LooseEndMissing`. */
export type LooseEndMissing = "tempo" | "export";

/** Mirrors `LooseEnd`. */
export interface LooseEnd {
  block_id: number;
  day: string;
  jira_issue: string;
  started_at: string;
  ended_at: string;
  seconds: number;
  description: string | null;
  missing: LooseEndMissing[];
}

/** Mirrors `LooseEnds`. */
export interface LooseEnds {
  cycle_start: string;
  cycle_end: string;
  items: LooseEnd[];
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
