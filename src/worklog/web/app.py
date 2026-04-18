"""FastAPI review UI — operates on blocks.

Endpoints:
  GET  /              — daily view, blocks ordered by start time
  POST /blocks/{id}/ticket       — assign jira_issue (from dropdown)
  POST /blocks/{id}/duration     — edit duration (minutes)
  POST /blocks/{id}/description  — edit description
  POST /blocks/{id}/delete       — remove a block (e.g. lunch/noise)
  POST /infer                    — rebuild blocks for a day
  POST /estimate                 — run claude -p on blocks for a day
  POST /jira/refresh             — refresh open-ticket cache
  POST /sync                     — push blocks to Tempo
"""

from __future__ import annotations

from datetime import date, datetime, timedelta
from pathlib import Path

from fastapi import FastAPI, Form, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates

from worklog.db import connect, init_db
from worklog.estimate import estimate_day
from worklog.infer import build_blocks
from worklog.infer_persist import load_day_events, persist_blocks
from worklog.tempo import sync_day

BASE_DIR = Path(__file__).parent
templates = Jinja2Templates(directory=str(BASE_DIR / "templates"))

app = FastAPI(title="worklog")
app.mount("/static", StaticFiles(directory=str(BASE_DIR / "static")), name="static")


def _parse_day(s: str | None) -> date:
    if not s:
        return date.today()
    return datetime.strptime(s, "%Y-%m-%d").date()


def _nullable(s: str) -> str | None:
    s = (s or "").strip()
    return s or None


@app.get("/", response_class=HTMLResponse)
def index(request: Request, day: str | None = None) -> HTMLResponse:
    init_db()
    d = _parse_day(day)
    with connect() as conn:
        blocks = [
            dict(r)
            for r in conn.execute(
                "SELECT * FROM blocks WHERE day = ? ORDER BY started_at",
                (d.isoformat(),),
            ).fetchall()
        ]
        event_counts = {
            r["block_id"]: r["n"]
            for r in conn.execute(
                """
                SELECT be.block_id AS block_id, COUNT(*) AS n
                  FROM block_events be
                  JOIN blocks b ON b.id = be.block_id
                 WHERE b.day = ?
                 GROUP BY be.block_id
                """,
                (d.isoformat(),),
            ).fetchall()
        }
        source_rows = conn.execute(
            """
            SELECT be.block_id AS block_id, e.source AS source, COUNT(*) AS n
              FROM block_events be
              JOIN blocks b ON b.id = be.block_id
              JOIN events e ON e.id = be.event_id
             WHERE b.day = ?
             GROUP BY be.block_id, e.source
             ORDER BY n DESC
            """,
            (d.isoformat(),),
        ).fetchall()
        block_sources: dict[int, list[dict]] = {}
        for r in source_rows:
            block_sources.setdefault(r["block_id"], []).append(
                {"source": r["source"], "n": r["n"]}
            )
        tickets = [
            dict(r)
            for r in conn.execute(
                "SELECT key, summary, status FROM jira_tickets ORDER BY updated DESC"
            ).fetchall()
        ]
        ticket_cache_meta = conn.execute(
            "SELECT COUNT(*) AS n, MAX(fetched_at) AS last FROM jira_tickets"
        ).fetchone()

    for b in blocks:
        b["event_count"] = event_counts.get(b["id"], 0)
        b["sources"] = block_sources.get(b["id"], [])

    total_seconds = sum(b["duration_seconds"] for b in blocks)
    unassigned_count = sum(1 for b in blocks if not b["jira_issue"])

    return templates.TemplateResponse(
        request,
        "index.html",
        {
            "day": d,
            "prev_day": d - timedelta(days=1),
            "next_day": d + timedelta(days=1),
            "blocks": blocks,
            "total_seconds": total_seconds,
            "unassigned_count": unassigned_count,
            "tickets": tickets,
            "ticket_cache_count": ticket_cache_meta["n"] or 0,
            "ticket_cache_last": ticket_cache_meta["last"],
        },
    )


@app.post("/blocks/{block_id}/ticket")
def assign_ticket(
    block_id: int, jira_issue: str = Form(""), day: str = Form(...)
) -> RedirectResponse:
    key = _nullable(jira_issue)
    with connect() as conn:
        conn.execute(
            "UPDATE blocks SET jira_issue = ? WHERE id = ?",
            (key, block_id),
        )
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/blocks/{block_id}/duration")
def set_duration(
    block_id: int, minutes: int = Form(...), day: str = Form(...)
) -> RedirectResponse:
    with connect() as conn:
        conn.execute(
            "UPDATE blocks SET duration_seconds = ?, estimated_by = 'manual' WHERE id = ?",
            (minutes * 60, block_id),
        )
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/blocks/{block_id}/description")
def set_description(
    block_id: int, description: str = Form(...), day: str = Form(...)
) -> RedirectResponse:
    with connect() as conn:
        conn.execute(
            "UPDATE blocks SET description = ?, estimated_by = 'manual' WHERE id = ?",
            (description, block_id),
        )
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/blocks/{block_id}/delete")
def delete_block(block_id: int, day: str = Form(...)) -> RedirectResponse:
    with connect() as conn:
        conn.execute("DELETE FROM blocks WHERE id = ?", (block_id,))
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/infer")
def run_infer(day: str = Form(...)) -> RedirectResponse:
    d = _parse_day(day)
    events = load_day_events(date=d)
    persist_blocks(build_blocks(events), day=d)
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/estimate")
def run_estimate(day: str = Form(...)) -> RedirectResponse:
    estimate_day(_parse_day(day))
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/jira/refresh")
def refresh_tickets(day: str = Form(...)) -> RedirectResponse:
    # Imported lazily so the web app still starts if `jira` isn't configured.
    import logging

    from worklog.collectors.jira_ import fetch_open_tickets

    try:
        fetch_open_tickets()
    except Exception as e:  # noqa: BLE001 - keep UI resilient regardless of Jira state
        logging.getLogger("worklog.web").warning("jira refresh failed: %s", e)
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/sync")
def sync(day: str = Form(...), dry_run: str = Form("true")) -> HTMLResponse:
    results = sync_day(_parse_day(day), dry_run=(dry_run == "true"))
    body = "<pre>" + "\n".join(str(r) for r in results) + "</pre>"
    return HTMLResponse(body + f"<a href='/?day={day}'>← back</a>")
