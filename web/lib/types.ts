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
  /** Repo-level folder key from the daemon, with `/.claude/worktrees/*`
   * already folded server-side (e.g. a block under
   * `…/lyfjastofnun/.claude/worktrees/ci-on-codebuild` has project
   * "lyfjastofnun"). Optional/absent on older daemon versions — callers
   * fall back to deriving the same key from `project_path`. */
  project?: string | null;
  /** "high"/"medium"/"low", from how many distinct sources fed the block. */
  confidence: "high" | "medium" | "low";
}

export interface SourceCount {
  source: string; // e.g. "github_commit", "claude_prompt", "gcal_event"
  n: number;
}

/** A gap of at least 30 minutes between two consecutive blocks on a day. */
export interface DayGap {
  started_at: string; // ISO-8601 UTC
  ended_at: string; // ISO-8601 UTC
  minutes: number;
}

/** One project's activity inside an `Overlap` window. */
export interface OverlapProject {
  project: string;
  human_events: number;
  background_events: number;
}

/** The owner's saved manual split of an `Overlap` window — fractions
 * keyed by project name, summing to 1. `null` on `Overlap.allocation`
 * means the automatic per-minute split still applies. */
export interface Allocation {
  shares: Record<string, number>;
}

/** A ≥10-minute window where ≥2 work projects were both active
 * (`GET /days/:day`'s `overlaps`) — the owner can rebalance which project
 * gets which minutes via `POST /days/:day/allocations`. */
export interface Overlap {
  started_at: string; // ISO-8601 UTC
  ended_at: string; // ISO-8601 UTC
  minutes: number;
  projects: OverlapProject[];
  allocation: Allocation | null;
}

/** One gap-bridged stretch of a project's activity. */
export interface ActivitySpan {
  started_at: string; // ISO-8601 UTC
  ended_at: string; // ISO-8601 UTC
}

/** A work project's full activity for the day (`GET /days/:day`'s
 * `activity`) — unlike blocks, which assign each minute to one owning
 * project, this is every project's actual spans, so the lanes view can
 * show a project's full activity even where another project owns the
 * block. */
export interface ProjectActivity {
  project: string;
  spans: ActivitySpan[];
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
  /** Whether the billing-cycle pruner's automatic due-check runs at all. */
  prune_enabled: boolean;
  /** Day-of-month the billing cycle starts (1-31). Default 20. */
  cycle_start_day: number;
  /** Last day-of-month the just-closed cycle can still take hours (1-31).
   * Default 23. */
  close_day: number;
  /** Editable work-hours window for browser heartbeat ingest, e.g.
   * "Mon-Fri 09:00-17:00". */
  work_hours: string;
  /** Minimum ratio the winner must beat "not enough evidence" by (RATIO_RANGE 1.0-5.0). */
  abstain_margin: number;
  /** Minimum ratio the winner must beat the runner-up by (RATIO_RANGE 1.0-5.0). */
  runner_up_ratio: number;
}

/** Partial update sent to `POST /settings`. Omitted groups are left
 * untouched; in `secrets`, only listed keys are written and an empty
 * string deletes the key. */
export interface SettingsUpdate {
  personal?: { work: string[]; personal: string[] };
  secrets?: Record<string, string>;
  timezone?: string;
  /** Omitted leaves the current enabled/disabled state untouched. */
  prune_enabled?: boolean;
  /** Omitted leaves the current cycle start day untouched. */
  cycle_start_day?: number;
  /** Omitted leaves the current close day untouched. */
  close_day?: number;
  /** Omitted leaves the work-hours window untouched. */
  work_hours?: string;
  /** Omitted leaves the abstain margin untouched. */
  abstain_margin?: number;
  /** Omitted leaves the runner-up ratio untouched. */
  runner_up_ratio?: number;
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
 * `worklog-core::billing` module — a group of blocks sharing the same
 * (customer, verkefni, ticket-or-folder). One row is one submission of the
 * invoicing form.
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
  /** How many blocks folded into this line. */
  block_count: number;
  /** Earliest block start / latest block end in the group (ISO-8601). */
  started_at: string;
  ended_at: string;
  /**
   * Distinct project_paths behind the line, most-used first. The most useful
   * hint for guessing a blank Viðskiptamaður — a folder name alone often
   * isn't enough to recognise which customer's work this was.
   */
  paths: string[];
  /** Ids of the blocks folded into this line — lets the day view group
   * blocks the way the export bills them rather than by Jira ticket. */
  block_ids: number[];
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

// ───────────────────── browser + Slack routing ─────────────────────

/** Where a routed event's project label came from (`routing_contract::LabelOrigin`).
 * `link` = the event's own text named the project (an exact repo/path mention).
 * `context` = the day's claude/shell/git_reflog activity around the event's time named it.
 * `dismissed` = the owner marked it "not work". `noise` = the daemon auto-labelled
 * it as not-work (after-hours DM, unrelated browsing) with no owner action. Both
 * are hidden by default — `GET /days/:day/routed` excludes them unless called with
 * `?include_hidden=true`. */
export type LabelOrigin = "rule" | "link" | "context" | "fix" | "guess" | "dismissed" | "noise";

/** What a hard rule matches on (`routing_contract::RuleKind`). */
export type RuleKind = "domain" | "slack_channel" | "container";

/** One row of `routing_rules` (`GET /routing/rules`). */
export interface Rule {
  id: number;
  kind: RuleKind;
  pattern: string;
  folder: string;
  created_at: string;
}

/** Body of `POST /events/:id/label`. `always` also creates a hard rule of that kind. */
export interface LabelRequest {
  folder: string;
  always: RuleKind | null;
}

/** A browser/Slack event as the UI sees it (`GET /days/:day/routed`). */
export interface RoutedEvent {
  id: number;
  source: string;
  started_at: string; // ISO-8601
  title: string;
  details: string | null;
  container: string | null;
  /** `null` = unsorted. */
  folder: string | null;
  label_origin: LabelOrigin | null;
  label_confidence: number | null;
}

/** `GET /routing/status`. */
export interface RoutingStatus {
  last_heartbeat: string | null;
  last_slack: string | null;
  classifier_reachable: boolean;
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
