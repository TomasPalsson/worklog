// HTTP client for the worklog Rust daemon.
//
// Both reads and writes go through the daemon. Reads used to hit
// bun:sqlite directly for raw speed, but that path was quietly broken on
// Docker Desktop — the container's read-only connection couldn't see WAL
// writes the host daemon had just committed, so unassign → re-assign
// looked like it failed until a hard reload. Routing reads through the
// daemon fixes it permanently and keeps the two paths on the same
// connection view.
//
// Two transports supported:
//   1. WORKLOG_DAEMON_URL — TCP (used by the dockerised web UI, since
//      Docker Desktop on macOS can't proxy unix sockets through its VM
//      bind mounts). Example: http://host.docker.internal:9323
//   2. Unix socket at WORKLOG_SOCKET or ~/.local/share/worklog/api.sock
//      (used for host-local clients — lower overhead, no port collision).
//
// Bun's global `fetch` accepts a `unix` option so the unix transport
// stays boringly idiomatic.

type Transport =
  | { kind: "tcp"; base: string }
  | { kind: "unix"; path: string };

function transport(): Transport {
  const url = process.env.WORKLOG_DAEMON_URL;
  if (url) return { kind: "tcp", base: url.replace(/\/$/, "") };
  return {
    kind: "unix",
    path:
      process.env.WORKLOG_SOCKET ??
      `${process.env.HOME ?? ""}/.local/share/worklog/api.sock`,
  };
}

type FetchInit = Parameters<typeof fetch>[1];

/**
 * Per-request timeout. `worklog estimate` shells out to `claude -p`
 * which can take 30+ seconds per block on larger days, so we use a
 * generous 60s cap for estimate-like routes and 10s for the rest.
 * Without this, a wedged daemon leaves the UI spinning forever.
 */
function timeoutMs(path: string): number {
  if (path.startsWith("/estimate")) return 60_000;
  if (path.startsWith("/sync")) return 30_000;
  if (path.startsWith("/jira/refresh")) return 30_000;
  if (path.startsWith("/infer")) return 30_000;
  // Jira/Tempo round-trips: project + account listing and issue creation.
  if (path.startsWith("/tickets/create")) return 30_000;
  if (path.startsWith("/projects")) return 20_000;
  if (path.startsWith("/accounts")) return 20_000;
  return 10_000;
}

async function call<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
  const t = transport();
  const signal = AbortSignal.timeout(timeoutMs(path));
  const init: FetchInit & { unix?: string } = {
    method,
    headers: { "content-type": "application/json" },
    signal,
  };
  if (t.kind === "unix") init.unix = t.path;
  if (body !== undefined) init.body = JSON.stringify(body);

  const url = t.kind === "tcp" ? `${t.base}${path}` : `http://worklog${path}`;
  let resp: Response;
  try {
    resp = await fetch(url, init);
  } catch (e) {
    // AbortSignal.timeout emits a DOMException with name "TimeoutError".
    // Rewrap so the caller can show a clearer message than the raw
    // "The operation was aborted" text.
    if ((e as Error).name === "TimeoutError") {
      throw new DaemonError(
        `daemon request to ${path} timed out after ${timeoutMs(path)}ms — ` +
          "the daemon may be stuck or unreachable",
        0,
      );
    }
    throw e;
  }
  const text = await resp.text();
  if (!resp.ok) {
    const msg =
      text.length > 0
        ? (() => {
            try {
              const j = JSON.parse(text);
              return j.error ?? text;
            } catch {
              return text;
            }
          })()
        : `HTTP ${resp.status}`;
    throw new DaemonError(msg, resp.status);
  }
  return text.length > 0 ? (JSON.parse(text) as T) : ({} as T);
}

export class DaemonError extends Error {
  constructor(message: string, public readonly status: number) {
    super(message);
    this.name = "DaemonError";
  }
}

export async function health(): Promise<{ ok: boolean; version: string }> {
  return call("GET", "/health");
}

export async function assignTicket(blockId: number, key: string | null) {
  return call("POST", `/blocks/${blockId}/ticket`, { jira_issue: key });
}

export async function setDuration(blockId: number, minutes: number) {
  return call("POST", `/blocks/${blockId}/duration`, { minutes });
}

export async function setDescription(blockId: number, description: string) {
  return call("POST", `/blocks/${blockId}/description`, { description });
}

export async function deleteBlock(blockId: number) {
  return call("POST", `/blocks/${blockId}/delete`);
}

/**
 * Flag a block as personal (or, with `isPersonal: false`, pull it back
 * into work). Personal blocks are dimmed, skipped by the estimator, and
 * excluded from Tempo sync. Only the classification changes — the
 * block's ticket, if any, is left as-is.
 */
export async function setPersonal(blockId: number, isPersonal: boolean) {
  return call("POST", `/blocks/${blockId}/personal`, { is_personal: isPersonal });
}

