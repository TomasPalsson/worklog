from __future__ import annotations

import json
import shutil
import sys
from datetime import date, datetime, timedelta
from pathlib import Path

import typer
from rich.console import Console
from rich.prompt import Confirm, Prompt
from rich.table import Table

from worklog.collectors import claude as claude_collector
from worklog.collectors import gcal as gcal_collector
from worklog.collectors import github as github_collector
from worklog.collectors import jira_ as jira_collector
from worklog.config import CONFIG_DIR, DB_PATH, ENV_PATH, ensure_dirs
from worklog.db import connect, init_db
from worklog.estimate import DEFAULT_MODEL, estimate_day
from worklog.infer import build_blocks
from worklog.infer_persist import load_day_events, persist_blocks
from worklog.tempo import sync_day

app = typer.Typer(help="Unified work-time tracker → Tempo.")
console = Console()


# ---------- Rust binary delegation ----------
#
# Commands below marked with `context_settings={"allow_extra_args": True,
# "ignore_unknown_options": True}` forward all remaining argv to the Rust
# `worklog` binary. We look for it first at ~/.worklog/bin/worklog-rs, then
# on $PATH. If missing, we print a one-line install hint. Stage 2+ of the
# rewrite will make this seamless — for now it's an explicit delegate so
# users can opt into the new commands without Python changes.


def _rust_binary() -> Path | None:
    """Locate the Rust `worklog` binary. Returns None if not installed yet."""
    from os import environ

    candidate = Path(environ.get("HOME", "")) / ".worklog" / "bin" / "worklog-rs"
    if candidate.is_file():
        return candidate
    which = shutil.which("worklog-rs")
    return Path(which) if which else None


def _exec_rust(args: list[str]) -> None:
    """Replace this process with the Rust binary, forwarding argv."""
    import os

    binary = _rust_binary()
    if binary is None:
        console.print(
            "[yellow]![/] Rust binary not installed yet. Run "
            "[bold]worklog upgrade[/] to build & install it."
        )
        raise typer.Exit(code=127)
    os.execvp(str(binary), [str(binary), *args])


def _parse_day(s: str | None) -> date:
    if not s:
        return date.today()
    return datetime.strptime(s, "%Y-%m-%d").date()


@app.command()
def init() -> None:
    """Create config dirs and DB schema. (Usually you want `worklog setup` instead.)"""
    ensure_dirs()
    init_db()
    console.print(f"[green]✓[/] config dir: {CONFIG_DIR}")
    console.print(f"[green]✓[/] database:   {DB_PATH}")
    console.print("\n[cyan]Next:[/] run [bold]worklog setup[/] to configure credentials.")


# ---------- Claude Code hook ----------

_HOOK_EVENTS = ("SessionStart", "UserPromptSubmit", "Stop", "SubagentStop", "SessionEnd")


def _hook_cmd() -> str:
    """Prefer the native Rust binary if installed, fall back to Python."""
    if rust_bin := shutil.which("worklog-hook"):
        return rust_bin
    # Stage 3 added `worklog hook-run` in Rust; point new installs at it.
    rust_worklog = _rust_binary()
    if rust_worklog is not None:
        return f"{rust_worklog} hook-run"
    worklog_bin = shutil.which("worklog") or "worklog"
    return f"{worklog_bin} hook run"


def _matches_our_hook(handler: dict, cmd: str) -> bool:
    for h in handler.get("hooks", []):
        if "worklog" in (h.get("command") or "") or h.get("command") == cmd:
            return True
    return False


def _install_hook() -> str:
    """Register the stdin-JSON hook in ~/.claude/settings.json. Idempotent."""
    settings_path = Path.home() / ".claude" / "settings.json"
    settings_path.parent.mkdir(parents=True, exist_ok=True)
    settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
    hooks = settings.setdefault("hooks", {})
    cmd = _hook_cmd()
    for event in _HOOK_EVENTS:
        handlers = hooks.setdefault(event, [])
        if any(_matches_our_hook(h, cmd) for h in handlers):
            continue
        handlers.append({"hooks": [{"type": "command", "command": cmd}]})
    settings_path.write_text(json.dumps(settings, indent=2))
    return cmd


