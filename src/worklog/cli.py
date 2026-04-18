from __future__ import annotations

from datetime import date, datetime, timedelta

import typer
import uvicorn
from rich.console import Console
from rich.prompt import Confirm, Prompt
from rich.table import Table

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
        values[key] = (value or "").strip()

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
    """Cluster the day's events into blocks (gap-timeout algorithm)."""
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
    """Ask `claude -p` to pick a Jira ticket + write a description for each block."""
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
    """Push reviewed blocks to Tempo (one worklog per block)."""
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
    host: str = typer.Option("127.0.0.1"),
    port: int = typer.Option(8765),
) -> None:
    """Start the review web UI."""
    init_db()
    uvicorn.run("worklog.web.app:app", host=host, port=port, reload=False)
