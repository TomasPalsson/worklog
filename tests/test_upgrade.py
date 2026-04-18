"""Tests for upgrade routing — `_rust_has_signed_updater` and source=auto."""

from __future__ import annotations

import subprocess
from unittest.mock import patch

from worklog.cli import _rust_has_signed_updater


def test_rust_has_signed_updater_returns_false_when_no_rust_binary() -> None:
    with patch("worklog.cli._rust_binary", return_value=None):
        assert _rust_has_signed_updater() is False


def test_rust_has_signed_updater_false_on_placeholder_output() -> None:
    """When the Rust binary prints 'placeholder' in stderr, the pubkey is
    not yet embedded and we must fall back to the git upgrade path."""
    fake_proc = subprocess.CompletedProcess(
        args=[], returncode=1, stdout="", stderr="release public key is the all-zero placeholder"
    )
    with (
        patch("worklog.cli._rust_binary", return_value="/fake/worklog-rs"),
        patch("subprocess.run", return_value=fake_proc),
    ):
        assert _rust_has_signed_updater() is False


def test_rust_has_signed_updater_true_on_network_error_output() -> None:
    """A non-placeholder error (e.g. unreachable manifest URL) means the
    updater is wired up with a real pubkey — signed upgrade is viable."""
    fake_proc = subprocess.CompletedProcess(
        args=[], returncode=1, stdout="", stderr="GET http://127.0.0.1:1/never: connection refused"
    )
    with (
        patch("worklog.cli._rust_binary", return_value="/fake/worklog-rs"),
        patch("subprocess.run", return_value=fake_proc),
    ):
        assert _rust_has_signed_updater() is True


def test_rust_has_signed_updater_false_on_timeout() -> None:
    """Regression for H5: subprocess.TimeoutExpired used to escape as a
    raw traceback when the Rust binary hung 3s. Must now fall back."""

    def raise_timeout(*_args, **_kwargs):
        raise subprocess.TimeoutExpired(cmd="worklog-rs", timeout=3)

    with (
        patch("worklog.cli._rust_binary", return_value="/fake/worklog-rs"),
        patch("subprocess.run", side_effect=raise_timeout),
    ):
        # Must return False, NOT re-raise.
        assert _rust_has_signed_updater() is False


def test_rust_has_signed_updater_false_on_oserror() -> None:
    """An OSError (e.g. binary disappeared between _rust_binary() and
    subprocess.run) must be treated as 'signed path unavailable',
    not crash the CLI."""

    def raise_oserror(*_args, **_kwargs):
        raise OSError(2, "No such file or directory")

    with (
        patch("worklog.cli._rust_binary", return_value="/fake/worklog-rs"),
        patch("subprocess.run", side_effect=raise_oserror),
    ):
        assert _rust_has_signed_updater() is False
