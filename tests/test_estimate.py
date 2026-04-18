"""Red-phase tests for the claude -p estimator."""

from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
from unittest.mock import patch

import pytest

from worklog.db import connect, init_db
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
    # seed a block
    with connect(p) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, company, started_at, ended_at, duration_seconds)
            VALUES ('2026-04-18', 'Acme', '2026-04-18T10:00:00+00:00',
                    '2026-04-18T10:45:00+00:00', 2700)
            """
        )
    return p


def test_schema_shape() -> None:
    assert ESTIMATE_SCHEMA["type"] == "object"
    assert set(ESTIMATE_SCHEMA["required"]) == {"minutes", "description"}


def test_system_prompt_nonempty() -> None:
    assert len(SYSTEM_PROMPT) > 100


def test_parse_envelope_result_json() -> None:
    envelope = json.dumps(
        {"result": json.dumps({"minutes": 45, "description": "Fix JWT"})}
    )
    assert parse_response(envelope) == {"minutes": 45, "description": "Fix JWT"}


def test_parse_raw_json_fallback() -> None:
    """Some CLI versions return bare JSON, not enveloped."""
    raw = json.dumps({"minutes": 30, "description": "Review PR"})
    assert parse_response(raw) == {"minutes": 30, "description": "Review PR"}


def test_parse_extracts_from_prose_wrapper() -> None:
    raw = 'Here you go:\n```json\n{"minutes": 15, "description": "x"}\n```\nDone.'
    parsed = parse_response(raw)
    assert parsed == {"minutes": 15, "description": "x"}


def test_parse_raises_on_malformed() -> None:
    with pytest.raises(ValueError):
        parse_response("this has no json at all")


def test_build_user_message_has_block_context(db: Path) -> None:
    with connect(db) as conn:
        block_row = conn.execute("SELECT * FROM blocks").fetchone()
    msg = build_user_message(block_row, events=[])
    parsed = json.loads(msg)
    assert parsed["block_duration_minutes"] == 45
    assert parsed["company"] == "Acme"


def test_estimate_day_calls_claude_once_per_unestimated_block(db: Path) -> None:
    """Blocks already carrying a description via estimated_by='claude_p' are skipped."""
    with connect(db) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, company, started_at, ended_at, duration_seconds,
                                description, estimated_by)
            VALUES ('2026-04-18', 'Side', '2026-04-18T14:00:00+00:00',
                    '2026-04-18T14:30:00+00:00', 1800, 'Already done', 'claude_p')
            """
        )

    calls = []

    def fake_claude(system_prompt, user_message, schema, model):
        calls.append(user_message)
        return {"minutes": 45, "description": "Implement thing (ACME-1)"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        result = estimate_day(datetime(2026, 4, 18).date())

    assert len(calls) == 1  # the first block, not the one with estimated_by
    assert result["estimated"] == 1
    assert result["skipped"] == 1


def test_estimate_day_writes_description_and_minutes(db: Path) -> None:
    def fake_claude(*_args, **_kwargs):
        return {"minutes": 60, "description": "Refactor auth module"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        row = conn.execute(
            "SELECT description, duration_seconds, estimated_by FROM blocks"
        ).fetchone()
    assert row["description"] == "Refactor auth module"
    assert row["duration_seconds"] == 60 * 60  # overwritten from claude's estimate
    assert row["estimated_by"] == "claude_p"


def test_estimate_day_falls_back_on_malformed_response(db: Path) -> None:
    def bad_claude(*_args, **_kwargs):
        raise ValueError("claude returned garbage")

    with patch("worklog.estimate._invoke_claude_p", side_effect=bad_claude):
        result = estimate_day(datetime(2026, 4, 18).date())

    assert result["estimated"] == 0
    assert result["failed"] == 1
    # Block still has its original duration; description stays null; estimated_by='gap'
    with connect(db) as conn:
        row = conn.execute(
            "SELECT description, estimated_by FROM blocks"
        ).fetchone()
    assert row["description"] is None
    assert row["estimated_by"] == "gap"


def test_rounds_minutes_to_nearest_15(db: Path) -> None:
    """Tempo billing hygiene — always round up to 15min multiple."""
    def fake_claude(*_args, **_kwargs):
        return {"minutes": 37, "description": "x"}

    with patch("worklog.estimate._invoke_claude_p", side_effect=fake_claude):
        estimate_day(datetime(2026, 4, 18).date())

    with connect(db) as conn:
        dur = conn.execute("SELECT duration_seconds FROM blocks").fetchone()[0]
    # 37 → round up to 45
    assert dur == 45 * 60