def _uninstall_hook() -> None:
    settings_path = Path.home() / ".claude" / "settings.json"
    if not settings_path.exists():
        return
    settings = json.loads(settings_path.read_text())
    hooks = settings.get("hooks", {})
    for event, handlers in list(hooks.items()):
        hooks[event] = [h for h in handlers if not _matches_our_hook(h, "")]
        if not hooks[event]:
            del hooks[event]
    settings_path.write_text(json.dumps(settings, indent=2))


@app.command()
def hook(action: str = typer.Argument(..., help="install | uninstall | status | run")) -> None:
    """Manage the Claude Code hook integration.

    install   — register stdin-JSON hook in ~/.claude/settings.json (Rust)
    uninstall — remove it (Rust)
    status    — report whether worklog is registered (Rust)
    run       — read a hook event from stdin and log it (Python — called by Claude)
    """
    # `run` stays Python: it's the event pipeline Claude invokes on every
    # SessionStart/Stop, still backed by the existing collector. Install and
    # uninstall delegate to the Rust binary so the settings.json editor has
    # one canonical implementation.
    if action == "run":
        sys.exit(claude_collector.main())
    if action not in {"install", "uninstall", "status"}:
        raise typer.BadParameter("action must be install, uninstall, status, or run")

    # Prefer the Rust binary — it's the canonical editor for
    # ~/.claude/settings.json now. Only fall back to Python when Rust is
    # genuinely missing (transitional / fresh installs that haven't run
    # `worklog upgrade` yet).
    import os

    rust_bin = _rust_binary()
    if rust_bin is not None:
        os.execvp(str(rust_bin), [str(rust_bin), "hook", action])
        # os.execvp does not return on success.

    console.print(
        "[yellow]![/] Rust binary missing — falling back to the Python installer. "
        "Run [bold]worklog upgrade[/] when convenient."
    )
    if action == "install":
        cmd = _install_hook()
        console.print("[green]✓[/] hook installed (Session/Prompt/Stop events)")
        console.print(f"  command: {cmd}")
    elif action == "uninstall":
        _uninstall_hook()
        console.print("[green]✓[/] hook removed")
    else:  # status
        path = Path.home() / ".claude" / "settings.json"
        if not path.exists():
            console.print(f"settings: {path}")
            console.print("installed: no")
            return
        settings = json.loads(path.read_text())
        hooks = settings.get("hooks", {})
        events = [e for e, hs in hooks.items() if any(_matches_our_hook(h, "") for h in hs)]
        console.print(f"settings: {path}")
        console.print(f"installed: {'yes' if events else 'no'}")
        if events:
            console.print(f"events:    {', '.join(events)}")


# ---------- setup wizard ----------

_ENV_KEYS: list[tuple[str, str, str, bool]] = [
    ("WORKLOG_JIRA_BASE_URL", "Jira base URL",
     "e.g. https://yourco.atlassian.net", False),
    ("WORKLOG_JIRA_EMAIL", "Jira email",
     "the address you use to log in", False),
    ("WORKLOG_JIRA_TOKEN", "Jira API token",
     "id.atlassian.com/manage-profile/security/api-tokens", True),
    ("WORKLOG_TEMPO_TOKEN", "Tempo API token",
     "Jira → apps → Tempo → Settings → API Integration", True),
    ("WORKLOG_GITHUB_TOKEN", "GitHub token",
     "github.com/settings/tokens — needs `repo` scope", True),
    ("WORKLOG_GITHUB_USER", "GitHub username",
     "your handle, e.g. TomasPalsson", False),
]


def _read_env_file() -> dict[str, str]:
    if not ENV_PATH.exists():
        return {}
    out: dict[str, str] = {}
    for line in ENV_PATH.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        out[k.strip()] = v.strip().strip('"').strip("'")
    return out


def _write_env_file(values: dict[str, str]) -> None:
    ENV_PATH.parent.mkdir(parents=True, exist_ok=True)
    lines = ["# worklog credentials — edit by hand or re-run `worklog setup`"]
    for k, v in values.items():
        if v == "":
            continue
        # Quote values that contain spaces or special chars for safety.
        if any(c in v for c in " #\"'"):
            v_out = '"' + v.replace('"', '\\"') + '"'
        else:
            v_out = v
        lines.append(f"{k}={v_out}")
    ENV_PATH.write_text("\n".join(lines) + "\n")
    ENV_PATH.chmod(0o600)  # tokens are secrets


