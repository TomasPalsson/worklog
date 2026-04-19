//! Retention policy — drop old events + blocks once they've been synced
//! to Tempo. The billing cycle runs 20th-to-19th so by the time a block
//! is >30 days old it's already invoiced; holding onto it forever just
//! bloats the db and the UI's day-picker.
//!
//! Safety rails:
//! * Blocks with `estimated_by = 'manual'` are NEVER deleted — they're
//!   the user's hand-edit and the ground truth.
//! * Blocks with no `tempo_worklog_id` (empty string OR NULL, per
//!   `normalise_tempo_id`) are kept unless they're explicitly a `'gap'`
//!   — the user hasn't reviewed them yet.
//! * Orphan events (not linked to any surviving block) are deleted too,
//!   but only when *they* are older than the cutoff — we'd rather keep
//!   an un-block'd event from yesterday than drop it silently.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Default retention window. See `CLAUDE.md` — billing cycle is 20th to
/// 19th, so anything >30 days old has been through a full sync cycle.
pub const DEFAULT_RETENTION_DAYS: i64 = 30;

/// What the purge did (or would have done, if `dry_run`).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PurgeReport {
    /// ISO `YYYY-MM-DD` — anything before this is fair game.
    pub cutoff_date: String,
    /// Blocks that were (or would be) deleted.
    pub blocks_deleted: i64,
    /// Events (orphan or cascaded) that were (or would be) deleted.
    pub events_deleted: i64,
    /// Old blocks we kept because the user hasn't synced them yet.
    pub blocks_kept_unsynced: i64,
    /// Old blocks we kept because the user hand-edited them.
    pub blocks_kept_manual: i64,
    /// If true, nothing was actually written to the database.
    pub dry_run: bool,
}

/// SQL fragment matching blocks that are old AND safe to delete:
/// synced to Tempo OR explicitly marked as `gap`, excluding manual
/// edits. Kept as a named const so the delete + counting queries use
/// identical logic and can't drift.
const PURGEABLE_BLOCKS_WHERE: &str = "
    day < ?1
    AND (estimated_by IS NULL OR estimated_by != 'manual')
    AND (
        (tempo_worklog_id IS NOT NULL AND tempo_worklog_id != '')
        OR estimated_by = 'gap'
    )
";

/// Blocks we decline to delete because the user hasn't synced (or
/// reviewed) them yet. Counted for the report so the user can see why
/// the rule preserved something.
const KEPT_UNSYNCED_WHERE: &str = "
    day < ?1
    AND (estimated_by IS NULL OR estimated_by != 'manual')
    AND (tempo_worklog_id IS NULL OR tempo_worklog_id = '')
    AND (estimated_by IS NULL OR estimated_by != 'gap')
";

const KEPT_MANUAL_WHERE: &str = "
    day < ?1 AND estimated_by = 'manual'
";

