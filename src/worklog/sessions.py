"""Session lifecycle: upsert on SessionStart, close on Stop/SessionEnd, reap stale.

Pure module — takes a connection, does SQL, returns. The Rust hook mirrors this
logic; keeping the Python version simple makes it the reference implementation.
"""

from __future__ import annotations

import sqlite3
from datetime import datetime, timedelta

REAPER_TTL = timedelta(minutes=5)


def open_session(
    conn: sqlite3.Connection,
    *,
    session_id: str,
    started_at: datetime,
    project_path: str | None = None,
) -> None:
    """Insert or bump event_count. Start time is NOT reset on subsequent calls."""
    conn.execute(
        """
        INSERT INTO sessions (session_id, started_at, project_path, event_count)
        VALUES (?, ?, ?, 1)
        ON CONFLICT(session_id) DO UPDATE SET
            project_path = COALESCE(sessions.project_path, excluded.project_path),
            event_count = sessions.event_count + 1
        """,
        (session_id, started_at.isoformat(), project_path),
    )


def close_session(
    conn: sqlite3.Connection,
    *,
    session_id: str,
    ended_at: datetime,
    end_source: str,
) -> None:
    """Record first close only. Stop → SubagentStop → SessionEnd → reaper ordering is intentional."""
    conn.execute(
        """
        UPDATE sessions
           SET ended_at = COALESCE(ended_at, ?),
               end_source = COALESCE(end_source, ?)
         WHERE session_id = ?
        """,
        (ended_at.isoformat(), end_source, session_id),
    )


def reap_stale(conn: sqlite3.Connection, *, now: datetime) -> int:
    """Close any still-open session whose last activity was > REAPER_TTL ago."""
    cutoff = (now - REAPER_TTL).isoformat()
    cursor = conn.execute(
        """
        UPDATE sessions
           SET ended_at = started_at,
               end_source = 'reaper'
         WHERE ended_at IS NULL AND started_at < ?
        """,
        (cutoff,),
    )
    return cursor.rowcount or 0
