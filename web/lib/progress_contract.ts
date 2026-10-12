// Mirrors rust/crates/worklog-core/src/estimate_progress_contract.rs.

/** Mirrors `estimate_progress_contract::PersonHours`. */
export interface PersonHours {
  account_id: string;
  name: string;
  is_you: boolean;
  seconds: number;
  /** Per-day logged seconds, ascending: [YYYY-MM-DD, seconds]. */
  by_day: Array<[string, number]>;
}

/** Mirrors `estimate_progress_contract::ProgressError`. */
export type ProgressError = "jira_unavailable" | "not_configured";

/** Mirrors `estimate_progress_contract::TicketProgress`. */
export interface TicketProgress {
  key: string;
  /** null = Jira has no estimate on the ticket (never 0). */
  estimate_seconds: number | null;
  /** You first, then seconds desc; not folded. */
  people: PersonHours[];
  /** Sum of `people[].seconds`. */
  logged_seconds: number;
  /** ISO-8601 UTC; null = never fetched. */
  pulled_at: string | null;
  error: ProgressError | null;
  /** `{jira_base_url}/browse/{key}`; null/absent without Jira credentials. */
  url?: string | null;
}

/** Mirrors `estimate_progress_contract::DayProgress`. */
export interface DayProgress {
  day: string;
  tickets: TicketProgress[];
}
