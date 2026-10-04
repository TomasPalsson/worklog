-- Worklog schema v11. Shared between Python and the Rust hook (include_str!).
-- All CREATE statements are idempotent (IF NOT EXISTS) so the Rust hook can
-- run this on every invocation with negligible cost.
--
-- v16 adds blocks.described_seconds — the block's wall-clock span when its
-- description was last written; see db.rs / infer_carry.rs.
-- v15 adds events.elsewhere and billing_line_texts — attribution and
-- billing-line texts (spec 006); see db.rs / clues_contract.rs.
-- v11 adds events.container/label_origin/label_confidence and the
-- routing_rules table — browser/Slack event routing; see db.rs / routing.rs
-- / routing_contract.rs.
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
    -- Firefox container name, e.g. "Personal" (routing v11).
    container TEXT,
    -- Where events.project_path's label came from: 'rule' | 'fix' | 'guess'
    -- (routing_contract::LabelOrigin). NULL = unsorted.
    label_origin TEXT,
    -- Model confidence when label_origin = 'guess'. NULL otherwise.
    label_confidence REAL,
    -- 0 = normal event, eligible for inference.
    -- 1 = an org commit/PR whose sha is in no local clone (spec 006,
    -- D-07): kept out of every block, listed in the day's "done
    -- elsewhere" list instead. See elsewhere.rs.
    -- 2 = owner-moved into a block by hand (FR-06, elsewhere::move_into_block):
    -- never votes, extends or lists again; a collector re-run or the
    -- upgrade_006 re-resolve must never touch this row back to 0/1.
    elsewhere INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_events_started ON events(started_at);
CREATE INDEX IF NOT EXISTS idx_events_tempo ON events(tempo_worklog_id);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_jira ON events(jira_issue);
-- Covers billing_registry::unmapped_folders (the Billing registry, loaded by every day
-- page) so it never reads the wide event rows. Leading on project_path, not started_at,
-- so no started_at-ordered query can pick it up and change its tie order.
CREATE INDEX IF NOT EXISTS idx_events_path_started ON events(project_path, started_at);

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
    -- User-ignored block (ISO time). Always paired with is_personal = 1 so
    -- every personal exclusion applies; survives rebuilds via infer.rs carry.
    ignored_at TEXT,
    -- Wall-clock span (ended_at - started_at, seconds) of the block at the
    -- moment its description was last written by the estimator. Compared
    -- against, not the current duration_seconds, so a description survives
    -- gradual growth but is dropped once the block outgrows the length it
    -- was actually written for. NULL for manual text and pre-migration rows.
    described_seconds INTEGER,
    -- Who set jira_issue: 'event' (the block's events share one key),
    -- 'auto' (estimator pick) or 'manual' (Owner). NULL = pre-v17 row,
    -- behaves like 'auto'.
    ticket_origin TEXT CHECK(ticket_origin IN ('event', 'auto', 'manual')),
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

-- Read-back of the user's Tempo worklogs. `owner` is 'worklog' for rows this
-- tool pushed and 'outside' for everything entered elsewhere.
CREATE TABLE IF NOT EXISTS tempo_remote_worklogs (
    tempo_worklog_id TEXT PRIMARY KEY,
    day TEXT NOT NULL,
    issue_id INTEGER NOT NULL,
    jira_issue TEXT,
    seconds INTEGER NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    owner TEXT NOT NULL CHECK (owner IN ('worklog','outside')),
    pulled_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tempo_remote_day_issue ON tempo_remote_worklogs(day, issue_id);

CREATE TABLE IF NOT EXISTS tempo_required_days (
    day TEXT PRIMARY KEY,
    required_seconds INTEGER NOT NULL CHECK (required_seconds >= 0),
    pulled_at TEXT NOT NULL
);

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
    -- 1 = the folder holds many customers' tenants (spec 005); the export
    -- splits each block between customers instead of billing this pin.
    multi_tenant INTEGER NOT NULL DEFAULT 0,
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

-- ─────────────────────────── routing rules ───────────────────────────
-- Hard rules for browser/Slack event routing (routing_contract::RuleKind /
-- Rule). A rule always wins over a model guess; see routing.rs.
CREATE TABLE IF NOT EXISTS routing_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 'domain' | 'slack_channel' | 'container' (RuleKind).
    kind TEXT NOT NULL,
    pattern TEXT NOT NULL,
    folder TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(kind, pattern)
);

-- ───────────────────────── overlap allocations ─────────────────────────
-- The owner's manual split of a `overlaps::Overlap` window — "70% on
-- vitinn-infra, 30% on lyfjastofnun" — read back by `infer_allocations`
-- so re-inferring the day honours the choice instead of the automatic
-- per-minute owner. See overlaps.rs / overlap_store.rs.
CREATE TABLE IF NOT EXISTS overlap_allocations (
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    shares TEXT NOT NULL, -- JSON object {project: fraction}
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(day, started_at, ended_at)
);

-- ───────────────────────────── tenants (spec 005) ─────────────────────────────
-- Multi-tenant infra folders (e.g. `vitinn-infra`, `genai-infra`) serve many
-- customers' tenants under one work folder; see billing_folder_map.multi_tenant,
-- tenants.rs and tenant_contract.rs.

-- Tenant roots under a multi-tenant folder (tenant_contract::TenantRoot).
-- `root` is `/`-separated relative to the folder; a `*` segment matches
-- exactly one directory name.
CREATE TABLE IF NOT EXISTS billing_tenant_roots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    folder TEXT NOT NULL,
    root TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(folder, root)
);

