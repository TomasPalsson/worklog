"""Tempo Cloud (tempo.io) worklog sync.

Reads reviewed events from the DB and posts worklogs to Tempo v4 API.
Each event's tempo_worklog_id is filled in after success so re-syncing is safe.

Grouping strategy: events for the same (date, company, jira_issue) are coalesced
into a single worklog per day. Duration is the sum of event durations, or — for
events without duration (commits, point-in-time) — we use the configured default
per company or ignore them.
"""

from __future__ import annotations

import sqlite3
from collections import defaultdict
from datetime import date, datetime, timedelta
from typing import Any

import httpx

from worklog.config import Settings, load_companies
from worklog.db import connect


def _group_for_day(
    conn: sqlite3.Connection, day: date
) -> dict[tuple[str, str], list[sqlite3.Row]]:
    """Group synced-able events on `day` by (company, jira_issue)."""
    start = datetime.combine(day, datetime.min.time()).isoformat()
    end = datetime.combine(day + timedelta(days=1), datetime.min.time()).isoformat()
    rows = conn.execute(
        """
        SELECT * FROM events
        WHERE started_at >= ? AND started_at < ?
          AND company IS NOT NULL
          AND tempo_worklog_id IS NULL
        """,
        (start, end),
    ).fetchall()
    groups: dict[tuple[str, str], list[sqlite3.Row]] = defaultdict(list)
    for r in rows:
        issue = r["jira_issue"]
        if not issue:
            # no jira issue → fall back to company-level default (set later)
            issue = ""
        groups[(r["company"], issue)].append(r)
    return groups


def sync_day(
    day: date,
    *,
    dry_run: bool = False,
    settings: Settings | None = None,
) -> list[dict[str, Any]]:
    settings = settings or Settings()
    if not dry_run and not settings.tempo_token:
        raise RuntimeError("WORKLOG_TEMPO_TOKEN not set")
    config = load_companies()
    company_map = {c.name: c for c in config.companies}

    results: list[dict[str, Any]] = []
    headers = {
        "Authorization": f"Bearer {settings.tempo_token}",
        "Content-Type": "application/json",
    }

    with connect() as conn, httpx.Client(headers=headers, timeout=30) as client:
        groups = _group_for_day(conn, day)
        for (company_name, issue), rows in groups.items():
            company = company_map.get(company_name)
            issue_key = issue or (company.jira_issue_default if company else None)
            if not issue_key:
                results.append(
                    {
                        "company": company_name,
                        "status": "skipped",
                        "reason": "no jira_issue and no jira_issue_default for company",
                        "event_ids": [r["id"] for r in rows],
                    }
                )
                continue

            total_seconds = sum(r["duration_seconds"] or 0 for r in rows)
            if total_seconds == 0:
                # No explicit durations — default each commit/PR to 15min placeholder
                total_seconds = 15 * 60 * max(1, len(rows))

            description = " | ".join(r["title"] for r in rows)[:2000]
            payload = {
                "issueKey": issue_key,
                "timeSpentSeconds": total_seconds,
                "startDate": day.isoformat(),
                "startTime": "09:00:00",
                "description": description,
                "authorAccountId": settings.jira_email or "",
            }
            if company and company.tempo_account_key:
                payload["attributes"] = [
                    {"key": "_Account_", "value": company.tempo_account_key}
                ]

            if dry_run:
                results.append({"company": company_name, "status": "dry-run", "payload": payload})
                continue

            r = client.post(f"{settings.tempo_base_url}/worklogs", json=payload)
            if r.status_code >= 300:
                results.append(
                    {
                        "company": company_name,
                        "status": "error",
                        "http_status": r.status_code,
                        "body": r.text,
                    }
                )
                continue
            worklog_id = str(r.json().get("tempoWorklogId"))
            ids = [row["id"] for row in rows]
            conn.executemany(
                "UPDATE events SET tempo_worklog_id = ? WHERE id = ?",
                [(worklog_id, i) for i in ids],
            )
            results.append(
                {
                    "company": company_name,
                    "status": "synced",
                    "tempo_worklog_id": worklog_id,
                    "event_ids": ids,
                }
            )
    return results
