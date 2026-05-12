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
  Event,
  JiraTicket,
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
