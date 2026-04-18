from __future__ import annotations

import json
import shutil
import sys
from datetime import date, datetime, timedelta
from pathlib import Path

import typer
import uvicorn
from rich.console import Console
from rich.table import Table

from worklog.collectors import claude as claude_collector
from worklog.collectors import gcal as gcal_collector
from worklog.collectors import github as github_collector
from worklog.collectors import jira_ as jira_collector
from worklog.config import COMPANIES_PATH, CONFIG_DIR, DB_PATH, ensure_dirs
from worklog.db import connect, init_db
from worklog.estimate import DEFAULT_MODEL, estimate_day
from worklog.infer import build_blocks
from worklog.infer_persist import load_day_events, persist_blocks
from worklog.tempo import sync_day

app = typer.Typer(help="Unified work-time tracker → Tempo.")
console = Console()


def _parse_day(s: str | None) -> date:
    if not s:
        return date.today()
    return datetime.strptime(s, "%Y-%m-%d").date()


@app.command()
def init() -> None:
    """Create config dirs, DB schema, and a companies.yaml stub."""
    ensure_dirs()
    init_db()
    if not COMPANIES_PATH.exists():
        COMPANIES_PATH.write_text(_COMPANIES_STUB)
    console.print(f"[green]✓[/] config dir: {CONFIG_DIR}")
    console.print(f"[green]✓[/] database:   {DB_PATH}")
    console.print(f"[green]✓[/] companies:  {COMPANIES_PATH}")
    console.print("\nNext:")
    console.print(f"  1. edit {COMPANIES_PATH}")
    console.print("  2. set tokens in ~/.config/worklog/.env (or export WORKLOG_*)")
    console.print("  3. worklog hook install")


@app.command()
def hook(action: str = typer.Argument(..., help="install | uninstall | run")) -> None:
    """Manage the Claude Code hook integration.

    install   — register stdin-JSON hook in ~/.claude/settings.json
    uninstall — remove it
    run       — read a hook event from stdin and log it (used by the hook itself)
    """
    if action == "run":
        sys.exit(claude_collector.main())
    settings_path = Path.home() / ".claude" / "settings.json"
    settings_path.parent.mkdir(parents=True, exist_ok=True)
    settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
    hooks = settings.setdefault("hooks", {})
    cmd = _hook_cmd()

    if action == "install":
        for event in ("SessionStart", "UserPromptSubmit", "Stop", "SubagentStop"):
            handlers = hooks.setdefault(event, [])
            if any(_matches_our_hook(h, cmd) for h in handlers):
                continue
            handlers.append({"hooks": [{"type": "command", "command": cmd}]})
        settings_path.write_text(json.dumps(settings, indent=2))
        console.print("[green]✓[/] hook installed (Session/Prompt/Stop events)")
        console.print(f"  command: {cmd}")
    elif action == "uninstall":
        for event, handlers in list(hooks.items()):
            hooks[event] = [h for h in handlers if not _matches_our_hook(h, cmd)]
            if not hooks[event]:
                del hooks[event]
        settings_path.write_text(json.dumps(settings, indent=2))
        console.print("[green]✓[/] hook removed")
    else:
        raise typer.BadParameter("action must be install, uninstall, or run")


def _matches_our_hook(handler: dict, cmd: str) -> bool:
    for h in handler.get("hooks", []):
        if h.get("command") == cmd:
            return True
    return False


def _hook_cmd() -> str:
    worklog_bin = shutil.which("worklog") or "worklog"
    return f"{worklog_bin} hook run"


@app.command()
def collect(
    source: str = typer.Argument("all", help="all | github | gcal | jira"),
    since: str = typer.Option(None, help="YYYY-MM-DD (default: 7 days ago)"),
    until: str = typer.Option(None, help="YYYY-MM-DD exclusive (default: tomorrow)"),
) -> None:
    """Pull activity from external sources into the local event store."""
    since_d = _parse_day(since) if since else date.today() - timedelta(days=7)
    until_d = _parse_day(until) if until else None

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
    for col in ("time", "src", "company", "title", "issue", "min"):
        table.add_column(col)
    for r in rows:
        table.add_row(
            r["started_at"][11:16],
            r["source"],
            r["company"] or "—",
            (r["title"] or "")[:60],
            r["jira_issue"] or "—",
            str((r["duration_seconds"] or 0) // 60),
        )
    console.print(table)


@app.command()
def infer(day: str = typer.Option(None, help="YYYY-MM-DD, default today")) -> None:
    """Cluster the day's events into blocks (gap-timeout algorithm)."""
    d = _parse_day(day)
    events = load_day_events(date=d)
    blocks = build_blocks(events)
    persist_blocks(blocks, day=d)
    total_min = sum(b.duration_seconds for b in blocks) // 60
    console.print(f"[green]✓[/] {d}: {len(blocks)} blocks, {total_min} min total")
    for b in blocks:
        flag = " [yellow](flagged)[/]" if b.flagged else ""
        console.print(
            f"  {b.started_at.strftime('%H:%M')}–{b.ended_at.strftime('%H:%M')} "
            f"{b.company} ({b.duration_seconds // 60}min, {b.event_count} events)"
            f"{flag}"
        )


@app.command()
def estimate(
    day: str = typer.Option(None, help="YYYY-MM-DD, default today"),
    model: str = typer.Option(DEFAULT_MODEL, help="claude model id (default: haiku 4.5)"),
) -> None:
    """Ask `claude -p` to write descriptions + sanity-check durations for blocks."""
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
    """Push reviewed events to Tempo."""
    d = _parse_day(day)
    results = sync_day(d, dry_run=dry_run)
    for r in results:
        console.print(r)


@app.command()
def serve(
    host: str = typer.Option("127.0.0.1"),
    port: int = typer.Option(8765),
) -> None:
    """Start the review web UI."""
    init_db()
    uvicorn.run("worklog.web.app:app", host=host, port=port, reload=False)


_COMPANIES_STUB = """\
# worklog companies configuration.
# First match wins across all companies (path → repo → jira → calendar → keyword).

default_company: null  # fallback when nothing matches

companies:
  - name: AcmeCorp
    tempo_account_key: ACME_ACCOUNT
    jira_issue_default: ACME-1       # fallback issue for commits/meetings without explicit issue
    path_prefixes:
      - /Users/tomas/projects/acme-
    github_repos:
      - AcmeCorp/api
      - AcmeCorp/web
    jira_projects:
      - ACME
    gcal_calendars: []
    gcal_keywords:
      - acme

  - name: SideClient
    tempo_account_key: SIDE_ACCOUNT
    jira_issue_default: SIDE-1
    path_prefixes:
      - /Users/tomas/projects/side-
    github_repos: []
    jira_projects:
      - SIDE
    gcal_keywords:
      - side client
"""