export async function runInfer(day: string) {
  return call<{ day: string; blocks: number; minutes: number }>("POST", "/infer", {
    day,
  });
}

export async function runEstimate(day: string, model?: string) {
  return call<{
    day: string;
    estimated: number;
    skipped: number;
    failed: number;
  }>("POST", "/estimate", model ? { day, model } : { day });
}

export async function runSync(day: string, dryRun = true) {
  return call<{
    day: string;
    dry_run: boolean;
    synced: number;
    // Tempo worklogs removed this run — orphaned when a synced block was
    // deleted or marked personal, then flushed from the deletion queue.
    deleted: number;
    skipped: number;
    errors: string[];
  }>("POST", "/sync", { day, dry_run: dryRun });
}

export async function refreshJira() {
  return call<{ tickets_written: number; source: string }>("POST", "/jira/refresh");
}

// ───────────────────── reads (v0.6) ─────────────────────

import type {
  Block,
  CommitEntry,
  CreateTicketInput,
  Event,
  JiraProject,
  JiraTicket,
  SettingsSaveResponse,
  SettingsUpdate,
  SettingsView,
  TempoAccount,
  TicketCacheMeta,
} from "./types";

interface DaySummary {
  day: string;
  total_seconds: number;
  blocks: Block[];
}

/**
 * One-shot day load: blocks enriched with event_count + sources, plus
 * the total seconds for the header. Replaces four separate direct-DB
 * queries with a single round-trip.
 */
export async function loadDaySummary(day: string): Promise<DaySummary> {
  return call<DaySummary>("GET", `/days/${day}`);
}

export async function listTickets(): Promise<{
  tickets: JiraTicket[];
  meta: TicketCacheMeta;
}> {
  return call("GET", "/tickets");
}

/**
 * Live Jira search for tickets the user is NOT assigned to. Results are
 * returned ephemerally — the daemon does NOT cache them, so the
 * estimator (which only reads tickets cached with `external = 0`) never
 * sees them. Only persists when the user picks one via
 * `rememberExternalTicket`.
 */
export async function searchTickets(q: string, limit = 20): Promise<JiraTicket[]> {
  // The daemon enforces a min length but the picker should already gate
  // this. encodeURIComponent because `q` can contain `+`, `&`, etc.
  const path = `/tickets/search?q=${encodeURIComponent(q)}&limit=${limit}`;
  return call<JiraTicket[]>("GET", path);
}

/**
 * Persist a ticket the user just picked from the live search so the
 * picker can render its summary on subsequent visits. The daemon flags
 * it `external = 1`, hiding it from the estimator while keeping it
 * available to the manual picker.
 */
export async function rememberExternalTicket(t: JiraTicket): Promise<void> {
  await call("POST", "/tickets/external", t);
}

/** Jira projects for the create-ticket project picker. */
export async function listProjects(): Promise<JiraProject[]> {
  return call<JiraProject[]>("GET", "/projects");
}

/** Tempo accounts (the customer mapping) for the create-ticket account
 * picker. */
export async function listAccounts(): Promise<TempoAccount[]> {
  return call<TempoAccount[]>("GET", "/accounts");
}

/**
 * Create a Jira issue, setting its Tempo account custom field so the
 * ticket's worklogs map to a customer. Returns the new ticket (with its
 * key) so the caller can immediately assign it to a block.
 */
export async function createTicket(input: CreateTicketInput): Promise<JiraTicket> {
  return call<JiraTicket>("POST", "/tickets/create", input);
}

/**
 * Events linked to a specific block, ordered by their own timestamp.
 * Fetched lazily on the first expand of the per-block events drill-down
 * so the day page's initial render stays cheap.
 */
export async function listBlockEvents(blockId: number): Promise<Event[]> {
  return call<Event[]>("GET", `/blocks/${blockId}/events`);
}

/**
 * Commits authored under the block's dominant project path inside its
 * window. Personal blocks and blocks without a dominant cwd come back
 * empty. Fetched lazily on first expand of the per-block commits
 * drill-down — same shape as `listBlockEvents`.
 */
export async function listBlockCommits(blockId: number): Promise<CommitEntry[]> {
  return call<CommitEntry[]>("GET", `/blocks/${blockId}/commits`);
}

// ───────────────────────── settings ─────────────────────────

/** Current settings snapshot for the panel. Token values are masked by
 * the daemon — see `SettingField.sensitive`. */
export async function loadSettings(): Promise<SettingsView> {
  return call<SettingsView>("GET", "/settings");
}

/** Apply a partial settings update. Returns the fresh snapshot plus a
 * reclassify summary when the personal patterns changed. */
export async function saveSettings(
  update: SettingsUpdate,
): Promise<SettingsSaveResponse> {
  return call<SettingsSaveResponse>("POST", "/settings", update);
}
