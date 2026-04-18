//! Session lifecycle — matches `src/worklog/sessions.py` line for line so
//! the Rust and Python hook implementations converge on identical db state.
//!
//! Pure module: every function takes an open `Connection` and runs SQL. No
//! globals, no i/o beyond the db.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

/// Reaper TTL — any session whose `started_at` is older than this with no
/// `ended_at` yet gets closed with `end_source = 'reaper'`.
pub const REAPER_TTL_MINUTES: i64 = 5;

pub fn open_session(
    conn: &Connection,
    session_id: &str,
    started_at: DateTime<Utc>,
    project_path: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (session_id, started_at, project_path, event_count)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(session_id) DO UPDATE SET
             project_path = COALESCE(sessions.project_path, excluded.project_path),
             event_count  = sessions.event_count + 1",
        params![session_id, started_at.to_rfc3339(), project_path],
    )?;
    Ok(())
}

pub fn close_session(
    conn: &Connection,
    session_id: &str,
    ended_at: DateTime<Utc>,
    end_source: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE sessions
            SET ended_at   = COALESCE(ended_at, ?1),
                end_source = COALESCE(end_source, ?2)
          WHERE session_id = ?3",
        params![ended_at.to_rfc3339(), end_source, session_id],
    )?;
    Ok(())
}

pub fn reap_stale(conn: &Connection, now: DateTime<Utc>) -> Result<usize> {
    let cutoff = (now - Duration::minutes(REAPER_TTL_MINUTES)).to_rfc3339();
    let n = conn.execute(
        "UPDATE sessions
            SET ended_at   = started_at,
                end_source = 'reaper'
          WHERE ended_at IS NULL AND started_at < ?1",
        params![cutoff],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use chrono::TimeZone;

    #[test]
    fn open_session_is_idempotent_and_bumps_count() {
        let conn = open_memory().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        open_session(&conn, "sess-1", now, Some("/tmp/p")).unwrap();
        open_session(&conn, "sess-1", now + Duration::minutes(1), None).unwrap();
        open_session(&conn, "sess-1", now + Duration::minutes(2), None).unwrap();

        let (count, project): (i64, Option<String>) = conn
            .query_row(
                "SELECT event_count, project_path FROM sessions WHERE session_id = ?1",
                params!["sess-1"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 3);
        assert_eq!(project.as_deref(), Some("/tmp/p"));
    }

    #[test]
    fn close_session_records_first_close_only() {
        let conn = open_memory().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        open_session(&conn, "sess-1", now, None).unwrap();
        close_session(&conn, "sess-1", now + Duration::minutes(10), "stop").unwrap();
        close_session(&conn, "sess-1", now + Duration::minutes(20), "session_end").unwrap();

        let (ended_at, end_source): (String, String) = conn
            .query_row(
                "SELECT ended_at, end_source FROM sessions WHERE session_id = ?1",
                params!["sess-1"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(ended_at.starts_with("2026-04-18T09:10"));
        assert_eq!(end_source, "stop");
    }

    #[test]
    fn reap_stale_closes_abandoned_sessions() {
        let conn = open_memory().unwrap();
        let far_past = Utc.with_ymd_and_hms(2026, 4, 18, 8, 0, 0).unwrap();
        open_session(&conn, "stale", far_past, None).unwrap();
        let recent = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        open_session(&conn, "fresh", recent, None).unwrap();

        let now = Utc.with_ymd_and_hms(2026, 4, 18, 9, 3, 0).unwrap();
        let n = reap_stale(&conn, now).unwrap();
        assert_eq!(n, 1);

        let stale_end_source: Option<String> = conn
            .query_row(
                "SELECT end_source FROM sessions WHERE session_id = ?1",
                params!["stale"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale_end_source.as_deref(), Some("reaper"));
        let fresh_end: Option<String> = conn
            .query_row(
                "SELECT end_source FROM sessions WHERE session_id = ?1",
                params!["fresh"],
                |r| r.get(0),
            )
            .unwrap();
        assert!(fresh_end.is_none());
    }
}
