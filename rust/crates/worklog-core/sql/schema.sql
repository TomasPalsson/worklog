-- Worklog schema v10. Shared between Python and the Rust hook (include_str!).
-- All CREATE statements are idempotent (IF NOT EXISTS) so the Rust hook can
-- run this on every invocation with negligible cost.
--
-- v10 adds the meta key/value table — daemon-persisted state such as the
-- billing-cycle pruner's last-run cutoff; see db.rs / purge.rs.
-- v9 adds billing_customers and billing_folder_map — the billing export's
-- customer/folder registry; see billing.rs / billing_registry.rs.
-- v8 adds blocks.exported_at — billing-export canary; see billing.rs /
-- purge.rs.
-- v4 adds blocks.is_personal — auto-classified from the block's dominant
-- project_path (see PersonalConfig in worklog-core::personal). Personal
-- blocks render dimmed in the UI, skip the estimator, and are excluded
-- from Tempo sync.
-- v3 dropped the "company" concept: everything is routed by jira_issue.
-- Open Jira tickets are cached in jira_tickets for the estimator + picker.

CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    source_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds INTEGER,
    title TEXT NOT NULL,
    details TEXT,
    repo TEXT,
    project_path TEXT,
    jira_issue TEXT,
    session_id TEXT,
    tempo_worklog_id TEXT,
    raw_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_events_started ON events(started_at);
CREATE INDEX IF NOT EXISTS idx_events_tempo ON events(tempo_worklog_id);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_jira ON events(jira_issue);

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT UNIQUE NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    end_source TEXT,
    project_path TEXT,
    event_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

CREATE TABLE IF NOT EXISTS blocks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    day TEXT NOT NULL,
    jira_issue TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    description TEXT,
    estimated_by TEXT,
    flagged INTEGER NOT NULL DEFAULT 0,
    tempo_worklog_id TEXT,
    is_personal INTEGER NOT NULL DEFAULT 0,
    -- `dirty = 1` means the block has been edited since it was synced
    -- (only ever set when tempo_worklog_id is present). The next
    -- `worklog sync` PUTs the new values to Tempo and clears the flag.
    dirty INTEGER NOT NULL DEFAULT 0,
    -- Billing-export "has been billed" canary — set by
    -- block_service::mark_exported when the block's day is marked
    -- exported (`worklog export --mark`). NULL means unexported.
    -- Tempo-independent; purge.rs treats this the same as a synced
    -- tempo_worklog_id.
    exported_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_blocks_day ON blocks(day);
CREATE INDEX IF NOT EXISTS idx_blocks_tempo ON blocks(tempo_worklog_id);
CREATE INDEX IF NOT EXISTS idx_blocks_jira ON blocks(jira_issue);

CREATE TABLE IF NOT EXISTS block_events (
    block_id INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (block_id, event_id)
);

-- Cache of the user's open Jira tickets, refreshed by `worklog collect jira`.
-- Feeds the UI picker, the estimator's candidate context, and — via the
-- numeric `issue_id` — Tempo Cloud's v4 worklog API (which deprecated
-- issueKey in favour of issueId).
CREATE TABLE IF NOT EXISTS jira_tickets (
    key TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    status TEXT,
    project_key TEXT,
    updated TEXT,
    issue_id TEXT,
    -- 1 if the ticket was picked manually by the user via the in-UI Jira
    -- search and is NOT in the assignee=currentUser() refresh set. The
    -- estimator filters these out so they're never offered to Claude.
    external INTEGER NOT NULL DEFAULT 0,
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_jira_tickets_updated ON jira_tickets(updated);

-- ───────────────────────── billing registry ─────────────────────────
-- Backs the billing export's Viðskiptamaður / Verkefni resolution.
-- Lives in SQLite (not a config file) so it is edited entirely from the
-- review UI's Settings → Billing section via the daemon.

-- The customers time can be billed to. `aliases` is a newline-separated
-- list matched case-insensitively against a block's Jira ticket summary
-- and description — that is how a shared infra folder (e.g. genai-infra,
-- which serves many customers) still resolves to the right customer.
CREATE TABLE IF NOT EXISTS billing_customers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    aliases TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Per-work-folder defaults. `folder` is the project root under the work
-- prefix (worktrees and sub-dirs collapse to it — see
-- billing::work_folder_for_path), e.g. `sjukra`, `apro-website`.
--
--   customer NULL → shared folder: resolve the customer from ticket /
--                   description text instead of pinning one here.
--   verkefni NULL → leave the accounting key blank for the user to pick
--                   (never model-guessed).
CREATE TABLE IF NOT EXISTS billing_folder_map (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    folder TEXT NOT NULL UNIQUE,
    customer TEXT,
    verkefni TEXT,
    -- 1 = Reikningshæft (billable), 0 = Óreikningshæft.
    billable INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- ────────────────────────────── meta ──────────────────────────────
-- Generic key/value store for daemon-persisted state that doesn't
-- warrant its own table — currently just the billing-cycle pruner's
-- latch (purge::LATCH_KEY = "last_prune_cutoff"); see purge.rs.
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
