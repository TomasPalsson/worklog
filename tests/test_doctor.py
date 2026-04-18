"""Tests for doctor diagnostic + setup wizard (env file I/O)."""

from __future__ import annotations

from pathlib import Path

import pytest
from typer.testing import CliRunner

from worklog.cli import _read_env_file, _write_env_file, app
from worklog.db import init_db


@pytest.fixture
def isolated(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    db = tmp_path / "worklog.db"
    env = tmp_path / "cfg" / ".env"
    monkeypatch.setattr("worklog.db.DB_PATH", db)
    monkeypatch.setattr("worklog.config.CONFIG_DIR", tmp_path / "cfg")
    monkeypatch.setattr("worklog.config.DB_PATH", db)
    monkeypatch.setattr("worklog.config.ENV_PATH", env)
    monkeypatch.setattr("worklog.cli.DB_PATH", db)
    monkeypatch.setattr("worklog.cli.CONFIG_DIR", tmp_path / "cfg")
    monkeypatch.setattr("worklog.cli.ENV_PATH", env)
    monkeypatch.setattr("pathlib.Path.home", lambda: tmp_path)
    init_db(db)
    return tmp_path


def test_doctor_prints_status(isolated: Path) -> None:
    result = CliRunner().invoke(app, ["doctor"])
    assert result.exit_code == 0
    assert "Database:" in result.stdout
    assert "Jira ticket cache:" in result.stdout
    assert "Credentials:" in result.stdout


def test_env_roundtrip(isolated: Path) -> None:
    vals = {
        "WORKLOG_JIRA_BASE_URL": "https://ex.atlassian.net",
        "WORKLOG_JIRA_TOKEN": "tok-123",
        "WORKLOG_GITHUB_USER": "TomasPalsson",
    }
    _write_env_file(vals)
    loaded = _read_env_file()
    assert loaded == vals


def test_env_file_has_600_perms(isolated: Path) -> None:
    _write_env_file({"WORKLOG_JIRA_TOKEN": "secret"})
    from worklog.config import ENV_PATH

    assert ENV_PATH.exists()
    mode = ENV_PATH.stat().st_mode & 0o777
    assert mode == 0o600, f"expected 600, got {oct(mode)}"


def test_env_quotes_values_with_spaces(isolated: Path) -> None:
    _write_env_file({"WORKLOG_JIRA_BASE_URL": "value with spaces"})
    from worklog.config import ENV_PATH

    text = ENV_PATH.read_text()
    assert '"value with spaces"' in text
    # and it reads back intact
    assert _read_env_file()["WORKLOG_JIRA_BASE_URL"] == "value with spaces"


def test_setup_writes_env_from_stdin(isolated: Path) -> None:
    # Typer's CliRunner simulates stdin for Prompt.ask
    stdin = "\n".join([
        "https://ex.atlassian.net",  # jira url
        "me@ex.com",                  # jira email
        "jira-tok",                   # jira token
        "tempo-tok",                  # tempo token
        "gh-tok",                     # github token
        "TomasPalsson",               # github user
        "n",                          # skip jira refresh
        "n",                          # skip hook install
    ]) + "\n"
    result = CliRunner().invoke(app, ["setup"], input=stdin)
    assert result.exit_code == 0, result.stdout
    loaded = _read_env_file()
    assert loaded["WORKLOG_JIRA_BASE_URL"] == "https://ex.atlassian.net"
    assert loaded["WORKLOG_JIRA_TOKEN"] == "jira-tok"
    assert loaded["WORKLOG_TEMPO_TOKEN"] == "tempo-tok"
    assert loaded["WORKLOG_GITHUB_TOKEN"] == "gh-tok"


def test_setup_reset_ignores_existing(isolated: Path) -> None:
    _write_env_file({"WORKLOG_JIRA_TOKEN": "old"})
    # 6 empty answers + skip refresh + skip hook install
    stdin = "\n".join([""] * 6 + ["n", "n"]) + "\n"
    result = CliRunner().invoke(app, ["setup", "--reset"], input=stdin)
    assert result.exit_code == 0
    # All empty values → nothing written (empty dict)
    assert _read_env_file() == {}
