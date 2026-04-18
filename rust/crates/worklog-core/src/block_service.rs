//! Block mutations — the in-process API used by both the axum server
//! (Stage 3.2) and any future direct Rust callers.
//!
//! Every mutation takes a `&Connection` so the caller owns transactions
//! across multiple operations. Returns the fresh block row so clients
//! don't need a second round-trip.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use crate::models::Block;
use crate::repo;

pub fn assign_ticket(conn: &Connection, block_id: i64, key: Option<&str>) -> Result<Block> {
    conn.execute(
        "UPDATE blocks SET jira_issue = ?1 WHERE id = ?2",
        params![key, block_id],
    )
    .context("assign_ticket")?;
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

pub fn set_duration(conn: &Connection, block_id: i64, minutes: u32) -> Result<Block> {
    // Read started_at so we can derive the matching ended_at. Leaving
    // ended_at stale makes it disagree with duration_seconds, which
    // confuses both the UI and anyone who inspects the raw DB.
    let started_at: String = conn
        .query_row(
            "SELECT started_at FROM blocks WHERE id = ?1",
            params![block_id],
            |r| r.get(0),
        )
        .with_context(|| format!("block {block_id} not found"))?;
    let new_end = derive_ended_at(&started_at, minutes as i64 * 60)?;

    // Mark as manual so re-estimation doesn't clobber it.
    conn.execute(
        "UPDATE blocks
            SET duration_seconds = ?1, ended_at = ?2, estimated_by = 'manual'
          WHERE id = ?3",
        params![minutes as i64 * 60, new_end, block_id],
    )
    .context("set_duration")?;
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

/// Compute the canonical ended_at string from started_at (ISO-8601) and
/// duration. Output format mirrors how the infer+repo layers emit
/// timestamps so round-trips stay byte-stable.
fn derive_ended_at(started_at: &str, duration_seconds: i64) -> Result<String> {
    let start: DateTime<Utc> = DateTime::parse_from_rfc3339(started_at)
        .with_context(|| format!("parsing started_at `{started_at}` as RFC3339"))?
        .with_timezone(&Utc);
    let end = start + Duration::seconds(duration_seconds);
    // Match the repo's emission: `2026-04-18T09:00:00+00:00` (no fractional
    // seconds when whole seconds). chrono's `%:z` yields `+00:00`.
    Ok(end.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

pub fn set_description(conn: &Connection, block_id: i64, description: &str) -> Result<Block> {
    conn.execute(
        "UPDATE blocks
            SET description = ?1, estimated_by = 'manual'
          WHERE id = ?2",
        params![description, block_id],
    )
    .context("set_description")?;
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

pub fn delete_block(conn: &Connection, block_id: i64) -> Result<()> {
    let n = conn
        .execute("DELETE FROM blocks WHERE id = ?1", params![block_id])
        .context("delete_block")?;
    if n == 0 {
        anyhow::bail!("block {block_id} not found");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn seed(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn assign_ticket_updates_and_clears() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = assign_ticket(&conn, id, Some("PROJ-1")).unwrap();
        assert_eq!(got.jira_issue.as_deref(), Some("PROJ-1"));
        let got = assign_ticket(&conn, id, None).unwrap();
        assert!(got.jira_issue.is_none());
    }

    #[test]
    fn set_duration_marks_manual_and_stores_seconds() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = set_duration(&conn, id, 45).unwrap();
        assert_eq!(got.duration_seconds, 45 * 60);
        assert_eq!(got.estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn set_duration_updates_ended_at_to_match() {
        // Regression: set_duration used to leave ended_at stale, so
        // ended_at - started_at disagreed with duration_seconds. Anyone
        // reading the raw DB (or the UI time range) saw the old end.
        let conn = open_memory().unwrap();
        let id = seed(&conn); // started 09:00, ended 09:30, 1800s
        let got = set_duration(&conn, id, 120).unwrap();
        assert_eq!(got.duration_seconds, 120 * 60);
        // 09:00 + 120m = 11:00 — match the same ISO format we store.
        assert_eq!(
            got.ended_at, "2026-04-18T11:00:00+00:00",
            "ended_at must track duration_seconds"
        );
    }

    #[test]
    fn set_description_marks_manual() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = set_description(&conn, id, "hello").unwrap();
        assert_eq!(got.description.as_deref(), Some("hello"));
        assert_eq!(got.estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn delete_block_removes_row() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        delete_block(&conn, id).unwrap();
        assert!(repo::get_block(&conn, id).unwrap().is_none());
    }

    #[test]
    fn errors_on_missing_block() {
        let conn = open_memory().unwrap();
        assert!(assign_ticket(&conn, 9999, Some("PROJ-1")).is_err());
        assert!(set_duration(&conn, 9999, 10).is_err());
        assert!(set_description(&conn, 9999, "x").is_err());
        assert!(delete_block(&conn, 9999).is_err());
    }
}
