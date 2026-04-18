from __future__ import annotations

import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path

from worklog.config import DB_PATH, ensure_dirs

SCHEMA = """
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,          -- claude | github | gcal | jira | manual
    source_id TEXT NOT NULL,       -- stable external id for dedupe
    started_at TEXT NOT NULL,      -- ISO-8601 UTC
    ended_at TEXT,                 -- nullable for point-in-time events
    duration_seconds INTEGER,      -- derived or reported
    title TEXT NOT NULL,
    details TEXT,                  -- freeform body / diff summary
    repo TEXT,
    project_path TEXT,             -- cwd when known (Claude hook)
    jira_issue TEXT,               -- e.g. ACME-123 if parseable
    company TEXT,                  -- resolved company name (nullable until classified)
    tempo_worklog_id TEXT,         -- set after successful Tempo sync
    raw_json TEXT,                 -- original payload for auditing
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(source, source_id)
);

CREATE INDEX IF NOT EXISTS idx_events_started ON events(started_at);
CREATE INDEX IF NOT EXISTS idx_events_company ON events(company);
CREATE INDEX IF NOT EXISTS idx_events_tempo ON events(tempo_worklog_id);
"""


@contextmanager
def connect(path: Path = DB_PATH) -> Iterator[sqlite3.Connection]:
    ensure_dirs()
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def init_db(path: Path = DB_PATH) -> None:
    with connect(path) as conn:
        conn.executescript(SCHEMA)


def upsert_event(
    conn: sqlite3.Connection,
    *,
    source: str,
    source_id: str,
    started_at: datetime,
    ended_at: datetime | None = None,
    duration_seconds: int | None = None,
    title: str,
    details: str | None = None,
    repo: str | None = None,
    project_path: str | None = None,
    jira_issue: str | None = None,
    company: str | None = None,
    raw_json: str | None = None,
) -> None:
    """Insert or update on (source, source_id). Does not overwrite tempo_worklog_id."""
    conn.execute(
        """
        INSERT INTO events (
            source, source_id, started_at, ended_at, duration_seconds,
            title, details, repo, project_path, jira_issue, company, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(source, source_id) DO UPDATE SET
            ended_at = excluded.ended_at,
            duration_seconds = excluded.duration_seconds,
            title = excluded.title,
            details = excluded.details,
            repo = excluded.repo,
            project_path = excluded.project_path,
            jira_issue = excluded.jira_issue,
            company = COALESCE(events.company, excluded.company),
            raw_json = excluded.raw_json
        """,
        (
            source,
            source_id,
            started_at.isoformat(),
            ended_at.isoformat() if ended_at else None,
            duration_seconds,
            title,
            details,
            repo,
            project_path,
            jira_issue,
            company,
            raw_json,
        ),
    )
