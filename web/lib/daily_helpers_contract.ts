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

/** True when a checklist should block the plain Send button (FR-13). */
export function hasRedRow(rows: PreflightRow[]): boolean {
  return rows.some((r) => !r.ok);
}
