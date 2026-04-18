-- Worklog schema v2. Shared between Python and the Rust hook (include_str!).
-- All CREATE statements are idempotent (IF NOT EXISTS) so the Rust hook can
-- run this on every invocation with negligible cost.

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
    company TEXT,
    session_id TEXT,
    tempo_worklog_id TEXT,
    raw_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_events_started ON events(started_at);
CREATE INDEX IF NOT EXISTS idx_events_company ON events(company);
CREATE INDEX IF NOT EXISTS idx_events_tempo ON events(tempo_worklog_id);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);

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
    company TEXT NOT NULL,
    jira_issue TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    description TEXT,
    estimated_by TEXT,
    flagged INTEGER NOT NULL DEFAULT 0,
    tempo_worklog_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_blocks_day ON blocks(day);
CREATE INDEX IF NOT EXISTS idx_blocks_tempo ON blocks(tempo_worklog_id);

CREATE TABLE IF NOT EXISTS block_events (
    block_id INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (block_id, event_id)
);
