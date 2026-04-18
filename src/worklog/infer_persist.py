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
    + description across re-inferences. Primary key is ``started_at``;
    when a new block's ``started_at`` shifted (because a backfilled
    earlier event moved the cluster's start), we fall back to an overlap
    match against the prior list — otherwise the CLAUDE.md canary
    invariant ("tempo_worklog_id MUST NEVER be cleared") would silently
    break on the next re-inference.
    """
    day_iso = day.isoformat()
    with connect() as conn:
        prior_rows = conn.execute(
            "SELECT * FROM blocks WHERE day = ? ORDER BY started_at",
            (day_iso,),
        ).fetchall()
        prior = {r["started_at"]: r for r in prior_rows}
        # Parse each prior's range once so the overlap fallback doesn't
        # re-parse on every inner-loop iteration.
        prior_parsed = [
            (isoparse(r["started_at"]), isoparse(r["ended_at"]), r)
            for r in prior_rows
        ]
        claimed_keys: set[str] = set()

        conn.execute("DELETE FROM blocks WHERE day = ?", (day_iso,))
        for b in blocks:
            key = b.started_at.isoformat()
            carry = prior.get(key)
            if carry is None:
                # Overlap fallback: find an unclaimed prior whose time
                # range overlaps this new block. Parsing both sides to
                # datetimes sidesteps any format drift between the
                # writer (Python isoformat) and the reader (Rust's
                # block_iso), which used to produce non-matching strings
                # for whole-second timestamps.
                for prev_start, prev_end, row in prior_parsed:
                    if row["started_at"] in claimed_keys:
                        continue
                    if b.started_at < prev_end and prev_start < b.ended_at:
                        carry = row
                        claimed_keys.add(row["started_at"])
                        break
            elif carry is not None:
                claimed_keys.add(key)

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
