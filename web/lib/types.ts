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
  event_count: number;
  sources: SourceCount[];
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

export type SourceKind = "github" | "claude" | "gcal" | "jira" | "other";

/** Collapse a raw DB `source` column into one of our display buckets. */
export function sourceKind(raw: string): SourceKind {
  if (raw.startsWith("github")) return "github";
  if (raw.startsWith("claude")) return "claude";
  if (raw.startsWith("gcal") || raw === "google_calendar") return "gcal";
  if (raw.startsWith("jira")) return "jira";
  return "other";
}
