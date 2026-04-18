// Spin up a temp SQLite, write the exact Rust schema, seed rows, check reads.
import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let tmpDir: string;
let dbFile: string;

beforeAll(() => {
  tmpDir = mkdtempSync(join(tmpdir(), "worklog-db-test-"));
  dbFile = join(tmpDir, "worklog.db");

  // Point the db module at the temp file before importing it.
  process.env.WORKLOG_DB = dbFile;

  // Seed with the same schema the Rust daemon uses.
  const schemaPath = join(
    import.meta.dir,
    "..",
    "..",
    "rust",
    "crates",
    "worklog-core",
    "sql",
    "schema.sql",
  );
  const schema = readFileSync(schemaPath, "utf8");
  const d = new Database(dbFile);
  d.exec(schema);

  d.exec(
    `INSERT INTO blocks (id, day, jira_issue, started_at, ended_at, duration_seconds, description, estimated_by)
     VALUES
       (1, '2026-04-18', 'PROJ-1', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 'Morning review', 'manual'),
       (2, '2026-04-18', NULL,     '2026-04-18T10:00:00+00:00', '2026-04-18T10:45:00+00:00', 2700, NULL,            'gap')`,
  );
  d.exec(
    `INSERT INTO events (id, source, source_id, started_at, title, jira_issue)
     VALUES
       (1, 'github_commit', 'abc1', '2026-04-18T09:05:00+00:00', 'feat: ship thing', 'PROJ-1'),
       (2, 'claude_prompt', 'p1',   '2026-04-18T09:10:00+00:00', 'fix the bug',      'PROJ-1'),
       (3, 'github_commit', 'abc2', '2026-04-18T10:10:00+00:00', 'cleanup',          NULL)`,
  );
  d.exec(
    `INSERT INTO block_events (block_id, event_id) VALUES (1, 1), (1, 2), (2, 3)`,
  );
  d.exec(
    `INSERT INTO jira_tickets (key, summary, status, updated, fetched_at)
     VALUES
       ('PROJ-1', 'Ship the thing', 'In Progress', '2026-04-17T12:00:00+00:00', '2026-04-18T08:00:00+00:00'),
       ('PROJ-2', 'Other thing',    'To Do',       '2026-04-15T09:00:00+00:00', '2026-04-18T08:00:00+00:00')`,
  );
  d.close();
});

afterAll(() => {
  rmSync(tmpDir, { recursive: true, force: true });
  delete process.env.WORKLOG_DB;
});

describe("listBlocksForDay", () => {
  it("returns blocks with event counts and source breakdowns", async () => {
    const { listBlocksForDay } = await import("./db");
    const blocks = await listBlocksForDay("2026-04-18");
    expect(blocks).toHaveLength(2);

    const first = blocks.find((b) => b.id === 1)!;
    expect(first.jira_issue).toBe("PROJ-1");
    expect(first.event_count).toBe(2);
    const sources = new Set(first.sources.map((s) => s.source));
    expect(sources.has("github_commit")).toBe(true);
    expect(sources.has("claude_prompt")).toBe(true);

    const second = blocks.find((b) => b.id === 2)!;
    expect(second.jira_issue).toBeNull();
    expect(second.event_count).toBe(1);
  });

  it("returns empty for unknown days", async () => {
    const { listBlocksForDay } = await import("./db");
    expect(await listBlocksForDay("2099-01-01")).toEqual([]);
  });
});

describe("listTickets", () => {
  it("returns tickets ordered by updated desc", async () => {
    const { listTickets } = await import("./db");
    const tickets = await listTickets();
    expect(tickets.length).toBe(2);
    expect(tickets[0].key).toBe("PROJ-1");
  });
});

describe("ticketCacheMeta", () => {
  it("reports count and last fetch time", async () => {
    const { ticketCacheMeta } = await import("./db");
    const m = await ticketCacheMeta();
    expect(m.count).toBe(2);
    expect(m.last_fetched).toBe("2026-04-18T08:00:00+00:00");
  });
});

describe("dayTotalSeconds", () => {
  it("sums duration_seconds for the day", async () => {
    const { dayTotalSeconds } = await import("./db");
    expect(await dayTotalSeconds("2026-04-18")).toBe(1800 + 2700);
  });
});
