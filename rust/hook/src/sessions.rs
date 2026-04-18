//! Mirror of Python sessions.py: open/close/reap with identical SQL.

use anyhow::Result;
use rusqlite::{params, Connection};

pub const REAPER_TTL_SECONDS: i64 = 5 * 60;

pub fn open_session(
    conn: &Connection,
    session_id: &str,
    started_at_iso: &str,
    project_path: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (session_id, started_at, project_path, event_count)
         VALUES (?, ?, ?, 1)
         ON CONFLICT(session_id) DO UPDATE SET
            project_path = COALESCE(sessions.project_path, excluded.project_path),
            event_count = sessions.event_count + 1",
        params![session_id, started_at_iso, project_path],
    )?;
    Ok(())
}

pub fn close_session(
    conn: &Connection,
    session_id: &str,
    ended_at_iso: &str,
    end_source: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE sessions
            SET ended_at = COALESCE(ended_at, ?),
                end_source = COALESCE(end_source, ?)
          WHERE session_id = ?",
        params![ended_at_iso, end_source, session_id],
    )?;
    Ok(())
}

pub fn reap_stale(conn: &Connection, cutoff_iso: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions
            SET ended_at = started_at,
                end_source = 'reaper'
          WHERE ended_at IS NULL AND started_at < ?",
        params![cutoff_iso],
    )?;
    Ok(())
}
