"""Tempo Cloud worklog sync — reads from the `blocks` table, not raw events.

Each block becomes exactly one Tempo worklog. `tempo_worklog_id` on the block
row is set after a successful POST so re-syncing is safe and cheap.
"""

from __future__ import annotations

from datetime import date
from typing import Any

import httpx

from worklog.config import Settings, load_companies
from worklog.db import connect


def _start_time(started_at_iso: str) -> str:
    # Tempo v4 wants HH:MM:SS; the block's ISO-8601 contains timezone — strip.
    return started_at_iso[11:19]


def sync_day(
    day: date,
    *,
    dry_run: bool = False,
    settings: Settings | None = None,
) -> list[dict[str, Any]]:
    settings = settings or Settings()
    if not dry_run and not settings.tempo_token:
        raise RuntimeError("WORKLOG_TEMPO_TOKEN not set")
    companies = {c.name: c for c in load_companies().companies}

    results: list[dict[str, Any]] = []
    headers = {
        "Authorization": f"Bearer {settings.tempo_token or ''}",
        "Content-Type": "application/json",
    }

    with connect() as conn, httpx.Client(headers=headers, timeout=30) as client:
        blocks = conn.execute(
            """
            SELECT * FROM blocks
             WHERE day = ? AND tempo_worklog_id IS NULL
             ORDER BY started_at
            """,
            (day.isoformat(),),
        ).fetchall()

        for block in blocks:
            company = companies.get(block["company"])
            issue_key = block["jira_issue"] or (
                company.jira_issue_default if company else None
            )
            if not issue_key:
                results.append(
                    {
                        "block_id": block["id"],
                        "company": block["company"],
                        "status": "skipped",
                        "reason": "no jira_issue and no company default",
                    }
                )
                continue

            payload: dict[str, Any] = {
                "issueKey": issue_key,
                "timeSpentSeconds": block["duration_seconds"],
                "startDate": block["day"],
                "startTime": _start_time(block["started_at"]),
                "description": block["description"] or f"Work on {issue_key}",
                "authorAccountId": settings.jira_email or "",
            }
            if company and company.tempo_account_key:
                payload["attributes"] = [
                    {"key": "_Account_", "value": company.tempo_account_key}
                ]

            if dry_run:
                results.append(
                    {"block_id": block["id"], "status": "dry-run", "payload": payload}
                )
                continue

            resp = client.post(f"{settings.tempo_base_url}/worklogs", json=payload)
            if resp.status_code >= 300:
                results.append(
                    {
                        "block_id": block["id"],
                        "status": "error",
                        "http_status": resp.status_code,
                        "body": resp.text,
                    }
                )
                continue
            worklog_id = str(resp.json().get("tempoWorklogId"))
            conn.execute(
                "UPDATE blocks SET tempo_worklog_id = ? WHERE id = ?",
                (worklog_id, block["id"]),
            )
            results.append(
                {
                    "block_id": block["id"],
                    "status": "synced",
                    "tempo_worklog_id": worklog_id,
                }
            )
    return results
