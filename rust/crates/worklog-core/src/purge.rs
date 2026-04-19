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

/// Purge everything older than `retention_days` that's safe to drop.
/// Returns a report regardless of `dry_run` — callers render it for the
/// user.
pub fn purge(_conn: &Connection, _retention_days: i64, _dry_run: bool) -> Result<PurgeReport> {
    anyhow::bail!("purge::purge not yet implemented")
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
