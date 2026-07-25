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
//!
//! NOTE: the above rails are being replaced by a rail-free, billing-cycle
//! aligned cutoff (see spec 002-billing-cycle-pruner). `cutoff_for_cycle`,
//! `cutoff_for_days` and `purge_rows` are the new surface; `purge` and its
//! supporting rail consts are still here only until the implementation
//! lands.

use anyhow::{Context, Result};
use chrono::NaiveDate;
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
    /// Subset of `blocks_deleted` that had NEITHER `tempo_worklog_id` NOR
    /// `exported_at` — work that was never billed. Not a rail: these are
    /// still deleted. It exists so the loss is visible instead of silent.
    pub blocks_deleted_unbilled: i64,
    /// Events (orphan or cascaded) that were (or would be) deleted.
    pub events_deleted: i64,
    /// Sessions that were (or would be) deleted. Populated from slice 2.
    pub sessions_deleted: i64,
    /// Manually-picked ticket cache entries deleted. Populated from slice 2.
    pub tickets_deleted: i64,
    /// Disk space reclaimed, in bytes. Populated from slice 3.
    pub bytes_freed: i64,
    /// Where the pre-prune snapshot was written. Populated from slice 3.
    pub snapshot_path: Option<String>,
    /// If true, nothing was actually written to the database.
    pub dry_run: bool,
    /// Old blocks we kept because the user hasn't synced them yet. Only
    /// populated by the legacy [`purge`] rail — dropped once that
    /// function is deleted.
    pub blocks_kept_unsynced: i64,
    /// Old blocks we kept because the user hand-edited them. Only
    /// populated by the legacy [`purge`] rail — dropped once that
    /// function is deleted.
    pub blocks_kept_manual: i64,
}

/// SQL fragment matching blocks that are old AND safe to delete:
/// synced to Tempo, exported for billing, OR explicitly marked as
/// `gap`, excluding manual edits. `exported_at` is the Tempo-independent
/// "has been billed" marker (the team moved off Tempo — see CLAUDE.md /
/// billing.rs) and is treated as full parity with `tempo_worklog_id`
/// here. Kept as a named const so the delete + counting queries use
/// identical logic and can't drift.
const PURGEABLE_BLOCKS_WHERE: &str = "
    day < ?1
    AND (estimated_by IS NULL OR estimated_by != 'manual')
    AND (
        (tempo_worklog_id IS NOT NULL AND tempo_worklog_id != '')
        OR (exported_at IS NOT NULL AND exported_at != '')
        OR estimated_by = 'gap'
    )
";

/// Blocks we decline to delete because the user hasn't synced (or
/// reviewed, or exported) them yet. Counted for the report so the user
/// can see why the rule preserved something.
const KEPT_UNSYNCED_WHERE: &str = "
    day < ?1
    AND (estimated_by IS NULL OR estimated_by != 'manual')
    AND (tempo_worklog_id IS NULL OR tempo_worklog_id = '')
    AND (exported_at IS NULL OR exported_at = '')
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
            &format!(
                "SELECT COUNT(*) FROM events
                 WHERE substr(started_at, 1, 10) < ?1
                   AND id NOT IN (
                       SELECT event_id FROM block_events WHERE block_id IN (
                           SELECT id FROM blocks WHERE NOT ({PURGEABLE_BLOCKS_WHERE})
                       )
                   )"
            ),
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
        ..Default::default()
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

/// Billing cycles run `cycle_start_day` (default the 20th) through the
/// day before `cycle_start_day` in the following month. Configurable in
/// spirit, but v1 ships defaults only — see spec 002 §4.1.
pub const DEFAULT_CYCLE_START_DAY: u32 = 20;
/// Last day of the month on which hours can still be submitted against
/// the cycle that just closed — default the 23rd (second business day
/// after the 19th, in the general case).
pub const DEFAULT_CLOSE_DAY: u32 = 23;

/// The number of days in `year`-`month`, via the first-of-next-month
/// minus first-of-this-month trick (handles the December → January
/// wraparound for free).
fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_of_next =
        NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("month + 1 is always valid");
    let first_of_this =
        NaiveDate::from_ymd_opt(year, month, 1).expect("(year, month) is always valid");
    (first_of_next - first_of_this).num_days() as u32
}

/// `cycle_start_day` clamped to the length of `year`-`month`, so a
/// configured value like 31 is legal even in a 28/29/30-day month.
fn effective_start_day(year: i32, month: u32, cycle_start_day: u32) -> u32 {
    cycle_start_day.min(days_in_month(year, month))
}

