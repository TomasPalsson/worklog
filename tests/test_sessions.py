"""Red-phase tests for session pairing and stale-session reaper."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from worklog.db import connect, init_db
from worklog.sessions import REAPER_TTL, close_session, open_session, reap_stale


@pytest.fixture
def db(tmp_path: Path) -> Path:
    p = tmp_path / "worklog.db"
    init_db(p)
    return p


def test_open_session_inserts_row(db: Path) -> None:
    now = datetime.now(UTC)
    with connect(db) as conn:
        open_session(conn, session_id="s1", started_at=now, project_path="/tmp/p")
        row = conn.execute(
            "SELECT session_id, started_at, ended_at, event_count, project_path "
            "FROM sessions WHERE session_id = ?",
            ("s1",),
        ).fetchone()
    assert row["session_id"] == "s1"
    assert row["ended_at"] is None
    assert row["event_count"] == 1
    assert row["project_path"] == "/tmp/p"


def test_open_session_idempotent_increments_count(db: Path) -> None:
    now = datetime.now(UTC)
    with connect(db) as conn:
        open_session(conn, session_id="s1", started_at=now)
        open_session(conn, session_id="s1", started_at=now + timedelta(seconds=10))
        row = conn.execute(
            "SELECT started_at, event_count FROM sessions WHERE session_id = 's1'"
        ).fetchone()
    assert row["event_count"] == 2
    # Original start must be preserved — the second call does NOT reset it
    assert row["started_at"] == now.isoformat()


def test_close_session_sets_ended_at(db: Path) -> None:
    start = datetime.now(UTC)
    end = start + timedelta(minutes=42)
    with connect(db) as conn:
        open_session(conn, session_id="s1", started_at=start)
        close_session(conn, session_id="s1", ended_at=end, end_source="stop")
        row = conn.execute(
            "SELECT ended_at, end_source FROM sessions WHERE session_id = 's1'"
        ).fetchone()
    assert row["ended_at"] == end.isoformat()
    assert row["end_source"] == "stop"


def test_close_session_does_not_overwrite(db: Path) -> None:
    """First close wins. Stop arrives first, SessionEnd arrives second — ignored."""
    start = datetime.now(UTC)
    stop_at = start + timedelta(minutes=10)
    sessionend_at = start + timedelta(minutes=11)
    with connect(db) as conn:
        open_session(conn, session_id="s1", started_at=start)
        close_session(conn, session_id="s1", ended_at=stop_at, end_source="stop")
        close_session(
            conn, session_id="s1", ended_at=sessionend_at, end_source="session_end"
        )
        row = conn.execute(
            "SELECT ended_at, end_source FROM sessions WHERE session_id = 's1'"
        ).fetchone()
    assert row["ended_at"] == stop_at.isoformat()
    assert row["end_source"] == "stop"


def test_close_session_missing_is_noop(db: Path) -> None:
    with connect(db) as conn:
        # should not raise
        close_session(
            conn, session_id="never-opened", ended_at=datetime.now(UTC), end_source="stop"
        )


def test_reap_stale_closes_old_open_sessions(db: Path) -> None:
    now = datetime.now(UTC)
    old_start = now - REAPER_TTL - timedelta(minutes=1)
    recent_start = now - timedelta(minutes=1)
    with connect(db) as conn:
        open_session(conn, session_id="stale", started_at=old_start)
        open_session(conn, session_id="fresh", started_at=recent_start)
        closed = reap_stale(conn, now=now)
        rows = {
            r["session_id"]: r
            for r in conn.execute(
                "SELECT session_id, ended_at, end_source FROM sessions"
            ).fetchall()
        }
    assert closed == 1
    assert rows["stale"]["ended_at"] == old_start.isoformat()
    assert rows["stale"]["end_source"] == "reaper"
    assert rows["fresh"]["ended_at"] is None
    assert rows["fresh"]["end_source"] is None


def test_reap_stale_leaves_already_closed_alone(db: Path) -> None:
    now = datetime.now(UTC)
    start = now - timedelta(hours=1)
    with connect(db) as conn:
        open_session(conn, session_id="s1", started_at=start)
        close_session(
            conn, session_id="s1", ended_at=start + timedelta(minutes=5), end_source="stop"
        )
        closed = reap_stale(conn, now=now)
        row = conn.execute(
            "SELECT end_source FROM sessions WHERE session_id = 's1'"
        ).fetchone()
    assert closed == 0
    assert row["end_source"] == "stop"


def test_reap_ttl_is_five_minutes() -> None:
    assert REAPER_TTL == timedelta(minutes=5)
