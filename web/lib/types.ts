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

// ───────────────────────── billing export ─────────────────────────

/**
 * One billable line item for a day, as computed by the Rust
 * `worklog-core::billing` module: a group of blocks sharing the same
 * (dominant repo, task, kind).
 *
 * NOTE the field is `kind`, not `type` — that's the Rust struct field
 * name and what `GET /export/:day` serialises on the structured rows.
 * (The *rendered* CSV/JSON payloads use a `type` column, since that's
 * the external billing system's vocabulary.) Getting this wrong renders
 * a blank column rather than failing loudly, so it's asserted in
 * `app/actions.test.ts`.
 */
export interface BillingRow {
  /** ISO day. Render as dd.mm.yyyy for the form's Dagsetning. */
  day: string;
  /** Resolved work folder — context for the user, not a form field. */
  folder: string;
  /** Viðskiptamaður. `null` when undetectable — the user fills it in. */
  customer: string | null;
  /** Verkefni (deild). `null` unless a folder pin supplied it; never guessed. */
  verkefni: string | null;
  /** The Jira key this line came from, when there was one. Context only. */
  ticket: string | null;
  /** Overlap-safe union of the group's block intervals, unrounded. */
  seconds: number;
  /** Tímar — half-hour-rounded hours (e.g. 4, 5.5). */
  hours: number;
  /** Reikningshæfi: true = Reikningshæft. */
  billable: boolean;
  /** Texti á reikning — the block description, unmodified. */
  invoice_text: string;
  /**
   * True when no block in the group had a description, so `invoice_text` is
   * a fallback ("Work in sjukra") rather than real work text — i.e. the day
   * hasn't been through `worklog estimate` yet. Surfaced so the panel can
   * explain that, instead of the fallback looking like a bug.
   */
  needs_description: boolean;
}

/** A customer time can be billed to. */
export interface BillingCustomer {
  id?: number | null;
  name: string;
  /** Alternate spellings matched against ticket summaries / descriptions. */
  aliases: string[];
}

/**
 * Per-work-folder billing defaults.
 * `customer: null` means "shared folder — resolve from text per line".
 * `verkefni: null` means "leave the accounting key blank".
 */
export interface BillingFolderMap {
  id?: number | null;
  folder: string;
  customer: string | null;
  verkefni: string | null;
  billable: boolean;
}

/** A work folder seen in recent events with no mapping yet. */
export interface UnmappedFolder {
  folder: string;
  events: number;
}

/** `GET /billing/registry` — everything Settings → Billing needs at once. */
export interface BillingRegistry {
  customers: BillingCustomer[];
  folders: BillingFolderMap[];
  unmapped: UnmappedFolder[];
}

/**
 * `GET /export/:day`. The daemon pre-renders all three output formats so
 * the Rust renderers stay the single source of truth — the browser never
 * re-derives the billing text it puts on the clipboard.
 */
export interface ExportResponse {
  day: string;
  /** Latest `exported_at` across the day's blocks; null if never marked. */
  exported_at: string | null;
  rows: BillingRow[];
  rendered: { text: string; csv: string; json: string };
}

/** `POST /export/:day/mark` — `marked` is how many blocks were NEWLY
 * marked (0 when the day was already fully exported). */
export interface MarkExportResponse {
  day: string;
  marked: number;
  exported_at: string | null;
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