/// The most recent cycle-start day on or before `d`, per the algorithm
/// in spec 002 Appendix A.
pub fn cycle_start_on_or_before(d: NaiveDate, cycle_start_day: u32) -> NaiveDate {
    use chrono::Datelike;
    let eff = effective_start_day(d.year(), d.month(), cycle_start_day);
    if d.day() >= eff {
        NaiveDate::from_ymd_opt(d.year(), d.month(), eff)
            .expect("effective_start_day is clamped to days_in_month")
    } else {
        let (py, pm) = if d.month() == 1 {
            (d.year() - 1, 12)
        } else {
            (d.year(), d.month() - 1)
        };
        let peff = effective_start_day(py, pm, cycle_start_day);
        NaiveDate::from_ymd_opt(py, pm, peff)
            .expect("effective_start_day is clamped to days_in_month")
    }
}

/// The billing-cycle cutoff: the earliest local day whose data survives.
/// `grace = close_day - cycle_start_day + 1` (4 with the defaults);
/// everything older than `cycle_start_on_or_before(today - grace days)`
/// is fair game.
pub fn cutoff_for_cycle(_today: NaiveDate, _cycle_start_day: u32, _close_day: u32) -> NaiveDate {
    unimplemented!()
}

/// A plain rolling-window cutoff, `days` before `today` — the
/// `--days` CLI override. No cycle alignment, but still rail-free once
/// `purge_rows` runs against it.
pub fn cutoff_for_days(_today: NaiveDate, _days: i64) -> NaiveDate {
    unimplemented!()
}