-- The owner's hand-set tenant → customer links (tenant_contract::TenantLink).
-- `customer NULL` + `ignored = 0` means "not yet linked" (Unmatched);
-- `ignored = 1` means "not a customer" and gives no clue.
CREATE TABLE IF NOT EXISTS billing_tenant_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    folder TEXT NOT NULL,
    tenant TEXT NOT NULL,
    customer TEXT,
    ignored INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(folder, tenant)
);

-- The owner's hand-set customer split for one block (tenant_contract::
-- CustomerShares), keyed by day + started_at so it survives re-infer
-- rebuilding block ids. `shares` is a JSON object {customer: fraction}.
-- `rows_json` (spec 006) is the v2 (customer, deild, %) split
-- (deild_contract::BlockShares); NULL until the block is re-saved. Read
-- in preference to `shares`, which stays for v1 rows (deild_contract::
-- ShareRow with `deild: None`) — see tenant_shares::parse_rows.
CREATE TABLE IF NOT EXISTS block_customer_shares (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    shares TEXT NOT NULL,
    rows_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(day, started_at)
);

-- ───────────────────────── deildir + super blocks (spec 006) ─────────────────────────
-- A deildir list per customer (deild_contract::Deild) — the accounting
-- keys (Verkefni) the Owner bills that customer under. `keywords` are
-- matched like billing_customers.aliases (billing_registry::alias_matches).
CREATE TABLE IF NOT EXISTS billing_deildir (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    customer TEXT NOT NULL,
    name TEXT NOT NULL,
    keywords TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(customer, name)
);

-- A block's last-seen resolved (customer, deild, description)
-- (deild_contract::ResolutionSnapshot), keyed like block_customer_shares
-- so it survives rebuilds. change_log::refresh_day diffs against this
-- after every write to catch changes an automatic writer makes
-- indirectly (spec A3). `parts_json` is the resolved ShareRow list.
CREATE TABLE IF NOT EXISTS block_resolution_snapshots (
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    description TEXT,
    parts_json TEXT NOT NULL,
    PRIMARY KEY(day, started_at)
);

-- One logged change to a block's customer, deild, split or description
-- (deild_contract::BlockChange). `seen_at NULL` means unseen (FR-11);
-- `batch` groups one writer's run into one pop-up (FR-10, D-07).
CREATE TABLE IF NOT EXISTS block_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    field TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    source TEXT NOT NULL,
    batch TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    seen_at TEXT
);

