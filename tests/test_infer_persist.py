"""Tests for load_day_events + persist_blocks."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from worklog.db import connect, init_db, upsert_event
from worklog.infer import build_blocks
from worklog.infer_persist import load_day_events, persist_blocks


@pytest.fixture
def db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    p = tmp_path / "worklog.db"
    monkeypatch.setattr("worklog.db.DB_PATH", p)
    init_db(p)
    return p


def _add(conn, ts: datetime, company: str, source: str = "github_commit", **extra) -> None:
    upsert_event(
        conn,
        source=source,
        source_id=f"{source}-{ts.isoformat()}",
        started_at=ts,
        title=f"event at {ts}",
        company=company,
        duration_seconds=extra.get("duration_seconds"),
        jira_issue=extra.get("jira_issue"),
    )


def test_load_day_events_filters_by_date(db: Path) -> None:
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, tzinfo=UTC), "Acme")
        _add(conn, datetime(2026, 4, 17, 10, tzinfo=UTC), "Acme")
    events = load_day_events(date=datetime(2026, 4, 18).date())
    assert len(events) == 1
    assert events[0].ts.day == 18


def test_persist_blocks_writes_rows_and_join(db: Path) -> None:
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC), "Acme")
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC), "Acme")
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)
    blocks = build_blocks(events)
    persist_blocks(blocks, day=day)

    with connect(db) as conn:
        block_rows = conn.execute("SELECT company, duration_seconds FROM blocks").fetchall()
        join_count = conn.execute("SELECT COUNT(*) FROM block_events").fetchone()[0]
    assert len(block_rows) == 1
    assert block_rows[0]["company"] == "Acme"
    assert join_count == 2


def test_persist_blocks_is_idempotent(db: Path) -> None:
    """Re-running infer + persist for the same day replaces prior blocks."""
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC), "Acme")
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC), "Acme")
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)

    persist_blocks(build_blocks(events), day=day)
    persist_blocks(build_blocks(events), day=day)  # second run

    with connect(db) as conn:
        n = conn.execute("SELECT COUNT(*) FROM blocks WHERE day = ?", (day.isoformat(),)).fetchone()[0]
    assert n == 1


def test_persist_preserves_tempo_worklog_id(db: Path) -> None:
    """If a block was already synced to Tempo, re-infer must not wipe tempo_worklog_id."""
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC), "Acme")
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC), "Acme")
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)

    persist_blocks(build_blocks(events), day=day)
    with connect(db) as conn:
        conn.execute("UPDATE blocks SET tempo_worklog_id = 'TW-99' WHERE day = ?", (day.isoformat(),))

    persist_blocks(build_blocks(events), day=day)
    with connect(db) as conn:
        row = conn.execute("SELECT tempo_worklog_id FROM blocks").fetchone()
    assert row["tempo_worklog_id"] == "TW-99"


def test_load_handles_session_duration(db: Path) -> None:
    """Claude events with duration_seconds from SessionStart→Stop pairing are used."""
    start = datetime(2026, 4, 18, 10, 0, tzinfo=UTC)
    with connect(db) as conn:
        upsert_event(
            conn,
            source="claude",
            source_id="sess-1:Stop:x",
            started_at=start,
            ended_at=start + timedelta(minutes=30),
            duration_seconds=1800,
            title="session",
            company="Acme",
            session_id="sess-1",
        )
    events = load_day_events(date=start.date())
    assert events[0].duration_seconds == 1800
