"""FastAPI TestClient tests for the block-based review UI."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from worklog.db import connect, init_db, upsert_jira_ticket


@pytest.fixture
def app_client(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> TestClient:
    db = tmp_path / "worklog.db"
    monkeypatch.setattr("worklog.db.DB_PATH", db)
    init_db(db)
    with connect(db) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds,
                                description, estimated_by)
            VALUES ('2026-04-18', 'ACME-1', '2026-04-18T10:00:00+00:00',
                    '2026-04-18T10:45:00+00:00', 2700, 'Do the thing', 'claude_p')
            """
        )
        upsert_jira_ticket(conn, key="ACME-1", summary="Refactor auth", status="In Progress")
        upsert_jira_ticket(conn, key="ACME-2", summary="Build widget", status="To Do")
    from worklog.web.app import app

    return TestClient(app)


def test_index_shows_block_and_ticket_options(app_client: TestClient) -> None:
    r = app_client.get("/?day=2026-04-18")
    assert r.status_code == 200
    assert "ACME-1" in r.text
    assert "Do the thing" in r.text
    assert "10:00–10:45" in r.text
    # datalist options present
    assert "ACME-2" in r.text
    assert "Refactor auth" in r.text


def test_block_duration_update(app_client: TestClient) -> None:
    app_client.post(
        "/blocks/1/duration",
        data={"minutes": "60", "day": "2026-04-18"},
        follow_redirects=False,
    )
    r = app_client.get("/?day=2026-04-18")
    assert 'value="60"' in r.text


def test_block_description_update(app_client: TestClient) -> None:
    app_client.post(
        "/blocks/1/description",
        data={"description": "Updated description", "day": "2026-04-18"},
        follow_redirects=False,
    )
    r = app_client.get("/?day=2026-04-18")
    assert "Updated description" in r.text


def test_block_ticket_assign(app_client: TestClient) -> None:
    app_client.post(
        "/blocks/1/ticket",
        data={"jira_issue": "ACME-2", "day": "2026-04-18"},
        follow_redirects=False,
    )
    r = app_client.get("/?day=2026-04-18")
    assert 'value="ACME-2"' in r.text


def test_block_ticket_clear(app_client: TestClient) -> None:
    app_client.post(
        "/blocks/1/ticket",
        data={"jira_issue": "", "day": "2026-04-18"},
        follow_redirects=False,
    )
    # empty ticket → block flagged as "No ticket"
    r = app_client.get("/?day=2026-04-18")
    assert "No ticket" in r.text


def test_block_delete(app_client: TestClient) -> None:
    app_client.post(
        "/blocks/1/delete",
        data={"day": "2026-04-18"},
        follow_redirects=False,
    )
    r = app_client.get("/?day=2026-04-18")
    assert "No blocks for 2026-04-18" in r.text


def test_empty_day_shows_helper_text(app_client: TestClient) -> None:
    r = app_client.get("/?day=2026-04-17")
    assert "No blocks for 2026-04-17" in r.text