/// Delete every block (and, via cascade, its `block_events` rows) whose
/// local `day` is before `cutoff`, plus every event before `cutoff` that
/// no surviving block references. Rail-free: sync state, edit
/// provenance, pending edits and personal classification make no
/// difference. `dry_run` writes nothing and reports simulated counts.
pub fn purge_rows(_conn: &Connection, _cutoff: NaiveDate, _dry_run: bool) -> Result<PurgeReport> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;

    /// Insert a block with every field the cutoff predicate could ever
    /// key off, aside from `dirty`/`is_personal` (see
    /// [`insert_block_with_flags`]).
    fn insert_block(
        conn: &Connection,
        day: &str,
        tempo_id: Option<&str>,
        estimated_by: Option<&str>,
        exported_at: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds,
                                 tempo_worklog_id, estimated_by, exported_at)
             VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00',
                     1800, ?2, ?3, ?4)",
            params![day, tempo_id, estimated_by, exported_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Insert a block with an explicit `dirty` / `is_personal` flag —
    /// the two columns the old rails never learned about (B8).
    fn insert_block_with_flags(
        conn: &Connection,
        day: &str,
        tempo_id: Option<&str>,
        dirty: i64,
        is_personal: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds,
                                 tempo_worklog_id, dirty, is_personal)
             VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00',
                     1800, ?2, ?3, ?4)",
            params![day, tempo_id, dirty, is_personal],
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

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// B1-B6: the authoritative cutoff table from spec Appendix A,
    /// checked date-by-date at the documented defaults.
    #[test]
    fn b1_through_b6_cutoff_for_cycle_table() {
        let cases: &[(&str, &str)] = &[
            ("2026-07-24", "2026-07-20"),
            ("2026-07-23", "2026-06-20"),
            ("2026-07-05", "2026-06-20"),
            ("2026-07-19", "2026-06-20"),
            ("2026-07-20", "2026-06-20"),
            ("2026-08-19", "2026-07-20"),
            ("2026-08-24", "2026-08-20"),
            ("2026-03-05", "2026-02-20"),
            ("2026-01-02", "2025-12-20"),
        ];
        for (today_str, expected_str) in cases {
            let today = date(today_str);
            let expected = date(expected_str);
            let cutoff = cutoff_for_cycle(today, DEFAULT_CYCLE_START_DAY, DEFAULT_CLOSE_DAY);
            assert_eq!(cutoff, expected, "today={today_str}");
        }
    }

    /// B7: a configured start day above the shortest month's length
    /// clamps instead of producing an invalid date or panicking.
    #[test]
    fn b7_cycle_start_day_31_clamps_within_february() {
        // Non-leap February 2026 has 28 days.
        assert_eq!(effective_start_day(2026, 2, 31), 28);
        let clamped = cycle_start_on_or_before(date("2026-02-28"), 31);
        assert_eq!(clamped, date("2026-02-28"));

        // Leap February 2028 has 29 days.
        assert_eq!(effective_start_day(2028, 2, 31), 29);
        let clamped_leap = cycle_start_on_or_before(date("2028-02-29"), 31);
        assert_eq!(clamped_leap, date("2028-02-29"));
    }

    /// B8: hand-edited, edited-since-sync, never-synced, personal — and
    /// exported-but-unsynced — blocks are ALL deleted once past the
    /// cutoff. No exemption survives the rail-free rewrite.
    #[test]
    fn b8_all_block_classes_deleted_no_exemption() {
        let conn = open_memory().unwrap();
        let old = "2026-02-10";
        insert_block(&conn, old, Some("tempo-1"), Some("manual"), None); // manual
        insert_block_with_flags(&conn, old, Some("tempo-2"), 1, 0); // dirty=1, synced
        insert_block(&conn, old, None, None, None); // tempo_worklog_id NULL
        insert_block(&conn, old, Some(""), None, None); // tempo_worklog_id ''
        insert_block_with_flags(&conn, old, None, 0, 1); // is_personal=1
        insert_block(&conn, old, None, None, Some("2026-02-11T09:00:00.000Z")); // exported, no tempo id

        let cutoff = date("2026-06-20");
        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.blocks_deleted, 6);
        assert_eq!(count(&conn, "blocks"), 0);
    }

    /// B9: a block newer than the cutoff survives along with every
    /// event linked to it — including one that is itself older than the
    /// cutoff (the `NOT IN block_events` guard) — while a true orphan
    /// event past the cutoff is deleted.
    #[test]
    fn b9_recent_block_and_linked_events_survive_true_orphan_deleted() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        let bid = insert_block(&conn, "2026-06-25", Some("tempo-x"), None, None);
        let old_linked = insert_event(&conn, "2026-02-10T09:00:00+00:00", "old-linked");
        let recent_linked = insert_event(&conn, "2026-06-25T09:05:00+00:00", "recent-linked");
        link(&conn, bid, old_linked);
        link(&conn, bid, recent_linked);
        insert_event(&conn, "2026-02-11T09:00:00+00:00", "orphan-old");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "blocks"), 1);
        assert_eq!(count(&conn, "events"), 2);
        assert_eq!(count(&conn, "block_events"), 2);
    }

    /// B10: a dry run reports non-zero counts while every affected
    /// table's row count stays identical.
    #[test]
    fn b10_dry_run_reports_counts_but_changes_nothing() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_block(&conn, "2026-02-10", Some("tempo-3"), None, None);
        insert_block(&conn, "2026-02-11", None, Some("gap"), None);
        insert_event(&conn, "2026-02-10T12:00:00+00:00", "orphan-old");
        insert_event(&conn, "2026-07-01T12:00:00+00:00", "orphan-fresh");

        let before_blocks = count(&conn, "blocks");
        let before_events = count(&conn, "events");

        let report = purge_rows(&conn, cutoff, true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.blocks_deleted, 2);
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "blocks"), before_blocks);
        assert_eq!(count(&conn, "events"), before_events);
    }

    /// B11: `cutoff_for_cycle` and `cutoff_for_days` compute different,
    /// independently-correct values from the same `today`.
    #[test]
    fn b11_cutoff_for_cycle_and_cutoff_for_days_differ() {
        let today = date("2026-07-24");
        let cycle_cutoff = cutoff_for_cycle(today, DEFAULT_CYCLE_START_DAY, DEFAULT_CLOSE_DAY);
        let days_cutoff = cutoff_for_days(today, 90);
        assert_eq!(cycle_cutoff, date("2026-07-20"));
        assert_eq!(days_cutoff, date("2026-04-25"));
        assert_ne!(cycle_cutoff, days_cutoff);
    }

    /// B12: `purge_rows` deletes a manual block under a `--days`-style
    /// cutoff too — the override is equally rail-free.
    #[test]
    fn b12_purge_rows_deletes_manual_block_under_days_style_cutoff() {
        let conn = open_memory().unwrap();
        let today = date("2026-07-24");
        let cutoff = cutoff_for_days(today, 30);
        insert_block(&conn, "2026-05-01", Some("tempo-9"), Some("manual"), None);

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.blocks_deleted, 1);
        assert_eq!(count(&conn, "blocks"), 0);
    }

    /// B37: of three deleted blocks — one synced, one exported, one
    /// with neither marker — exactly the last is never-billed.
    #[test]
    fn b37_blocks_deleted_unbilled_counts_only_the_never_billed_subset() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);
        insert_block(
            &conn,
            "2026-02-11",
            None,
            None,
            Some("2026-02-12T09:00:00.000Z"),
        );
        insert_block(&conn, "2026-02-12", None, None, None);

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.blocks_deleted, 3);
        assert_eq!(report.blocks_deleted_unbilled, 1);
    }

    /// The cutoff is always rendered as a plain ISO `YYYY-MM-DD` — the
    /// CLI's rendering depends on that exact width.
    #[test]
    fn cutoff_date_is_reported_as_iso() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        let report = purge_rows(&conn, cutoff, true).unwrap();
        assert_eq!(report.cutoff_date, "2026-06-20");
        assert_eq!(report.cutoff_date.len(), 10);
    }
}
