// Unix-socket HTTP client for the worklog Rust daemon.
//
// All write endpoints are here. Reads go through bun:sqlite in ./db.ts
// — the daemon's GET /blocks/:day would work too, but direct SQLite is
// half a millisecond vs a cross-process RPC and keeps the daemon free
// for writes.
//
// Bun's global `fetch` accepts a `unix` option so this stays boringly
// idiomatic — no http.Agent, no hand-rolled socket writes.

function socketPath(): string {
  return (
    process.env.WORKLOG_SOCKET ??
    `${process.env.HOME ?? ""}/.local/share/worklog/api.sock`
  );
}

type FetchInit = Parameters<typeof fetch>[1];

async function call<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
  const init: FetchInit & { unix?: string } = {
    method,
    unix: socketPath(),
    headers: { "content-type": "application/json" },
  };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }
  // Host doesn't matter — the unix socket routes everything.
  const resp = await fetch(`http://worklog${path}`, init);
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