_TOKEN_PREFIXES: dict[str, tuple[str, ...]] = {
    "WORKLOG_JIRA_TOKEN": ("ATATT",),
    "WORKLOG_GITHUB_TOKEN": ("github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"),
}


def _scrub_token(key: str, value: str) -> str:
    """Strip stray prefix glyphs pasted from clipboard icons (e.g. Atlassian's
    Copy button, which sometimes prepends a non-printable to the token).
    """
    for prefix in _TOKEN_PREFIXES.get(key, ()):
        if prefix in value and not value.startswith(prefix):
            return value[value.index(prefix):]
    return value


def _mask(v: str) -> str:
    if not v:
        return ""
    if len(v) <= 8:
        return "•" * len(v)
    return v[:4] + "•" * (len(v) - 8) + v[-4:]


@app.command()
def setup(
    reset: bool = typer.Option(False, "--reset", help="Ignore existing values"),
) -> None:
    """Interactive wizard: enter Jira/Tempo/GitHub credentials once."""
    ensure_dirs()
    init_db()

    existing = {} if reset else _read_env_file()

    console.print("[bold]worklog setup[/]")
    console.print(f"Writing credentials to [dim]{ENV_PATH}[/]\n")

    values: dict[str, str] = {}
    for key, label, hint, secret in _ENV_KEYS:
        current = existing.get(key, "")
        shown_current = _mask(current) if secret and current else current
        if current:
            prompt_msg = f"{label} [dim]({shown_current})[/]"
        else:
            prompt_msg = f"{label}\n  [dim]hint: {hint}[/]\n  value"
        value = Prompt.ask(
            prompt_msg,
            default=current,
            password=secret and not current,
            show_default=not secret,
        )
        cleaned = _scrub_token(key, (value or "").strip())
        values[key] = cleaned

    _write_env_file(values)
    console.print(f"\n[green]✓[/] saved to {ENV_PATH}")

    console.print("\n[bold]Google Calendar (optional)[/] — skipped by setup.")
    console.print(
        "  To enable: download OAuth client credentials from "
        "console.cloud.google.com → APIs & Services → Credentials → "
        f"save JSON at [dim]{CONFIG_DIR / 'google_credentials.json'}[/]"
    )

    if values.get("WORKLOG_JIRA_TOKEN") and Confirm.ask(
        "\nRefresh your open Jira tickets now?", default=True
    ):
        try:
            n = jira_collector.fetch_open_tickets()
            console.print(f"[green]✓[/] cached {n} open tickets")
        except Exception as e:  # noqa: BLE001 - diagnostic only
            console.print(f"[red]✗[/] jira refresh failed: {e}")

    console.print(
        "\n[bold]Claude Code hook[/] — logs your coding sessions automatically "
        "so they show up as blocks."
    )
    if Confirm.ask("Install the Claude Code hook?", default=True):
        cmd = _install_hook()
        console.print(f"[green]✓[/] hook installed → [dim]{cmd}[/]")
        console.print(
            "  [dim]takes effect in your next Claude Code session "
            "(current sessions won't re-read settings.json)[/]"
        )

    console.print("\n[bold]You're done.[/] Typical daily flow:")
    console.print(
        "  [dim]worklog collect all && worklog infer && "
        "worklog estimate && worklog serve[/]"
    )


# ---------- collectors ----------


