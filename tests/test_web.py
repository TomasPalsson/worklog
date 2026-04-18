"""FastAPI TestClient tests for the block-based review UI."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from worklog.db import connect, init_db


@pytest.fixture
def app_client(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> TestClient:
    db = tmp_path / "worklog.db"
    monkeypatch.setattr("worklog.db.DB_PATH", db)
    monkeypatch.setattr(
        "worklog.config.COMPANIES_PATH", tmp_path / "companies.yaml"
    )
    init_db(db)
    with connect(db) as conn:
        conn.execute(
            """
            INSERT INTO blocks (day, company, started_at, ended_at, duration_seconds,
                                description, estimated_by)
            VALUES ('2026-04-18', 'Acme', '2026-04-18T10:00:00+00:00',
                    '2026-04-18T10:45:00+00:00', 2700, 'Do the thing', 'claude_p')
            """
        )
    from worklog.web.app import app

    return TestClient(app)


def test_index_shows_block(app_client: TestClient) -> None:
    r = app_client.get("/?day=2026-04-18")
    assert r.status_code == 200
    assert "Acme" in r.text
    assert "Do the thing" in r.text
    assert "10:00–10:45" in r.text


def test_block_duration_update(app_client: TestClient) -> None:
    # Grab block id
    r = app_client.get("/?day=2026-04-18")
    assert r.status_code == 200
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


def test_empty_day_shows_helper_text(app_client: TestClient) -> None:
    r = app_client.get("/?day=2026-04-17")
    assert "No blocks for 2026-04-17" in r.text
