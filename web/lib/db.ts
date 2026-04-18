// Direct SQLite reader.
//
// Writes go through the Rust daemon (see ./daemon.ts) — this module
// never mutates. WAL mode on the writer side means we can read concurrently
// without ever blocking the daemon.
//
// All functions are async so the `bun:sqlite` import can stay dynamic:
// Next.js's build step invokes pages under Node, which can't resolve the
// `bun:` URI scheme. Dynamic import defers resolution to runtime, where
// we're always running inside bun.

import type { Database as DB } from "bun:sqlite";
import type { Block, JiraTicket, SourceCount, TicketCacheMeta } from "./types";

function dbPath(): string {
  return (
    process.env.WORKLOG_DB ??
    `${process.env.HOME ?? ""}/.local/share/worklog/worklog.db`
  );
}

let _db: DB | null = null;
async function db(): Promise<DB> {
  if (_db) return _db;
  const { Database } = await import("bun:sqlite");
  const d = new Database(dbPath(), { readonly: true });
  d.exec("PRAGMA query_only = ON");
  _db = d;
  return d;
}

export async function listBlocksForDay(day: string): Promise<Block[]> {
  const d = await db();
  const rows = d
    .query(
      `SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
              description, estimated_by, tempo_worklog_id
         FROM blocks
        WHERE day = ?
        ORDER BY started_at`,
    )
    .all(day) as Array<Omit<Block, "event_count" | "sources">>;

  if (rows.length === 0) return [];

  const ids = rows.map((r) => r.id);
  const placeholders = ids.map(() => "?").join(",");

  const counts = d
    .query<
      { block_id: number; n: number },
      number[]
    >(
      `SELECT block_id, COUNT(*) AS n FROM block_events
        WHERE block_id IN (${placeholders})
        GROUP BY block_id`,
    )
    .all(...ids);
  const countByBlock = new Map(counts.map((c) => [c.block_id, c.n]));

  const sourceRows = d
    .query<
      { block_id: number; source: string; n: number },
      number[]
    >(
      `SELECT be.block_id AS block_id, e.source AS source, COUNT(*) AS n
         FROM block_events be
         JOIN events e ON e.id = be.event_id
        WHERE be.block_id IN (${placeholders})
        GROUP BY be.block_id, e.source
        ORDER BY n DESC`,
    )
    .all(...ids);
  const sourcesByBlock = new Map<number, SourceCount[]>();
  for (const r of sourceRows) {
    const arr = sourcesByBlock.get(r.block_id) ?? [];
    arr.push({ source: r.source, n: r.n });
    sourcesByBlock.set(r.block_id, arr);
  }

  return rows.map((r) => ({
    ...r,
    event_count: countByBlock.get(r.id) ?? 0,
    sources: sourcesByBlock.get(r.id) ?? [],
  })) as Block[];
}

export async function listTickets(): Promise<JiraTicket[]> {
  const d = await db();
  return d
    .query(
      `SELECT key, summary, status, updated
         FROM jira_tickets
        ORDER BY COALESCE(updated, '') DESC, key ASC`,
    )
    .all() as JiraTicket[];
}

export async function ticketCacheMeta(): Promise<TicketCacheMeta> {
  const d = await db();
  const row = d
    .query<
      { n: number; last: string | null },
      []
    >(`SELECT COUNT(*) AS n, MAX(fetched_at) AS last FROM jira_tickets`)
    .get();
  return {
    count: row?.n ?? 0,
    last_fetched: row?.last ?? null,
  };
}

export async function dayTotalSeconds(day: string): Promise<number> {
  const d = await db();
  const row = d
    .query<
      { total: number | null },
      [string]
    >(`SELECT SUM(duration_seconds) AS total FROM blocks WHERE day = ?`)
    .get(day);
  return row?.total ?? 0;
}
