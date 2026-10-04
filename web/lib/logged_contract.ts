// Mirrors rust/crates/worklog-core/src/logged_contract.rs (spec 014).
// The flag rule (`day_state`) lives only in Rust; the web reads `state`.

/** Mirrors `logged_contract::LOGGED_MAX_RANGE_DAYS` (inclusive from..=to). */
export const LOGGED_MAX_RANGE_DAYS = 42;

/** Mirrors `logged_contract::DISMISS_REASON_MAX_CHARS` (after trim). */
export const DISMISS_REASON_MAX_CHARS = 80;

/** Mirrors `logged_contract::DayState`. */
export type DayState =
  | "not_fetched"
  | "pending"
  | "off"
  | "full"
  | "dismissed"
  | "under";

/** Mirrors `tempo_hub_contract::WorklogOwner`. */
export type WorklogOwner = "worklog" | "outside";

/** Mirrors `logged_contract::LoggedEntry`. */
export interface LoggedEntry {
  tempo_worklog_id: string;
  issue_id: number;
  /** `null` when the issue isn't cached — show `issue_id`. */
  jira_issue: string | null;
  seconds: number;
  description: string;
  owner: WorklogOwner;
}

/** Mirrors `logged_contract::LoggedDay`. */
export interface LoggedDay {
  day: string;
  logged_seconds: number;
  required_seconds: number | null;
  state: DayState;
  dismissal_reason: string | null;
  entries: LoggedEntry[];
}

/** Mirrors `logged_contract::LoggedRange`. */
export interface LoggedRange {
  from: string;
  to: string;
  /** The daemon's local date (`$WORKLOG_TZ`). */
  today: string;
  days: LoggedDay[];
  /** RFC3339 UTC of the newest pull; `null` when nothing is stored. */
  pulled_at: string | null;
}