@app.command()
def collect(
    source: str = typer.Argument("all", help="all | github | gcal | jira"),
    since: str = typer.Option(None, help="YYYY-MM-DD (default: 7 days ago)"),
    until: str = typer.Option(None, help="YYYY-MM-DD exclusive (default: tomorrow)"),
) -> None:
    """Pull activity from external sources into the local event store.

    jira/github/all delegate to the Rust binary (Stage 2). gcal stays in
    Python until Stage 2.1 adds Google OAuth support to the Rust side.
    """
    since_d = _parse_day(since) if since else date.today() - timedelta(days=7)
    until_d = _parse_day(until) if until else None

    # gcal is Python-only for now; strip it out of the Rust-delegated slice.
    wants_gcal = source in {"all", "gcal"}
    wants_rust = source in {"all", "jira", "github"}

    if wants_rust and _rust_binary() is not None:
        import os
        import subprocess

        rust_args: list[str] = ["collect"]
        if source == "gcal":
            # Skip rust entirely for gcal-only requests.
            pass
        else:
            rust_args.append("jira" if source == "jira" else "github" if source == "github" else "all")
            days = (date.today() + timedelta(days=1) - since_d).days
            rust_args.extend(["--days", str(max(days, 1))])
            # Run Rust; when source is 'all' we still want to run the
            # Python gcal collector after, so we spawn+wait rather than
            # exec into Rust.
            res = subprocess.run(  # noqa: S603
                [str(_rust_binary()), *rust_args], check=False
            )
            if res.returncode != 0:
                console.print(f"[red]✗[/] rust collect exited {res.returncode}")
            if source in {"jira", "github"}:
                return  # Done — no Python fallback needed.
            _ = os  # silence unused import when gcal path is skipped

    # Python path — either the Rust binary is missing, or the user asked
    # for gcal specifically, or `all` (continuing from Rust above).
    sources: list[str] = []
    if source == "all":
        sources = ["gcal"]  # jira+github already handled by Rust above
    elif source == "gcal":
        sources = ["gcal"]
    elif _rust_binary() is None:
        # Full Python fallback when Rust isn't installed.
        sources = ["github", "gcal", "jira"] if source == "all" else [source]

    for s in sources:
        try:
            if s == "github":
                n = github_collector.collect(since=since_d, until=until_d)
            elif s == "gcal":
                n = gcal_collector.collect(since=since_d, until=until_d)
            elif s == "jira":
                n = jira_collector.collect(since=since_d, until=until_d)
            else:
                raise typer.BadParameter(f"unknown source: {s}")
            console.print(f"[green]✓[/] {s}: {n} events")
        except Exception as e:  # noqa: BLE001
            console.print(f"[red]✗[/] {s}: {e}")


