from __future__ import annotations

import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime
from importlib.resources import files
from pathlib import Path

from worklog.config import DB_PATH, ensure_dirs

SCHEMA = files("worklog").joinpath("schema.sql").read_text()

_CONNECTION_PRAGMAS = (
    "PRAGMA foreign_keys = ON",
    "PRAGMA journal_mode = WAL",
    "PRAGMA synchronous = NORMAL",
    "PRAGMA busy_timeout = 2000",
)


@contextmanager
def connect(path: Path | None = None) -> Iterator[sqlite3.Connection]:
    # Resolve DB_PATH at call time so tests can monkeypatch it.
    ensure_dirs()
    conn = sqlite3.connect(path if path is not None else DB_PATH)
    conn.row_factory = sqlite3.Row
    for pragma in _CONNECTION_PRAGMAS:
        conn.execute(pragma)
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def _table_exists(conn: sqlite3.Connection, name: str) -> bool:
    return (
        conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            (name,),
        ).fetchone()
        is not None
    )


def _table_columns(conn: sqlite3.Connection, name: str) -> set[str]:
    return {row[1] for row in conn.execute(f"PRAGMA table_info({name})").fetchall()}


def _migrate_events_table(conn: sqlite3.Connection) -> None:
    cols = _table_columns(conn, "events")
    if "session_id" not in cols:
        conn.execute("ALTER TABLE events ADD COLUMN session_id TEXT")
    if "company" in cols:
        conn.execute("DROP INDEX IF EXISTS idx_events_company")
        conn.execute("ALTER TABLE events DROP COLUMN company")


def _migrate_blocks_table(conn: sqlite3.Connection) -> None:
    # v2 → v3: drop the NOT NULL company column and rebuild via copy-swap
    # (ALTER DROP COLUMN can fail on a NOT NULL column without a default on
    # some SQLite versions, so rebuild the table for safety).
    if "company" not in _table_columns(conn, "blocks"):
        return
    conn.executescript(
        """
        CREATE TABLE blocks_v3 (
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
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        INSERT INTO blocks_v3 (id, day, jira_issue, started_at, ended_at,
                               duration_seconds, description, estimated_by,
                               flagged, tempo_worklog_id, created_at)
            SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
                   description, estimated_by, flagged, tempo_worklog_id, created_at
              FROM blocks;
        DROP TABLE blocks;
        ALTER TABLE blocks_v3 RENAME TO blocks;
        """
    )


def init_db(path: Path | None = None) -> None:
    """Apply schema v3: migrate legacy v2 shape, then ensure all v3 objects."""
    with connect(path if path is not None else DB_PATH) as conn:
        if _table_exists(conn, "events"):
            _migrate_events_table(conn)
        if _table_exists(conn, "blocks"):
            _migrate_blocks_table(conn)
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
    session_id: str | None = None,
    raw_json: str | None = None,
) -> None:
    """Insert or update on (source, source_id). Does not overwrite tempo_worklog_id."""
    conn.execute(
        """
        INSERT INTO events (
            source, source_id, started_at, ended_at, duration_seconds,
            title, details, repo, project_path, jira_issue,
            session_id, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(source, source_id) DO UPDATE SET
            ended_at = COALESCE(excluded.ended_at, events.ended_at),
            duration_seconds = COALESCE(excluded.duration_seconds, events.duration_seconds),
            title = excluded.title,
            details = excluded.details,
            repo = excluded.repo,
            project_path = excluded.project_path,
            jira_issue = excluded.jira_issue,
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
            session_id,
            raw_json,
        ),
    )


def upsert_jira_ticket(
    conn: sqlite3.Connection,
    *,
    key: str,
    summary: str,
    status: str | None = None,
    project_key: str | None = None,
    updated: str | None = None,
) -> None:
    """Insert or update an open Jira ticket in the cache."""
    conn.execute(
        """
        INSERT INTO jira_tickets (key, summary, status, project_key, updated)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET
            summary = excluded.summary,
            status = excluded.status,
            project_key = excluded.project_key,
            updated = excluded.updated,
            fetched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        """,
        (key, summary, status, project_key, updated),
    )
