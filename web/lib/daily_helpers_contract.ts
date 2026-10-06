// Mirrors rust/crates/worklog-core/src/daily_helpers_contract.rs (spec 018).
// Owned by the orchestrator: a task that needs a new shape escalates.

/** Mirrors `UNDO_DEPTH`. */
export const UNDO_DEPTH = 20;

/** Mirrors `BlockChange`. */
export type BlockChange =
  | "delete"
  | "merge"
  | "split"
  | "ticket"
  | "hours"
  | "text"
  | "personal"
  | "ignored";

/** Mirrors `UndoOutcome`. */
export type UndoOutcome =
  | { outcome: "restored"; change: BlockChange; block_ids: number[] }
  | { outcome: "nothing_to_undo" }
  | { outcome: "refused_synced"; block_id: number };

/** Mirrors `PreflightCheck`. */
export type PreflightCheck = "ticketed" | "no_double_count" | "day_hours" | "line_text" | "read_back";

/** Mirrors `PreflightRow`. */
export interface PreflightRow {
  check: PreflightCheck;
  ok: boolean;
  detail: string;
  target: string | null;
}

/** Mirrors `StandupDraft`. */
export interface StandupDraft {
  today: string[];
  next: string[];
  blockers: string[];
}

/** Mirrors `PostOutcome`. */
export type PostOutcome =
  | { outcome: "posted"; permalink: string }
  | { outcome: "no_channel" }
  | { outcome: "no_thread"; channel: string }
  | { outcome: "slack_refused"; error: string };

/** Mirrors `NudgeKind`. */
export type NudgeKind = "review_requested" | "merged_not_done" | "stale";

/** Mirrors `Nudge`. */
export interface Nudge {
  kind: NudgeKind;
  text: string;
  url: string | null;
}

/** Mirrors `RECAP_TOP_GAPS`. */
export const RECAP_TOP_GAPS = 3;

/** Mirrors `ALREADY_IN_TEMPO`: a line's match status when Tempo already holds it. */
export const ALREADY_IN_TEMPO = "already_in_tempo";

/** Mirrors `MatchVerdict`. */
export type MatchVerdict =
  | { verdict: "already_in_tempo"; tempo_worklog_id: string }
  | { verdict: "different" }
  | { verdict: "unchecked"; reason: string };

/** Mirrors `RecapLine`. */
export interface RecapLine {
  jira_issue: string;
  seconds: number;
}

/** Mirrors `HeldBackLine`. */
export interface HeldBackLine {
  jira_issue: string;
  reason: string;
}

/** Mirrors `RecapGap` (RFC3339 UTC). */
export interface RecapGap {
  started_at: string;
  ended_at: string;
  minutes: number;
}

/** Mirrors `Recap`. */
export interface Recap {
  day: string;
  sent: RecapLine[];
  held_back: HeldBackLine[];
  coverage_percent: number;
  gaps: RecapGap[];
}

/** Mirrors `GapAction`. */
export type GapAction =
  | { action: "personal" }
  | { action: "break" }
  | { action: "pick_ticket"; jira_issue: string };

/** True when a checklist should block the plain Send button (FR-13). */
export function hasRedRow(rows: PreflightRow[]): boolean {
  return rows.some((r) => !r.ok);
}
