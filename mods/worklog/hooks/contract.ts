// Shared shapes for the worklog Claude Code mod. Owned by the spec
// orchestrator: task code imports from here and never redeclares these.
// Field names mirror the daemon's JSON exactly (rust/crates/worklog-core).

export const DAEMON_URL = 'http://127.0.0.1:9323'
export const POLL_MS = 60_000
export const DEFAULT_REQUIRED_SECONDS = 8 * 3600
export const NUDGE_STORE_PREFIX = 'nudge:'
export const TICKET_ASKED_STORE_PREFIX = 'ticket-asked:'
export const REVIEW_PANE_ID = 'worklog-review'
export const JIRA_KEY_RE = /\b([A-Z][A-Z0-9]{1,9}-\d+)\b/

/** A local calendar day, `YYYY-MM-DD`, in the daemon's WORKLOG_TZ. */
export type LocalDay = string

/** What a session knows about where it runs; computed from the cwd. */
export type WorkContext = {
  cwd: string
  /** Under ~/Desktop/Work/ or the git `origin` remote is in the aproorg org. */
  isWork: boolean
  branch: string | undefined
  /** First Jira key in the branch name, if any. */
  ticket: string | undefined
}

/** `Block` from models.rs, the fields the mod reads. */
export type Block = {
  id: number
  day: LocalDay
  jira_issue: string | null
  started_at: string
  ended_at: string
  duration_seconds: number
  description: string | null
  is_personal: boolean
  ignored_at: string | null
  tempo_worklog_id: string | null
  exported_at: string | null
}

/** GET /days/:day — `DaySummary`, the fields the mod reads. */
export type DaySummary = {
  day: LocalDay
  total_seconds: number
  blocks: (Block & { project: string | null })[]
}

/** GET /logged?from&to — `LoggedRange`; the mod reads only `today`. */
export type LoggedRange = { today: LocalDay }

/** `GET /tasks` entry, the fields the mod reads. */
export type RecentTask = {
  key: string
  summary: string
  assigned: boolean
  last_worked_day: string | null
}

/** One day of GET /weeks/:monday/closeout. */
export type CloseoutDay = {
  day: LocalDay
  logged_seconds: number
  synced_seconds: number
  tempo_seconds: number
  required_seconds: number | null
  pending_lines: number
}

/** GET /weeks/:monday/closeout, the fields the mod reads. */
export type WeekCloseout = { days: CloseoutDay[] }

/**
 * The engine capabilities lib.ts needs. The validator only follows `$` into
 * functions declared in the same file, so lib.ts never receives `$`: each hook
 * file builds an `Io` from closures over its own `$` and passes that instead.
 */
export type Io = {
  fetch: (
    url: string,
    init?: { method?: string; headers?: Record<string, string>; body?: string },
  ) => Promise<{ status: number; ok: boolean; text: string }>
  run: (argv: string[], cwd: string) => Promise<{ exitCode: number; stdout: string }>
  after: (ms: number, fn: () => void) => { cancel(): void }
  now: () => Promise<number>
  home: string | undefined
}

/** Every daemon call resolves to this; never throws. */
export type DaemonResult<T> = { ok: true; value: T } | { ok: false; error: string }

/** The four review actions (D-09) and their daemon routes + bodies. */
export type ReviewAction =
  | { kind: 'personal'; is_personal: boolean }
  | { kind: 'ignore'; ignored: boolean }
  | { kind: 'ticket'; jira_issue: string | null }
  | { kind: 'description'; description: string }
