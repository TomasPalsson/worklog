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


def _add(conn, ts: datetime, source: str = "github_commit", **extra) -> None:
    upsert_event(
        conn,
        source=source,
        source_id=f"{source}-{ts.isoformat()}",
        started_at=ts,
        title=f"event at {ts}",
        duration_seconds=extra.get("duration_seconds"),
        jira_issue=extra.get("jira_issue"),
    )


def test_load_day_events_filters_by_date(db: Path) -> None:
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, tzinfo=UTC))
        _add(conn, datetime(2026, 4, 17, 10, tzinfo=UTC))
    events = load_day_events(date=datetime(2026, 4, 18).date())
    assert len(events) == 1
    assert events[0].ts.day == 18


def test_persist_blocks_writes_rows_and_join(db: Path) -> None:
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC), jira_issue="ACME-1")
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC), jira_issue="ACME-1")
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)
    blocks = build_blocks(events)
    persist_blocks(blocks, day=day)

    with connect(db) as conn:
        rows = conn.execute(
            "SELECT jira_issue, duration_seconds FROM blocks"
        ).fetchall()
        join_count = conn.execute("SELECT COUNT(*) FROM block_events").fetchone()[0]
    assert len(rows) == 1
    assert rows[0]["jira_issue"] == "ACME-1"
    assert join_count == 2


def test_persist_blocks_is_idempotent(db: Path) -> None:
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC))
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC))
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)

    persist_blocks(build_blocks(events), day=day)
    persist_blocks(build_blocks(events), day=day)

    with connect(db) as conn:
        n = conn.execute(
            "SELECT COUNT(*) FROM blocks WHERE day = ?", (day.isoformat(),)
        ).fetchone()[0]
    assert n == 1


def test_persist_preserves_tempo_worklog_id_and_manual_ticket(db: Path) -> None:
    """Tempo-ID, description, and a manually-assigned ticket must survive re-infer."""
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC))
        _add(conn, datetime(2026, 4, 18, 10, 10, tzinfo=UTC))
    day = datetime(2026, 4, 18).date()
    events = load_day_events(date=day)

    persist_blocks(build_blocks(events), day=day)
    with connect(db) as conn:
        conn.execute(
            """
            UPDATE blocks
               SET tempo_worklog_id = 'TW-99',
                   description = 'manual desc',
                   jira_issue = 'MAN-7'
             WHERE day = ?
            """,
            (day.isoformat(),),
        )

    persist_blocks(build_blocks(events), day=day)
    with connect(db) as conn:
        row = conn.execute(
            "SELECT tempo_worklog_id, description, jira_issue FROM blocks"
        ).fetchone()
    assert row["tempo_worklog_id"] == "TW-99"
    assert row["description"] == "manual desc"
    assert row["jira_issue"] == "MAN-7"


def test_persist_preserves_carry_on_started_at_shift(db: Path) -> None:
    """Regression: a backfilled earlier event shifts a block's
    started_at, so the strict-key carry lookup misses. Without the
    overlap fallback, tempo_worklog_id silently clears — violating the
    CLAUDE.md canary. Python must mirror the Rust fix."""
    day = datetime(2026, 4, 18).date()
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 10, 0, tzinfo=UTC))
        _add(conn, datetime(2026, 4, 18, 10, 5, tzinfo=UTC))
    events = load_day_events(date=day)
    persist_blocks(build_blocks(events), day=day)

    with connect(db) as conn:
        conn.execute(
            """
            UPDATE blocks
               SET tempo_worklog_id = 'TW-42',
                   description = 'reviewed',
                   jira_issue = 'PROJ-3',
                   estimated_by = 'manual'
             WHERE day = ?
            """,
            (day.isoformat(),),
        )

    # Backfill adds an earlier event → started_at of the block shifts
    # backward by 5 minutes.
    with connect(db) as conn:
        _add(conn, datetime(2026, 4, 18, 9, 55, tzinfo=UTC))

    persist_blocks(build_blocks(load_day_events(date=day)), day=day)

    with connect(db) as conn:
        row = conn.execute(
            "SELECT tempo_worklog_id, description, jira_issue, estimated_by "
            "FROM blocks WHERE day = ?",
            (day.isoformat(),),
        ).fetchone()
    assert row["tempo_worklog_id"] == "TW-42"
    assert row["description"] == "reviewed"
    assert row["jira_issue"] == "PROJ-3"
    assert row["estimated_by"] == "manual"


def test_load_handles_explicit_duration(db: Path) -> None:
    """Events with ended_at + duration_seconds (e.g. gcal meetings) are used verbatim."""
    start = datetime(2026, 4, 18, 10, 0, tzinfo=UTC)
    with connect(db) as conn:
        upsert_event(
            conn,
            source="gcal",
            source_id="cal:evt-1",
            started_at=start,
            ended_at=start + timedelta(minutes=30),
            duration_seconds=1800,
            title="meeting",
        )
    events = load_day_events(date=start.date())
    assert events[0].duration_seconds == 1800
