"""FastAPI review UI.

Browse activity by day, reassign companies, edit durations, trigger a Tempo sync.
Run with: `worklog serve` → http://127.0.0.1:8765
"""

from __future__ import annotations

from datetime import date, datetime, timedelta
from pathlib import Path
from typing import Any

from fastapi import FastAPI, Form, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates

from worklog.config import load_companies
from worklog.db import connect, init_db
from worklog.tempo import sync_day

BASE_DIR = Path(__file__).parent
templates = Jinja2Templates(directory=str(BASE_DIR / "templates"))

app = FastAPI(title="worklog")
app.mount("/static", StaticFiles(directory=str(BASE_DIR / "static")), name="static")


def _parse_day(s: str | None) -> date:
    if not s:
        return date.today()
    return datetime.strptime(s, "%Y-%m-%d").date()


@app.get("/", response_class=HTMLResponse)
def index(request: Request, day: str | None = None) -> HTMLResponse:
    init_db()
    d = _parse_day(day)
    start = datetime.combine(d, datetime.min.time()).isoformat()
    end = datetime.combine(d + timedelta(days=1), datetime.min.time()).isoformat()

    with connect() as conn:
        rows = conn.execute(
            """
            SELECT * FROM events
            WHERE started_at >= ? AND started_at < ?
            ORDER BY started_at ASC
            """,
            (start, end),
        ).fetchall()

    companies = load_companies().companies
    totals: dict[str, int] = {}
    by_company: dict[str, list[dict[str, Any]]] = {}
    for r in rows:
        key = r["company"] or "unassigned"
        by_company.setdefault(key, []).append(dict(r))
        totals[key] = totals.get(key, 0) + (r["duration_seconds"] or 0)

    return templates.TemplateResponse(
        request,
        "index.html",
        {
            "day": d,
            "prev_day": d - timedelta(days=1),
            "next_day": d + timedelta(days=1),
            "by_company": by_company,
            "totals": totals,
            "companies": companies,
        },
    )


@app.post("/events/{event_id}/assign")
def assign_company(
    event_id: int,
    company: str = Form(...),
    day: str = Form(...),
) -> RedirectResponse:
    with connect() as conn:
        conn.execute(
            "UPDATE events SET company = ? WHERE id = ?",
            (company or None, event_id),
        )
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/events/{event_id}/duration")
def set_duration(event_id: int, minutes: int = Form(...), day: str = Form(...)) -> RedirectResponse:
    with connect() as conn:
        conn.execute(
            "UPDATE events SET duration_seconds = ? WHERE id = ?",
            (minutes * 60, event_id),
        )
    return RedirectResponse(url=f"/?day={day}", status_code=303)


@app.post("/sync")
def sync(day: str = Form(...), dry_run: bool = Form(False)) -> HTMLResponse:
    d = _parse_day(day)
    results = sync_day(d, dry_run=dry_run)
    return HTMLResponse(
        "<pre>"
        + "\n".join(str(r) for r in results)
        + f"\n</pre><a href='/?day={day}'>back</a>"
    )