-- ───────────────────────── billing-line texts (spec 006) ─────────────────────────
-- One generated or hand-edited Icelandic text per billing line
-- (clues_contract::BillingLineKey), the "Texti á reikning" a boss or
-- customer reads. `customer = ''` when the line's customer is
-- unresolved. `origin = 'manual'` rows are never overwritten by
-- generation (FR-31); see line_text.rs.
CREATE TABLE IF NOT EXISTS billing_line_texts (
    day TEXT NOT NULL,
    folder TEXT NOT NULL,
    customer TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL,
    origin TEXT NOT NULL CHECK(origin IN ('generated', 'manual')),
    updated_at TEXT NOT NULL,
    PRIMARY KEY(day, folder, customer)
);

-- ───────────────────────── tempo ticket lines (spec 011) ─────────────────────────
-- One row per (day, jira_issue): the stored worklog text and optional
-- hours override for that ticket line. `hours_override_seconds` is a
-- positive multiple of 30 minutes; see tempo_line_contract.rs.
CREATE TABLE IF NOT EXISTS tempo_line_texts (
    day TEXT NOT NULL,
    jira_issue TEXT NOT NULL,
    text TEXT,
    text_origin TEXT CHECK(text_origin IN ('generated', 'manual')),
    source_hash TEXT,
    hours_override_seconds INTEGER CHECK(hours_override_seconds IS NULL OR (hours_override_seconds > 0 AND hours_override_seconds % 1800 = 0)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY(day, jira_issue)
);

-- ───────────────────── transcript file cache (perf T2) ─────────────────────
-- Per-file fingerprint for the Claude transcript collector's tick skip
-- (collectors/claude_transcripts.rs, claude_helpers.rs): a `.jsonl` whose
-- size + mtime_ns haven't changed since it was last fully read for this
-- exact [since_ts, until_ts) window is skipped outright instead of
-- re-read + re-parsed. `claimed_uuids_json` is the JSON array of line
-- uuids the file itself won the cross-file dedupe race for last time (a
-- resumed session can copy an older file's lines, same uuid, into a new
-- file), so a skip still reseeds the run's `seen` set exactly as a full
-- read would. `events_written` replays into CollectReport so a skip never
-- changes `worklog day`'s printed "events=N". `extra_key` is an additional
-- cache-key component for state a file's own bytes don't capture — the
-- background-job session set for session files, empty for helper files.
-- ───────────────────────── session pins (spec 008) ─────────────────────────
-- A statement "session S is for customer C from time T on"
-- (design.md §1). `source` is 'claude' (set by the pin command) or
-- 'inherited' (copied into a new session on the same branch at start).
-- See session_pins.rs.
CREATE TABLE IF NOT EXISTS session_pins (
    session_id TEXT NOT NULL,
    customer TEXT NOT NULL,
    from_at TEXT NOT NULL,
    folder TEXT NOT NULL,
    branch TEXT,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (session_id, from_at)
);

CREATE INDEX IF NOT EXISTS idx_session_pins_branch ON session_pins(folder, branch, from_at);

-- The card kept for a block after its raw events are deleted (spec 010).
-- `json` is a digest_contract::BlockDigest. A card is written once and
-- never replaced.
CREATE TABLE IF NOT EXISTS block_digest (
    block_id INTEGER PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    built_at TEXT NOT NULL,
    json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transcript_file_cache (
    path TEXT NOT NULL,
    since_ts INTEGER NOT NULL,
    until_ts INTEGER NOT NULL,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    claimed_uuids_json TEXT NOT NULL,
    events_written INTEGER NOT NULL DEFAULT 0,
    extra_key TEXT NOT NULL DEFAULT '',
    PRIMARY KEY(path, since_ts, until_ts)
);

CREATE TABLE IF NOT EXISTS account_clues (
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    clue TEXT NOT NULL,
    hits INTEGER NOT NULL DEFAULT 0,
    wrong INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, clue)
);

CREATE TABLE IF NOT EXISTS account_decisions (
    id INTEGER PRIMARY KEY,
    decided_at TEXT NOT NULL,
    summary TEXT NOT NULL,
    picked_id TEXT NOT NULL,
    guessed_id TEXT,
    correct INTEGER NOT NULL,
    clues TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS account_ticket_counts (
    account_id TEXT PRIMARY KEY,
    account_name TEXT NOT NULL,
    tickets INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tempo_day_dismissals (
    day TEXT PRIMARY KEY,
    reason TEXT NOT NULL,
    dismissed_at TEXT NOT NULL
);
