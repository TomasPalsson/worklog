"""Tests for the claude -p estimator (v3: picks ticket from open tickets)."""

from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
from unittest.mock import patch

import pytest

from worklog.db import connect, init_db, upsert_jira_ticket
from worklog.estimate import (
    ESTIMATE_SCHEMA,
    SYSTEM_PROMPT,
    build_user_message,
    estimate_day,
    parse_response,
)


@pytest.fixture
def db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    p = tmp_path / "worklog.db"
    monkeypatch.setattr("worklog.db.DB_PATH", p)
    init_db(p)
    with connect(p) as conn:
        cur = conn.execute(
            """
            INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
            VALUES ('2026-04-18', '2026-04-18T10:00:00+00:00',
                    '2026-04-18T10:45:00+00:00', 2700)
            """
        )
        block_id = cur.lastrowid
        conn.execute(
            """
            INSERT INTO events (source, source_id, started_at, title, details)
            VALUES ('github_commit', 'sha1', '2026-04-18T10:00:00+00:00',
                    'Add JWT refresh (ACME-42)', 'body mentioning ACME-42')
            """
        )
        evt_id = conn.execute("SELECT id FROM events").fetchone()[0]
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?, ?)",
            (block_id, evt_id),
        )
        upsert_jira_ticket(conn, key="ACME-1", summary="Open ticket one", status="In Progress")
    return p


def test_schema_has_jira_issue() -> None:
    assert set(ESTIMATE_SCHEMA["required"]) == {"jira_issue", "minutes", "description"}


def test_system_prompt_mentions_candidate_tickets() -> None:
    assert len(SYSTEM_PROMPT) > 100
    assert "candidate_tickets" in SYSTEM_PROMPT


def test_parse_envelope_result_json() -> None:
    envelope = json.dumps(
        {"result": json.dumps({"jira_issue": "ACME-1", "minutes": 45, "description": "Fix JWT"})}
    )
    assert parse_response(envelope) == {
        "jira_issue": "ACME-1",
        "minutes": 45,
        "description": "Fix JWT",
    }


def test_parse_raw_json_fallback() -> None:
    raw = json.dumps({"jira_issue": None, "minutes": 30, "description": "Review PR"})
    assert parse_response(raw) == {
        "jira_issue": None,
        "minutes": 30,
        "description": "Review PR",
    }


def test_parse_raises_on_malformed() -> None:
    with pytest.raises(ValueError):
        parse_response("this has no json at all")


def test_build_user_message_includes_candidates_and_literals(db: Path) -> None:
    with connect(db) as conn:
        block_row = conn.execute("SELECT * FROM blocks").fetchone()
        events = conn.execute("SELECT * FROM events").fetchall()
    msg = build_user_message(
        block_row,
        list(events),
        open_tickets=[{"key": "ACME-1", "summary": "s", "status": "In Progress"}],
        literal_matches=["ACME-42"],
    )
    parsed = json.loads(msg)
    assert parsed["candidate_tickets"][0]["key"] == "ACME-1"
    assert parsed["literal_matches"] == ["ACME-42"]
    assert parsed["block_duration_minutes"] == 45


def test_estimate_day_writes_ticket_description_and_minutes(db: Path) -> None:
    def fake_claude(*_args, **_kwargs):
        return {
            "jira_issue": "ACME-1",
            "minutes": 60,
            "description": "Refactor auth module",
        }

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        row = conn.execute(
            "SELECT jira_issue, description, duration_seconds, estimated_by FROM blocks"
        ).fetchone()
    assert row["jira_issue"] == "ACME-1"
    assert row["description"] == "Refactor auth module"
    assert row["duration_seconds"] == 60 * 60
    assert row["estimated_by"] == "claude_p"


def test_estimate_allows_literal_match_not_in_cache(db: Path) -> None:
    """Claude may pick a key that's NOT in the cache iff it appeared literally in events."""
    def fake_claude(*_args, **_kwargs):
        return {
            "jira_issue": "ACME-42",  # literal match; not in cache
            "minutes": 30,
            "description": "Fix JWT refresh",
        }

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        row = conn.execute("SELECT jira_issue FROM blocks").fetchone()
    assert row["jira_issue"] == "ACME-42"


def test_estimate_rejects_invented_ticket(db: Path) -> None:
    """A ticket key that's neither in the cache nor a literal match is discarded."""
    def fake_claude(*_args, **_kwargs):
        return {
            "jira_issue": "HALLUC-1",
            "minutes": 30,
            "description": "x",
        }

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        row = conn.execute("SELECT jira_issue FROM blocks").fetchone()
    # HALLUC-1 was dropped; no prior inference on this block → None.
    assert row["jira_issue"] is None


def test_estimate_falls_back_on_malformed_response(db: Path) -> None:
    def bad_claude(*_args, **_kwargs):
        raise ValueError("claude returned garbage")

    with patch("worklog.estimate._invoke_claude_p", side_effect=bad_claude):
        result = estimate_day(datetime(2026, 4, 18).date())

    assert result["estimated"] == 0
    assert result["failed"] == 1
    with connect(db) as conn:
        row = conn.execute(
            "SELECT description, estimated_by FROM blocks"
        ).fetchone()
    assert row["description"] is None
    assert row["estimated_by"] == "gap"


def test_rounds_minutes_to_nearest_15(db: Path) -> None:
    def fake_claude(*_args, **_kwargs):
        return {"jira_issue": "ACME-1", "minutes": 37, "description": "x"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        dur = conn.execute("SELECT duration_seconds FROM blocks").fetchone()[0]
    assert dur == 45 * 60


def test_estimate_skips_already_estimated(db: Path) -> None:
    """A block with estimated_by='claude_p' is not re-invoked."""
    with connect(db) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, started_at, ended_at, duration_seconds,
                                description, estimated_by, jira_issue)
            VALUES ('2026-04-18', '2026-04-18T14:00:00+00:00',
                    '2026-04-18T14:30:00+00:00', 1800, 'already', 'claude_p', 'X-1')
            """
        )
    calls = []

    def fake_claude(*_args, **_kwargs):
        calls.append(1)
        return {"jira_issue": "ACME-1", "minutes": 45, "description": "Fresh"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        result = estimate_day(datetime(2026, 4, 18).date())
    assert len(calls) == 1
    assert result["skipped"] == 1
    assert result["estimated"] == 1


def test_estimate_skips_manual_blocks(db: Path) -> None:
    """A block the user has hand-edited (estimated_by='manual') must NOT
    be overwritten on re-estimation — CLAUDE.md invariant."""
    with connect(db) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, started_at, ended_at, duration_seconds,
                                description, estimated_by, jira_issue)
            VALUES ('2026-04-18', '2026-04-18T14:00:00+00:00',
                    '2026-04-18T14:30:00+00:00', 1800,
                    'user typed this', 'manual', 'USR-7')
            """
        )

    def fake_claude(*_args, **_kwargs):
        # If this is invoked for the manual block we should clobber it.
        return {"jira_issue": "BOT-1", "minutes": 90, "description": "AI-rewrote"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        result = estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        manual_row = conn.execute(
            "SELECT description, estimated_by, jira_issue, duration_seconds "
            "FROM blocks WHERE jira_issue = 'USR-7'"
        ).fetchone()
    assert manual_row["description"] == "user typed this"
    assert manual_row["estimated_by"] == "manual"
    assert manual_row["jira_issue"] == "USR-7"
    assert manual_row["duration_seconds"] == 1800
    assert result["skipped"] >= 1