/// Purge everything older than `retention_days` that's safe to drop.
/// Returns a report regardless of `dry_run` — callers render it for the
/// user.
pub fn purge(conn: &Connection, retention_days: i64, dry_run: bool) -> Result<PurgeReport> {
    let cutoff = chrono::Utc::now().date_naive() - chrono::Duration::days(retention_days);
    let cutoff_iso = cutoff.to_string();

    // Count informational "kept" rows up front — these stay whether or
    // not we're in dry-run. Purely for the user-facing report.
    let blocks_kept_unsynced: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM blocks WHERE {KEPT_UNSYNCED_WHERE}"),
            params![cutoff_iso],
            |r| r.get(0),
        )
        .context("counting unsynced blocks past cutoff")?;
    let blocks_kept_manual: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM blocks WHERE {KEPT_MANUAL_WHERE}"),
            params![cutoff_iso],
            |r| r.get(0),
        )
        .context("counting manual-edited blocks past cutoff")?;

    // Count + delete purgeable blocks in one shot under a tx so the
    // cascade on block_events lands atomically.
    let blocks_deleted: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM blocks WHERE {PURGEABLE_BLOCKS_WHERE}"),
            params![cutoff_iso],
            |r| r.get(0),
        )
        .context("counting purgeable blocks")?;

    // An orphan event is one no surviving block references AND that itself
    // is older than the cutoff. `substr(started_at, 1, 10)` lifts the date
    // out of the ISO-8601 TEXT column — fast enough at our scale and
    // matches how `load_day_events` already slices.
    let events_deleted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events
             WHERE substr(started_at, 1, 10) < ?1
               AND id NOT IN (
                   SELECT event_id FROM block_events WHERE block_id IN (
                       SELECT id FROM blocks WHERE NOT (day < ?1 AND (
                           (estimated_by IS NULL OR estimated_by != 'manual')
                           AND (
                               (tempo_worklog_id IS NOT NULL AND tempo_worklog_id != '')
                               OR estimated_by = 'gap'
                           )
                       ))
                   )
               )",
            params![cutoff_iso],
            |r| r.get(0),
        )
        .context("counting orphan events past cutoff")?;

    let report = PurgeReport {
        cutoff_date: cutoff_iso.clone(),
        blocks_deleted,
        events_deleted,
        blocks_kept_unsynced,
        blocks_kept_manual,
        dry_run,
    };

    if dry_run {
        return Ok(report);
    }

    // Real run — do the deletes in a single transaction so a crash
    // mid-purge doesn't leave block_events dangling (the FK cascade
    // already handles that, but txn keeps counts consistent with what
    // we reported).
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("DELETE FROM blocks WHERE {PURGEABLE_BLOCKS_WHERE}"),
        params![cutoff_iso],
    )
    .context("deleting purgeable blocks")?;
    tx.execute(
        "DELETE FROM events
         WHERE substr(started_at, 1, 10) < ?1
           AND id NOT IN (SELECT event_id FROM block_events)",
        params![cutoff_iso],
    )
    .context("deleting orphan events")?;
    tx.commit()?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;

    /// Insert a block with every field the purge rule cares about.
    fn insert_block(
        conn: &Connection,
        day: &str,
        tempo_id: Option<&str>,
        estimated_by: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds,
                                 tempo_worklog_id, estimated_by)
             VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00',
                     1800, ?2, ?3)",
            params![day, tempo_id, estimated_by],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_event(conn: &Connection, started_at: &str, source_id: &str) -> i64 {
        repo::upsert_event(
            conn,
            &Event::minimal("github_commit", source_id, started_at, "commit"),
        )
        .unwrap()
    }

    fn link(conn: &Connection, block_id: i64, event_id: i64) {
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, event_id],
        )
        .unwrap();
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn b5_deletes_old_synced_block_and_its_events() {
        // B5: >30d old + tempo_worklog_id present → block + linked events
        // both cleared.
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn, "2026-02-10", Some("tempo-1"), None);
        let eid = insert_event(&conn, "2026-02-10T09:05:00+00:00", "ev-old-1");
        link(&conn, bid, eid);

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 1);
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "blocks"), 0);
        assert_eq!(count(&conn, "events"), 0);
    }

    #[test]
    fn b6_deletes_old_gap_block() {
        // B6: estimated_by='gap' means the user reviewed and discarded
        // this block. Safe to drop once it's past the window.
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", None, Some("gap"));

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 1);
        assert_eq!(count(&conn, "blocks"), 0);
    }

    #[test]
    fn b7_keeps_old_unsynced_block() {
        // B7: old but never synced → user still needs to sync. Don't
        // delete unreviewed work silently.
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", None, None);

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_kept_unsynced, 1);
        assert_eq!(count(&conn, "blocks"), 1);
    }

    #[test]
    fn b7_keeps_old_block_with_empty_string_tempo_id() {
        // CLAUDE.md: empty string AND NULL both mean "unsynced". A block
        // that was re-estimated then had its tempo_id cleared (happens
        // via normalise_tempo_id) must NOT be deleted.
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", Some(""), None);

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(count(&conn, "blocks"), 1);
    }

    #[test]
    fn b8_keeps_old_manual_block_even_when_synced() {
        // B8: user's hand-edit is ground truth. Never delete, regardless
        // of how old it is or whether it's synced. Matches the estimator
        // invariant in CLAUDE.md.
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-2"), Some("manual"));

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_kept_manual, 1);
        assert_eq!(count(&conn, "blocks"), 1);
    }

    #[test]
    fn b9_keeps_recent_block_even_when_synced() {
        // B9: today-ish data — don't touch, it's the current cycle.
        let conn = open_memory().unwrap();
        let today = chrono::Utc::now().date_naive();
        let recent = today - chrono::Duration::days(5);
        insert_block(&conn, &recent.to_string(), Some("tempo-fresh"), None);

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(count(&conn, "blocks"), 1);
    }

    #[test]
    fn b10_dry_run_reports_but_does_not_delete() {
        // B10: dry-run is how users sanity-check the rule before pulling
        // the trigger. Report must still be accurate.
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-3"), None);
        insert_block(&conn, "2026-02-11", None, Some("gap"));

        let report = purge(&conn, 30, true).unwrap();
        assert_eq!(report.blocks_deleted, 2);
        assert!(report.dry_run);
        // Nothing actually removed.
        assert_eq!(count(&conn, "blocks"), 2);
    }

    #[test]
    fn deletes_orphan_events_older_than_cutoff() {
        // An event never linked to a block but sitting in the table past
        // the retention window should go too. Protects against collector
        // drift leaving stale rows forever.
        let conn = open_memory().unwrap();
        insert_event(&conn, "2026-02-10T12:00:00+00:00", "orphan-old");
        let today = chrono::Utc::now();
        let fresh_ts = today.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
        insert_event(&conn, &fresh_ts, "orphan-fresh");

        let report = purge(&conn, 30, false).unwrap();
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "events"), 1);
    }

    #[test]
    fn cutoff_date_is_reported() {
        // Rendering in the CLI needs a date to tell the user what
        // window we used. Must be ISO `YYYY-MM-DD`.
        let conn = open_memory().unwrap();
        let report = purge(&conn, 30, true).unwrap();
        assert_eq!(report.cutoff_date.len(), 10);
        assert_eq!(&report.cutoff_date[4..5], "-");
        assert_eq!(&report.cutoff_date[7..8], "-");
    }
}
