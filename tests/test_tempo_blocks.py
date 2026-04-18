"""Tempo sync must use blocks (with real durations), not raw events."""

from __future__ import annotations

from datetime import date
from pathlib import Path
from unittest.mock import patch

import pytest

from worklog.db import connect, init_db


@pytest.fixture
def db(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    p = tmp_path / "worklog.db"
    monkeypatch.setattr("worklog.db.DB_PATH", p)
    init_db(p)
    with connect(p) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, company, jira_issue, started_at, ended_at,
                                duration_seconds, description, estimated_by)
            VALUES ('2026-04-18', 'Acme', 'ACME-42',
                    '2026-04-18T10:00:00+00:00', '2026-04-18T10:45:00+00:00',
                    2700, 'Refactor auth module', 'claude_p')
            """
        )
    return p


def test_sync_day_dry_run_uses_block_duration(db: Path) -> None:
    from worklog.tempo import sync_day

    results = sync_day(date(2026, 4, 18), dry_run=True)
    assert len(results) == 1
    payload = results[0]["payload"]
    assert payload["issueKey"] == "ACME-42"
    assert payload["timeSpentSeconds"] == 2700  # NOT a 15-min placeholder
    assert payload["description"] == "Refactor auth module"


def test_sync_day_marks_block_after_success(db: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    from worklog.tempo import sync_day

    monkeypatch.setenv("WORKLOG_TEMPO_TOKEN", "test-token")

    class FakeResp:
        status_code = 200

        def json(self):
            return {"tempoWorklogId": "TW-999"}

    class FakeClient:
        def __init__(self, *a, **k):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *a):
            pass

        def post(self, url, json):
            return FakeResp()

    with patch("worklog.tempo.httpx.Client", FakeClient):
        results = sync_day(date(2026, 4, 18), dry_run=False)

    assert results[0]["status"] == "synced"
    with connect(db) as conn:
        tempo_id = conn.execute(
            "SELECT tempo_worklog_id FROM blocks"
        ).fetchone()[0]
    assert tempo_id == "TW-999"


def test_sync_day_skips_already_synced(db: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Re-running sync for a day that was already synced must not double-post."""
    from worklog.tempo import sync_day

    monkeypatch.setenv("WORKLOG_TEMPO_TOKEN", "test-token")
    with connect(db) as conn:
        conn.execute("UPDATE blocks SET tempo_worklog_id = 'TW-1'")

    results = sync_day(date(2026, 4, 18), dry_run=True)
    # Block already synced should not appear in results
    statuses = [r.get("status") for r in results]
    assert "dry-run" not in statuses