@app.command()
def today(day: str = typer.Option(None, help="YYYY-MM-DD, default today")) -> None:
    """Show a tabular summary of one day's events."""
    d = _parse_day(day)
    start = datetime.combine(d, datetime.min.time()).isoformat()
    end = datetime.combine(d + timedelta(days=1), datetime.min.time()).isoformat()
    with connect() as conn:
        rows = conn.execute(
            "SELECT * FROM events WHERE started_at >= ? AND started_at < ? ORDER BY started_at",
            (start, end),
        ).fetchall()

    table = Table(title=f"worklog — {d}")
    for col in ("time", "src", "title", "issue", "min"):
        table.add_column(col)
    for r in rows:
        table.add_row(
            r["started_at"][11:16],
            r["source"],
            (r["title"] or "")[:60],
            r["jira_issue"] or "—",
            str((r["duration_seconds"] or 0) // 60),
        )
    console.print(table)


@app.command()
def infer(day: str = typer.Option(None, help="YYYY-MM-DD, default today")) -> None:
    """Cluster the day's events into blocks (gap-timeout algorithm).

    Delegates to the Rust binary when available; Python fallback otherwise.
    """
    rust_bin = _rust_binary()
    if rust_bin is not None:
        import os

        d = _parse_day(day)
        os.execvp(str(rust_bin), [str(rust_bin), "infer", "--day", d.isoformat()])

    d = _parse_day(day)
    events = load_day_events(date=d)
    blocks = build_blocks(events)
    persist_blocks(blocks, day=d)
    total_min = sum(b.duration_seconds for b in blocks) // 60
    console.print(f"[green]✓[/] {d}: {len(blocks)} blocks, {total_min} min total")
    for b in blocks:
        flag = " [yellow](flagged)[/]" if b.flagged else ""
        issue = f" {b.jira_issue}" if b.jira_issue else ""
        console.print(
            f"  {b.started_at.strftime('%H:%M')}–{b.ended_at.strftime('%H:%M')} "
            f"({b.duration_seconds // 60}min, {b.event_count} events){issue}{flag}"
        )


@app.command()
def estimate(
    day: str = typer.Option(None, help="YYYY-MM-DD, default today"),
    model: str = typer.Option(DEFAULT_MODEL, help="claude model id (default: haiku 4.5)"),
) -> None:
    """Ask `claude -p` to pick a Jira ticket + write a description for each block.

    Delegates to the Rust binary when available; Python fallback otherwise.
    """
    rust_bin = _rust_binary()
    if rust_bin is not None:
        import os

        d = _parse_day(day)
        os.execvp(
            str(rust_bin),
            [str(rust_bin), "estimate", "--day", d.isoformat(), "--model", model],
        )

    d = _parse_day(day)
    stats = estimate_day(d, model=model)
    console.print(
        f"[green]✓[/] {d}: estimated={stats['estimated']}, "
        f"skipped={stats['skipped']}, failed={stats['failed']}"
    )


@app.command()
def sync(
    day: str = typer.Option(None, help="YYYY-MM-DD (default today)"),
    dry_run: bool = typer.Option(True, help="Show payloads without POSTing"),
) -> None:
    """Push reviewed blocks to Tempo (one worklog per block).

    Delegates to the Rust binary when available; falls back to the
    Python implementation so the command works even before
    `worklog upgrade` has installed the Rust binary.
    """
    rust_bin = _rust_binary()
    if rust_bin is not None:
        import os

        d = _parse_day(day)
        args = [str(rust_bin), "sync", "--day", d.isoformat()]
        if dry_run:
            args.append("--dry-run")
        os.execvp(str(rust_bin), args)

    # Python fallback (no-Rust-binary path).
    d = _parse_day(day)
    results = sync_day(d, dry_run=dry_run)
    for r in results:
        console.print(r)


@app.command()
def doctor() -> None:
    """Diagnose worklog setup: paths, credentials, ticket cache."""
    import shutil

    def ok(label: str, detail: str) -> None:
        console.print(f"[green]✓[/] {label:28} {detail}")

    def warn(label: str, detail: str) -> None:
        console.print(f"[yellow]![/] {label:28} {detail}")

    def fail(label: str, detail: str) -> None:
        console.print(f"[red]✗[/] {label:28} {detail}")

    if DB_PATH.exists():
        init_db()
        with connect() as conn:
            n_events = conn.execute("SELECT COUNT(*) FROM events").fetchone()[0]
            n_blocks = conn.execute("SELECT COUNT(*) FROM blocks").fetchone()[0]
            cache = conn.execute(
                "SELECT COUNT(*) AS n, MAX(fetched_at) AS last FROM jira_tickets"
            ).fetchone()
        ok("Database:", f"{DB_PATH} ({n_events} events, {n_blocks} blocks)")
        if cache["n"]:
            ok("Jira ticket cache:", f"{cache['n']} tickets (fetched {cache['last']})")
        else:
            warn(
                "Jira ticket cache:",
                "empty — run `worklog setup` or `worklog collect jira`",
            )
    else:
        warn("Database:", f"{DB_PATH} (does not exist — run `worklog setup`)")

    if ENV_PATH.exists():
        env = _read_env_file()
        missing = [
            k for k, *_ in _ENV_KEYS
            if k.endswith("_TOKEN") or k.endswith("_URL") or k.endswith("_EMAIL")
            if not env.get(k)
        ]
        if missing:
            warn("Credentials:", f"{ENV_PATH} — missing {', '.join(missing)}")
        else:
            ok("Credentials:", f"{ENV_PATH}")
    else:
        warn("Credentials:", f"{ENV_PATH} (missing — run `worklog setup`)")

    if claude_bin := shutil.which("claude"):
        ok("claude CLI:", claude_bin)
    else:
        fail("claude CLI:", "not on PATH — `worklog estimate` will fail")


@app.command()
def serve(
    port: int = typer.Option(3333, help="port to bind on localhost"),
) -> None:
    """Start the review web UI.

    Stage 4: the FastAPI app has been retired in favour of a dockerised
    Next.js + Bun stack. This command just delegates to the Rust
    `worklog web up` which orchestrates the container + daemon.
    """
    _exec_rust(["web", "up", "--port", str(port)])


@app.command()
def day(
    day: str = typer.Option(None, help="YYYY-MM-DD, default today"),
    no_serve: bool = typer.Option(False, "--no-serve", help="Skip opening the review UI"),
    model: str = typer.Option(DEFAULT_MODEL, help="claude model id"),
) -> None:
    """One-shot: collect → infer → estimate → serve. The daily command."""
    d = _parse_day(day)

    console.print("[bold]collecting[/] github + jira …")
    for s, fn in (
        ("github", lambda: github_collector.collect(since=d, until=None)),
        ("jira", lambda: jira_collector.collect(since=d, until=None)),
    ):
        try:
            n = fn()
            console.print(f"  [green]✓[/] {s}: {n} events")
        except Exception as e:  # noqa: BLE001
            console.print(f"  [yellow]![/] {s}: {e}")

    console.print("\n[bold]inferring blocks[/] …")
    events = load_day_events(date=d)
    blocks = build_blocks(events)
    persist_blocks(blocks, day=d)
    total_min = sum(b.duration_seconds for b in blocks) // 60
    console.print(f"  [green]✓[/] {len(blocks)} blocks, {total_min} min total")

    console.print("\n[bold]asking claude to pick tickets + write descriptions[/] …")
    try:
        stats = estimate_day(d, model=model)
        console.print(
            f"  [green]✓[/] estimated={stats['estimated']}, "
            f"skipped={stats['skipped']}, failed={stats['failed']}"
        )
    except Exception as e:  # noqa: BLE001
        console.print(f"  [yellow]![/] estimate skipped: {e}")

    if no_serve:
        return
    console.print("\n[bold]bringing up review UI[/] at http://localhost:3333")
    console.print("  [dim]ctrl+c to bring it down, or `worklog web down`[/]\n")
    _exec_rust(["web", "up"])


@app.command()
def upgrade(
    ref: str = typer.Option("main", help="git branch/tag/SHA (git upgrade only)"),
    source: str = typer.Option(
        "auto",
        help="'signed' = signed binary via `worklog self-update` "
        "(Stage 5, recommended). 'git' = legacy uv-based reinstall. "
        "'auto' = signed if the release pubkey is embedded, else git.",
    ),
) -> None:
    """Upgrade worklog.

    Prefers the signed self-updater (Stage 5) when the release public
    key is baked in; otherwise falls back to the legacy git-based
    ``uv tool install`` flow.
    """
    import subprocess

    if source == "signed" or (source == "auto" and _rust_has_signed_updater()):
        rust = _rust_binary()
        if rust is None:
            console.print(
                "[red]✗[/] signed upgrade needs the Rust binary — "
                "run [bold]worklog upgrade --source git[/] first."
            )
            raise typer.Exit(code=1)
        console.print("[bold]upgrading via signed self-update[/] …")
        result = subprocess.run([str(rust), "self-update"], check=False)  # noqa: S603 - trusted args
        raise typer.Exit(code=result.returncode)

    # Legacy path: uv pulls a fresh git checkout + rebuilds the Rust binary.
    repo = "git+ssh://git@github.com/TomasPalsson/worklog.git"
    if ref and ref != "main":
        repo = f"{repo}@{ref}"
    uv_bin = shutil.which("uv") or "uv"
    console.print(f"[bold]upgrading worklog (git)[/] from {repo} …")
    result = subprocess.run(  # noqa: S603 - trusted args
        [uv_bin, "tool", "install", "--force", "--reinstall", repo],
        check=False,
    )
    if result.returncode != 0:
        console.print("[red]✗[/] upgrade failed — check the uv output above")
        raise typer.Exit(code=result.returncode)
    console.print("[green]✓[/] python upgraded")
    _install_rust_binary()
    console.print("[green]✓[/] all set. run [bold]worklog doctor[/] to confirm.")


def _rust_has_signed_updater() -> bool:
    """True iff the Rust binary is installed AND has a real release pubkey
    embedded (not the all-zero placeholder). We detect it by running
    ``self-update --check`` with a bogus URL — if the pubkey is a
    placeholder we get an explicit error, otherwise we get a network
    error (which means the updater is wired up).
    """
    import subprocess

    rust = _rust_binary()
    if rust is None:
        return False
    # `self-update --check` without a real URL probes the pubkey state
    # cheaply: placeholder → fails with a specific message *before* any
    # network call.
    proc = subprocess.run(  # noqa: S603 - trusted args
        [str(rust), "self-update", "--check", "--manifest-url", "http://127.0.0.1:1/never"],
        capture_output=True,
        text=True,
        timeout=3,
    )
    combined = (proc.stdout or "") + (proc.stderr or "")
    return "placeholder" not in combined


def _install_rust_binary() -> None:
    """Build and install the Rust binary from the installed package's rust/ dir."""
    import subprocess

    # Locate the rust/ workspace. When installed via `uv tool install git+…`
    # we won't have one — fall back to cloning into a temp dir.
    pkg_root = Path(__file__).resolve().parent.parent.parent
    rust_dir = pkg_root / "rust"

    cargo = shutil.which("cargo")
    if cargo is None:
        console.print(
            "[yellow]![/] cargo not found — skipping Rust binary install. "
            "Install Rust from rustup.rs and re-run [bold]worklog upgrade[/]."
        )
        return

    if not rust_dir.is_dir():
        # Installed via `uv tool install` which strips the rust/ tree. Clone
        # it into a scratch dir so we can still build.
        import tempfile

        tmp = Path(tempfile.mkdtemp(prefix="worklog-rust-"))
        rust_dir = tmp / "worklog" / "rust"
        console.print(f"  [dim]cloning rust/ into {tmp} …[/]")
        clone = subprocess.run(  # noqa: S603 - trusted args
            ["git", "clone", "--depth", "1", "git@github.com:TomasPalsson/worklog.git", str(tmp / "worklog")],
            check=False,
            capture_output=True,
            text=True,
        )
        if clone.returncode != 0:
            console.print(f"[yellow]![/] git clone failed: {clone.stderr.strip()}")
            return

    bin_dir = Path.home() / ".worklog" / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    dest = bin_dir / "worklog-rs"

    console.print("[bold]building Rust binary[/] (this takes ~30s the first time) …")
    build = subprocess.run(  # noqa: S603
        [cargo, "build", "--release", "--bin", "worklog", "--manifest-path", str(rust_dir / "Cargo.toml")],
        check=False,
    )
    if build.returncode != 0:
        console.print("[yellow]![/] Rust build failed — see cargo output above.")
        return

    src = rust_dir / "target" / "release" / "worklog"
    if not src.is_file():
        console.print(f"[yellow]![/] Rust binary missing at {src}")
        return

    shutil.copy2(src, dest)
    dest.chmod(0o755)
    console.print(f"[green]✓[/] rust binary installed at {dest}")


# ---------- Rust passthrough commands ----------
#
# These shell out to the Rust binary. They're deliberately defined at the
# end of the module so the delegation helper above is available, and so
# `worklog --help` lists them alongside the Python commands.

_PASSTHROUGH_CONTEXT = {"allow_extra_args": True, "ignore_unknown_options": True}


@app.command("db", context_settings=_PASSTHROUGH_CONTEXT)
def db_passthrough(ctx: typer.Context) -> None:
    """Database operations — delegates to the Rust binary.

    Try: `worklog db migrate`, `worklog db info`, `worklog db path`.
    """
    _exec_rust(["db", *ctx.args])


@app.command("secret", context_settings=_PASSTHROUGH_CONTEXT)
def secret_passthrough(ctx: typer.Context) -> None:
    """OS-keychain secrets — delegates to the Rust binary.

    Try: `worklog secret list`, `worklog secret set jira_api_token`.
    """
    _exec_rust(["secret", *ctx.args])


@app.command("schedule", context_settings=_PASSTHROUGH_CONTEXT)
def schedule_passthrough(ctx: typer.Context) -> None:
    """Scheduled collection — delegates to the Rust binary.

    Try: `worklog schedule install --interval 15m`, `worklog schedule status`.
    """
    _exec_rust(["schedule", *ctx.args])


@app.command("version")
def version_passthrough() -> None:
    """Show Python + Rust binary versions."""
    import importlib.metadata
    import subprocess

    try:
        py_ver = importlib.metadata.version("worklog")
    except importlib.metadata.PackageNotFoundError:
        py_ver = "unknown"
    console.print(f"worklog (python)  {py_ver}")

    rust = _rust_binary()
    if rust is None:
        console.print("worklog (rust)    [yellow]not installed[/] — run `worklog upgrade`")
        return
    result = subprocess.run([str(rust), "version"], capture_output=True, text=True, check=False)  # noqa: S603
    console.print(f"worklog (rust)    {result.stdout.strip() or result.stderr.strip()}")
