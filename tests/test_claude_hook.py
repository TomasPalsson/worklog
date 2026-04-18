"""Tests for the Claude Code hook's session pairing behavior."""

from __future__ import annotations

import json
import os
from datetime import datetime
from pathlib import Path
from typing import Any

import pytest

from worklog.collectors import claude as claude_hook
from worklog.db import connect, init_db


@pytest.fixture(autouse=True)
def temp_db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    db = tmp_path / "worklog.db"
    # Redirect both the Settings default AND the db module constant.
    monkeypatch.setattr("worklog.db.DB_PATH", db)
    monkeypatch.setattr("worklog.config.DB_PATH", db)
    monkeypatch.setattr(
        "worklog.config.COMPANIES_PATH", tmp_path / "companies.yaml"
    )
    init_db(db)
    os.environ.pop("WORKLOG_GITHUB_TOKEN", None)
    return db


def _payload(event: str, **extra: Any) -> dict[str, Any]:
    return {
        "hook_event_name": event,
        "session_id": "sess-abc",
        "cwd": "/tmp/proj",
        "transcript_path": "/tmp/t.jsonl",
        **extra,
    }


def test_session_start_creates_session_row(temp_db: Path) -> None:
    claude_hook.handle(_payload("SessionStart", source="startup"))
    with connect(temp_db) as conn:
        row = conn.execute(
            "SELECT session_id, ended_at FROM sessions WHERE session_id = 'sess-abc'"
        ).fetchone()
    assert row is not None
    assert row["ended_at"] is None


def test_stop_closes_existing_session(temp_db: Path) -> None:
    claude_hook.handle(_payload("SessionStart", source="startup"))
    claude_hook.handle(_payload("Stop"))
    with connect(temp_db) as conn:
        row = conn.execute(
            "SELECT ended_at, end_source FROM sessions WHERE session_id = 'sess-abc'"
        ).fetchone()
    assert row["ended_at"] is not None
    assert row["end_source"] == "stop"


def test_event_rows_carry_session_id(temp_db: Path) -> None:
    claude_hook.handle(_payload("UserPromptSubmit", prompt="do a thing"))
    with connect(temp_db) as conn:
        rows = conn.execute(
            "SELECT session_id FROM events WHERE source = 'claude'"
        ).fetchall()
    assert len(rows) == 1
    assert rows[0]["session_id"] == "sess-abc"


def test_subagent_stop_does_not_end_parent_session(temp_db: Path) -> None:
    claude_hook.handle(_payload("SessionStart", source="startup"))
    claude_hook.handle(_payload("SubagentStop", agent_id="sub-1", agent_type="Explore"))
    with connect(temp_db) as conn:
        row = conn.execute(
            "SELECT ended_at FROM sessions WHERE session_id = 'sess-abc'"
        ).fetchone()
    # SubagentStop is informational — parent session stays open until Stop/SessionEnd
    assert row["ended_at"] is None


def test_session_end_event_closes_session(temp_db: Path) -> None:
    claude_hook.handle(_payload("SessionStart", source="startup"))
    claude_hook.handle(_payload("SessionEnd", reason="logout"))
    with connect(temp_db) as conn:
        row = conn.execute(
            "SELECT end_source FROM sessions WHERE session_id = 'sess-abc'"
        ).fetchone()
    assert row["end_source"] == "session_end"


def test_malformed_json_does_not_raise(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """main() must return 0 for every malformed input."""
    import io

    monkeypatch.setattr("sys.stdin", io.StringIO("not-json"))
    assert claude_hook.main() == 0


def test_hook_writes_happen_before_any_print(
    temp_db: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Hook must never print to stdout (Claude Code would surface it)."""
    import io
    import sys

    payload = json.dumps(_payload("UserPromptSubmit", prompt="x"))
    sys.stdin = io.StringIO(payload)
    claude_hook.main()
    captured = capsys.readouterr()
    assert captured.out == ""
