"""Red-phase tests for schema v2: sessions + blocks + events.session_id."""

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


def test_sessions_table_exists(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "sessions" in _tables(conn)


def test_sessions_columns(fresh_db: Path) -> None:
    expected = {
        "id",
        "session_id",
        "started_at",
        "ended_at",
        "end_source",
        "project_path",
        "event_count",
    }
    with connect(fresh_db) as conn:
        assert expected <= _columns(conn, "sessions")


def test_sessions_session_id_unique(fresh_db: Path) -> None:
    now = datetime.now(UTC).isoformat()
    with connect(fresh_db) as conn:
        conn.execute(
            "INSERT INTO sessions (session_id, started_at) VALUES (?, ?)",
            ("dup", now),
        )
        with pytest.raises(sqlite3.IntegrityError):
            conn.execute(
                "INSERT INTO sessions (session_id, started_at) VALUES (?, ?)",
                ("dup", now),
            )


def test_blocks_table_exists(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "blocks" in _tables(conn)


def test_blocks_columns(fresh_db: Path) -> None:
    expected = {
        "id",
        "day",
        "company",
        "jira_issue",
        "started_at",
        "ended_at",
        "duration_seconds",
        "description",
        "estimated_by",
        "flagged",
        "tempo_worklog_id",
    }
    with connect(fresh_db) as conn:
        assert expected <= _columns(conn, "blocks")


def test_block_events_join_table(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "block_events" in _tables(conn)
        cols = _columns(conn, "block_events")
        assert {"block_id", "event_id"} <= cols


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
            INSERT INTO blocks (day, company, started_at, ended_at, duration_seconds)
            VALUES (?, ?, ?, ?, ?)
            """,
            ("2026-04-18", "Acme", now.isoformat(), now.isoformat(), 600),
        )
        block_id = conn.execute("SELECT id FROM blocks").fetchone()[0]
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?, ?)",
            (block_id, event_id),
        )
        conn.execute("DELETE FROM blocks WHERE id = ?", (block_id,))
        rows = conn.execute("SELECT COUNT(*) FROM block_events").fetchone()[0]
        assert rows == 0


def test_events_has_session_id_column(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        assert "session_id" in _columns(conn, "events")


def test_events_session_id_indexed(fresh_db: Path) -> None:
    with connect(fresh_db) as conn:
        indexes = {
            r[0]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='index'"
            ).fetchall()
        }
        assert "idx_events_session" in indexes


def test_upsert_event_accepts_session_id(fresh_db: Path) -> None:
    """upsert_event must accept and persist session_id."""
    with connect(fresh_db) as conn:
        upsert_event(
            conn,
            source="claude",
            source_id="s1:SessionStart",
            started_at=datetime.now(UTC),
            title="SessionStart",
            session_id="s1",
        )
        row = conn.execute(
            "SELECT session_id FROM events WHERE source_id = ?",
            ("s1:SessionStart",),
        ).fetchone()
        assert row["session_id"] == "s1"


def test_migration_v1_to_v2_preserves_events(tmp_path: Path) -> None:
    """A v1 DB (events only, no session_id) migrates to v2 without losing data."""
    db = tmp_path / "v1.db"
    # Simulate v1 schema: only the events table, no sessions/blocks, no session_id
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
                tempo_worklog_id TEXT,
                raw_json TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                UNIQUE(source, source_id)
            );
            """
        )
        c.execute(
            """
            INSERT INTO events (source, source_id, started_at, title)
            VALUES ('legacy', 'x', '2026-04-01T12:00:00+00:00', 'kept row')
            """
        )

    # Run init_db on the existing file — should migrate, not error
    init_db(db)

    with connect(db) as conn:
        assert "sessions" in _tables(conn)
        assert "blocks" in _tables(conn)
        assert "session_id" in _columns(conn, "events")
        row = conn.execute(
            "SELECT title FROM events WHERE source_id = 'x'"
        ).fetchone()
        assert row["title"] == "kept row"
