"""Schema v4 tests: events (no session_id, no company), blocks, jira_tickets.

v3→v4 migration drops the sessions table and events.session_id.
"""

from __future__ import annotations

import sqlite3
from datetime import UTC, datetime
from pathlib import Path

import pytest

from worklog.db import connect, init_db, upsert_event


def _columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {r[1] for r in conn.execute(f"PRAGMA table_info({table})").fetchall()}


def _tables(conn: sqlite3.Connection) -> set[str]:
    return {
        r[0]
        for r in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        ).fetchall()
    }


@pytest.fixture
def fresh_db(tmp_path: Path) -> Path:
    db = tmp_path / "worklog.db"
    init_db(db)
    return db


def test_events_columns_no_company(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        cols = _columns(conn, "events")
        assert "company" not in cols
        assert {"source", "source_id", "started_at", "jira_issue", "session_id"} <= cols


def test_sessions_table_exists(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "sessions" in _tables(conn)


def test_blocks_columns_exclude_company(fresh_db: Path) -> None:
    expected = {
        "id", "day", "jira_issue", "started_at", "ended_at",
        "duration_seconds", "description", "estimated_by", "flagged",
        "tempo_worklog_id",
    }
    with connect(fresh_db) as conn:
        cols = _columns(conn, "blocks")
        assert expected <= cols
        assert "company" not in cols


def test_jira_tickets_table(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "jira_tickets" in _tables(conn)
        cols = _columns(conn, "jira_tickets")
        assert {"key", "summary", "status", "project_key", "updated", "fetched_at"} <= cols


def test_block_events_cascades_on_delete(fresh_db: Path) -> None:
    now = datetime.now(UTC)
    with connect(fresh_db) as conn:
        upsert_event(
            conn,
            source="github_commit",
            source_id="sha1",
            started_at=now,
            title="t",
        )
        event_id = conn.execute("SELECT id FROM events").fetchone()[0]
        conn.execute(
            """
            INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
            VALUES (?, ?, ?, ?)
            """,
            ("2026-04-18", now.isoformat(), now.isoformat(), 600),
        )
        block_id = conn.execute("SELECT id FROM blocks").fetchone()[0]
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?, ?)",
            (block_id, event_id),
        )
        conn.execute("DELETE FROM blocks WHERE id = ?", (block_id,))
        assert conn.execute("SELECT COUNT(*) FROM block_events").fetchone()[0] == 0


def test_migration_v2_drops_company_keeps_sessions(tmp_path: Path) -> None:
    db = tmp_path / "v2.db"
    with sqlite3.connect(db) as c:
        c.executescript(
            """
            CREATE TABLE events (
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
            CREATE TABLE blocks (
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
            INSERT INTO blocks (day, company, jira_issue, started_at, ended_at, duration_seconds, description)
              VALUES ('2026-04-17', 'Acme', 'ACME-1', '2026-04-17T10:00:00+00:00',
                      '2026-04-17T11:00:00+00:00', 3600, 'prior work');
            """
        )

    init_db(db)
    with connect(db) as conn:
        assert "company" not in _columns(conn, "blocks")
        assert "company" not in _columns(conn, "events")
        row = conn.execute("SELECT description, jira_issue FROM blocks").fetchone()
        assert row["description"] == "prior work"
        assert row["jira_issue"] == "ACME-1"
