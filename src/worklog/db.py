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
    if "session_id" not in _table_columns(conn, "events"):
        conn.execute("ALTER TABLE events ADD COLUMN session_id TEXT")


def init_db(path: Path | None = None) -> None:
    """Apply schema v2: migrate legacy `events` shape, then ensure all v2 objects."""
    with connect(path if path is not None else DB_PATH) as conn:
        if _table_exists(conn, "events"):
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
