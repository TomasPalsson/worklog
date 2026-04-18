"""Tests for the doctor diagnostic + hook-install Rust preference."""

from __future__ import annotations

from pathlib import Path

import pytest
from typer.testing import CliRunner

from worklog.cli import _hook_cmd, app
from worklog.db import init_db


@pytest.fixture
def isolated(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    monkeypatch.setattr("worklog.db.DB_PATH", tmp_path / "worklog.db")
    monkeypatch.setattr("worklog.config.CONFIG_DIR", tmp_path / "cfg")
    monkeypatch.setattr("pathlib.Path.home", lambda: tmp_path)
    init_db(tmp_path / "worklog.db")
    return tmp_path


def test_doctor_command_prints_status(isolated: Path) -> None:
    runner = CliRunner()
    result = runner.invoke(app, ["doctor"])
    assert result.exit_code == 0
    assert "Database:" in result.stdout
    assert "Jira ticket cache:" in result.stdout
    assert "Hook binary:" in result.stdout


def test_hook_cmd_prefers_rust_binary(isolated: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """If worklog-hook is on PATH, that's the hook command."""
    fake_bin = isolated / "bin" / "worklog-hook"
    fake_bin.parent.mkdir()
    fake_bin.write_text("#!/bin/sh\nexit 0\n")
    fake_bin.chmod(0o755)
    monkeypatch.setenv("PATH", f"{fake_bin.parent}:/usr/bin:/bin")
    cmd = _hook_cmd()
    assert cmd.endswith("worklog-hook"), f"got: {cmd!r}"
    assert "hook run" not in cmd


def test_hook_cmd_falls_back_to_python(isolated: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("PATH", "/usr/bin:/bin")  # no worklog-hook
    cmd = _hook_cmd()
    assert cmd.endswith("worklog hook run")
