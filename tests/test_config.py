"""$WORKLOG_HOME path resolution parity with the Rust side.

Rust's `paths::resolve` collapses the env override into a single
directory (config_dir == data_dir == root). Python must mirror that,
otherwise a user who sets `WORKLOG_HOME=/tmp/worklog-dev` gets
different DB paths on each side and their events silently split.
"""

from __future__ import annotations

import importlib
import os
from pathlib import Path


def _reload(monkeypatch, override: str | None) -> object:
    """Reload worklog.config with the desired $WORKLOG_HOME state."""
    if override is None:
        monkeypatch.delenv("WORKLOG_HOME", raising=False)
    else:
        monkeypatch.setenv("WORKLOG_HOME", override)
    import worklog.config

    return importlib.reload(worklog.config)


def test_default_paths_use_xdg_layout(monkeypatch) -> None:
    cfg = _reload(monkeypatch, None)
    assert cfg.CONFIG_DIR == Path.home() / ".config" / "worklog"
    assert cfg.DATA_DIR == Path.home() / ".local" / "share" / "worklog"
    assert cfg.DB_PATH == cfg.DATA_DIR / "worklog.db"


def test_worklog_home_collapses_to_single_root(tmp_path: Path, monkeypatch) -> None:
    """Regression for QA round 3: Python used to ignore $WORKLOG_HOME,
    so Rust would use /tmp/foo/worklog.db while Python used
    ~/.local/share/worklog/worklog.db. Mixing the two wrote events to
    separate DBs and lost data silently."""
    root = tmp_path / "worklog-home"
    cfg = _reload(monkeypatch, str(root))
    assert cfg.CONFIG_DIR == root
    assert cfg.DATA_DIR == root
    # And matches Rust's collapse: everything under the one root.
    assert cfg.DB_PATH == root / "worklog.db"
    assert cfg.ENV_PATH == root / ".env"


def test_worklog_home_with_tilde_expands(monkeypatch) -> None:
    home = os.environ["HOME"]
    cfg = _reload(monkeypatch, "~/my-worklog")
    assert str(cfg.DATA_DIR) == f"{home}/my-worklog"


def test_empty_worklog_home_falls_back_to_xdg(monkeypatch) -> None:
    # An empty string should behave like "not set" rather than
    # collapsing to $PWD (which would be a surprise).
    cfg = _reload(monkeypatch, "")
    assert cfg.DATA_DIR == Path.home() / ".local" / "share" / "worklog"
