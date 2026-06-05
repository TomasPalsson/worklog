// Shared types mirroring the Rust models in worklog-core. Kept thin on
// purpose — we only list the columns the UI actually reads.

/** A row from the `events` table as the daemon returns it. */
export interface Event {
  id: number;
  source: string;
  source_id: string;
  started_at: string; // ISO-8601
  ended_at: string | null;
  duration_seconds: number | null;
  title: string;
  details: string | null;
  repo: string | null;
  project_path: string | null;
  jira_issue: string | null;
  session_id: string | null;
  tempo_worklog_id: string | null;
  raw_json: string | null;
}

export interface Block {
  id: number;
  day: string;
  jira_issue: string | null;
  started_at: string; // ISO-8601 UTC
  ended_at: string; // ISO-8601 UTC
  duration_seconds: number;
  description: string | null;
  estimated_by: "manual" | "claude" | "gap" | string | null;
  tempo_worklog_id: string | null;
  /** Auto-classified from the block's dominant project_path. Personal
   * blocks dim in the UI, skip the estimator, and aren't synced to Tempo. */
  is_personal: boolean;
  /** True when the block has been edited since its `tempo_worklog_id` was
   * written — the next sync PUTs the new values instead of duplicating. */
  dirty: boolean;
  event_count: number;
  sources: SourceCount[];
  /** Dominant working directory across the block's events — the path the
   * bulk of its commands ran in. Null for blocks with no cwd (pure
   * calendar / PR-review blocks). */
  project_path: string | null;
}

export interface SourceCount {
  source: string; // e.g. "github_commit", "claude_prompt", "gcal_event"
  n: number;
}

export interface JiraTicket {
  key: string;
  summary: string | null;
  status: string | null;
  updated: string | null;
}

export interface TicketCacheMeta {
  count: number;
  last_fetched: string | null;
}

/** A Jira project for the create-ticket picker (`GET /projects`). */
export interface JiraProject {
  key: string;
  name: string;
  id: string | null;
}

/** A Tempo account — the billing bucket that maps logged time to a
 * customer (`GET /accounts`). The chosen account's `id` is written onto
 * the new issue's account custom field. */
export interface TempoAccount {
  id: number;
  key: string;
  name: string;
  customer: string | null;
}

/** Body for `POST /tickets/create`. `account_id` is the chosen Tempo
 * account id as a string; the account is the customer mapping, so the UI
 * requires it. */
export interface CreateTicketInput {
  project_key: string;
  summary: string;
  account_id?: string;
  description?: string;
  issue_type?: string;
}

/** One commit landed inside a block's window. Returned by the daemon's
 * `/blocks/:id/commits` route, fetched lazily by the BlockCard
 * commits drill-down. `github_url` is omitted when origin isn't on
 * GitHub. */
export interface CommitEntry {
  sha: string;
  short_sha: string;
  subject: string;
  author_email: string;
  committed_at: string; // ISO-8601 with offset
  files_changed: number;
  insertions: number;
  deletions: number;
  github_url?: string;
}

// ───────────────────────── settings ─────────────────────────

/** One credential/config key as `GET /settings` returns it. Token-like
 * keys are masked: `sensitive` is true and `value` is null — only
 * `present` tells you whether one is stored. Non-sensitive keys (emails,
 * URLs, model names) come back with their `value` for prefill. */
export interface SettingField {
  key: string;
  present: boolean;
  sensitive: boolean;
  value: string | null;
}

/** Full settings snapshot backing the settings panel. */
export interface SettingsView {
  personal: { work: string[]; personal: string[] };
  secrets: SettingField[];
  /** Raw WORKLOG_TZ value, e.g. "+01:00" / "UTC" / "" (empty = UTC). */
  timezone: string;
  personal_config_path: string | null;
}

/** Partial update sent to `POST /settings`. Omitted groups are left
 * untouched; in `secrets`, only listed keys are written and an empty
 * string deletes the key. */
export interface SettingsUpdate {
  personal?: { work: string[]; personal: string[] };
  secrets?: Record<string, string>;
  timezone?: string;
}

export interface ReclassifyStats {
  total: number;
  changed_to_personal: number;
  changed_to_work: number;
  unchanged: number;
}

/** `POST /settings` response: the fresh snapshot plus, when patterns
 * changed, what the reclassify pass did. */
export interface SettingsSaveResponse extends SettingsView {
  reclassified: ReclassifyStats | null;
}

export type SourceKind = "github" | "claude" | "gcal" | "jira" | "other";

/** Collapse a raw DB `source` column into one of our display buckets. */
export function sourceKind(raw: string): SourceKind {
  if (raw.startsWith("github")) return "github";
  if (raw.startsWith("claude")) return "claude";
  if (raw.startsWith("gcal") || raw === "google_calendar") return "gcal";
  if (raw.startsWith("jira")) return "jira";
  return "other";
}
