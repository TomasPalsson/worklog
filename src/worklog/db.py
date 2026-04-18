from __future__ import annotations

import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime
from importlib.resources import files
from pathlib import Path

from worklog.config import DB_PATH, ensure_dirs

SCHEMA = files("worklog").joinpath("schema.sql").read_text()


@contextmanager
def connect(path: Path = DB_PATH) -> Iterator[sqlite3.Connection]:
    ensure_dirs()
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    conn.execute("PRAGMA journal_mode = WAL")
    conn.execute("PRAGMA synchronous = NORMAL")
    conn.execute("PRAGMA busy_timeout = 2000")
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def _migrate_events_table(conn: sqlite3.Connection) -> None:
    """Add v2 columns to a pre-existing v1 events table.

    Idempotent: safe to call when `events` already has the new columns.
    """
    cols = {row[1] for row in conn.execute("PRAGMA table_info(events)").fetchall()}
    if "session_id" not in cols:
        conn.execute("ALTER TABLE events ADD COLUMN session_id TEXT")


def init_db(path: Path = DB_PATH) -> None:
    """Create schema v2 from scratch or migrate an existing v1 database."""
    with connect(path) as conn:
        # ALTER first, then executescript (CREATE IF NOT EXISTS) for fresh bits.
        existing = {
            r[0]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            ).fetchall()
        }
        if "events" in existing:
            _migrate_events_table(conn)
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
    session_id: str | None = None,
    raw_json: str | None = None,
) -> None:
    """Insert or update on (source, source_id). Does not overwrite tempo_worklog_id."""
    conn.execute(
        """
        INSERT INTO events (
            source, source_id, started_at, ended_at, duration_seconds,
            title, details, repo, project_path, jira_issue, company,
            session_id, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(source, source_id) DO UPDATE SET
            ended_at = COALESCE(excluded.ended_at, events.ended_at),
            duration_seconds = COALESCE(excluded.duration_seconds, events.duration_seconds),
            title = excluded.title,
            details = excluded.details,
            repo = excluded.repo,
            project_path = excluded.project_path,
            jira_issue = excluded.jira_issue,
            company = COALESCE(events.company, excluded.company),
            session_id = COALESCE(events.session_id, excluded.session_id),
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
            session_id,
            raw_json,
        ),
    )
