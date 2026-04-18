"""DB-facing side of block inference: load events, persist blocks."""

from __future__ import annotations

import sqlite3
from datetime import date, datetime, timedelta

from dateutil.parser import isoparse

from worklog.db import connect
from worklog.infer import Block, InferEvent


def load_day_events(*, date: date, conn: sqlite3.Connection | None = None) -> list[InferEvent]:
    """All events whose start date matches `date`."""
    start = datetime.combine(date, datetime.min.time()).isoformat()
    end = datetime.combine(date + timedelta(days=1), datetime.min.time()).isoformat()

    def _query(c: sqlite3.Connection) -> list[InferEvent]:
        return [
            InferEvent(
                ts=isoparse(r["started_at"]),
                source=r["source"],
                duration_seconds=r["duration_seconds"],
                jira_issue=r["jira_issue"],
                event_id=r["id"],
            )
            for r in c.execute(
                "SELECT * FROM events WHERE started_at >= ? AND started_at < ? ORDER BY started_at",
                (start, end),
            ).fetchall()
        ]

    if conn is not None:
        return _query(conn)
    with connect() as owned:
        return _query(owned)


def persist_blocks(blocks: list[Block], *, day: date) -> None:
    """Replace the day's blocks transactionally, preserving tempo_worklog_id
    + description via (started_at) as the stable key across re-inferences.
    """
    day_iso = day.isoformat()
    with connect() as conn:
        prior = {
            r["started_at"]: r
            for r in conn.execute(
                "SELECT * FROM blocks WHERE day = ?", (day_iso,)
            ).fetchall()
        }
        conn.execute("DELETE FROM blocks WHERE day = ?", (day_iso,))
        for b in blocks:
            carry = prior.get(b.started_at.isoformat())
            tempo_id = carry["tempo_worklog_id"] if carry else None
            description = carry["description"] if carry else None
            estimated_by = carry["estimated_by"] if carry else None
            # If the user had manually assigned a ticket, preserve it; otherwise
            # fall back to inference.
            jira_issue = (
                carry["jira_issue"]
                if carry and carry["jira_issue"]
                else b.jira_issue
            )

            cur = conn.execute(
                """
                INSERT INTO blocks (
                    day, jira_issue, started_at, ended_at,
                    duration_seconds, description, estimated_by, flagged,
                    tempo_worklog_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    b.day,
                    jira_issue,
                    b.started_at.isoformat(),
                    b.ended_at.isoformat(),
                    b.duration_seconds,
                    description,
                    estimated_by,
                    1 if b.flagged else 0,
                    tempo_id,
                ),
            )
            block_id = cur.lastrowid
            conn.executemany(
                "INSERT INTO block_events (block_id, event_id) VALUES (?, ?)",
                [(block_id, eid) for eid in b.event_ids],
            )
