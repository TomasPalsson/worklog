//! Retention policy — billing-cycle-aligned, rail-free deletion.
//!
//! The owner bills in cycles that run `cycle_start_day` (default the
//! 20th) through the day before `cycle_start_day` in the following
//! month. Once a cycle's submission window has shut (the second business
//! day after it ends, default the 23rd) nothing can add hours to it any
//! more, so retaining its rows has no value. `cutoff_for_cycle` derives
//! that boundary; `purge_rows` deletes every row strictly older than it
//! from `blocks` (and, by cascade, `block_events`), `events`, and
//! `sessions`, plus every manually-picked (`external = 1`) `jira_tickets`
//! entry no surviving block references any more.
//!
//! Deliberately rail-free: sync state (`tempo_worklog_id`), export state
//! (`exported_at`), edit provenance (`estimated_by`), the pending-edit
//! flag (`dirty`), and personal classification (`is_personal`) make no
//! difference to what gets deleted. The previous version of this module
//! exempted unsynced and hand-edited blocks; that exemption made
//! personal blocks (which can never sync or export) immortal and made
//! dirty blocks (which do carry a `tempo_worklog_id`) *more* likely to
//! be deleted than protected. See spec 002-billing-cycle-pruner §1.1.
//! Recoverability comes from a pre-prune snapshot ([`run`]'s `VACUUM
//! INTO` step), not from exemptions.
//!
//! `blocks_deleted_unbilled` on [`PurgeReport`] exists so that loss is
//! visible: it counts deleted blocks that carried neither a Tempo id nor
//! an `exported_at` marker, i.e. work nobody will ever be paid for.
//!
//! `billing_customers` and `billing_folder_map` are never touched by any
//! cutoff — they are persistent, UI-edited registry tables, not time
//! data (CLAUDE.md).

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection};

/// Default retention window for the `--days` CLI override. Unrelated to
/// the billing cycle; a plain rolling window.
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
    /// Sessions that were (or would be) deleted.
    pub sessions_deleted: i64,
    /// Manually-picked (`external = 1`) ticket cache entries deleted
    /// because no surviving block references them any more.
    /// Collector-owned (`external = 0`) entries are never touched.
    pub tickets_deleted: i64,
    /// Disk space reclaimed, in bytes. Left at the default of `0` by
    /// [`purge_rows`] directly; populated by [`run`] after its post-delete
    /// `VACUUM`.
    pub bytes_freed: i64,
    /// Where the pre-prune snapshot was written. Left at the default of
    /// `None` by [`purge_rows`] directly; populated by [`run`].
    pub snapshot_path: Option<String>,
    /// If true, nothing was actually written to the database.
    pub dry_run: bool,
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
pub fn cutoff_for_cycle(today: NaiveDate, cycle_start_day: u32, close_day: u32) -> NaiveDate {
    let grace = close_day.saturating_sub(cycle_start_day) + 1;
    let anchor = today - chrono::Duration::days(i64::from(grace));
    cycle_start_on_or_before(anchor, cycle_start_day)
}

/// A plain rolling-window cutoff, `days` before `today` — the
/// `--days` CLI override. No cycle alignment, but still rail-free once
/// `purge_rows` runs against it.
pub fn cutoff_for_days(today: NaiveDate, days: i64) -> NaiveDate {
    today - chrono::Duration::days(days)
}

/// Delete every block (and, via cascade, its `block_events` rows) whose
/// local `day` is before `cutoff`, every event before `cutoff` that no
/// surviving block references, every session before `cutoff`, and every
/// manually-picked (`external = 1`) ticket cache entry no surviving
/// block references any more. Rail-free: sync state, edit provenance,
/// pending edits and personal classification make no difference.
/// `dry_run` writes nothing and reports simulated counts that mirror
/// exactly what a real run would delete.
pub fn purge_rows(conn: &Connection, cutoff: NaiveDate, dry_run: bool) -> Result<PurgeReport> {
    let cutoff_iso = cutoff.to_string();
    // The exact UTC instant of local midnight at the cutoff — events and
    // sessions store UTC timestamps, so comparing them against a bare
    // local-date string would skew by the configured offset. `day`, in
    // contrast, is itself a local-date string and compares directly.
    let instant_iso = crate::tz::utc_window_for_local_day(cutoff).0.to_rfc3339();

    // Never-billed count is taken BEFORE any deletion — the rows (and
    // the markers that would prove they were never billed) are gone
    // once the delete runs.
    let blocks_deleted_unbilled: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM blocks
             WHERE day < ?1
               AND (tempo_worklog_id IS NULL OR tempo_worklog_id = '')
               AND (exported_at IS NULL OR exported_at = '')",
            params![cutoff_iso],
            |r| r.get(0),
        )
        .context("counting never-billed blocks past cutoff")?;

    if dry_run {
        let blocks_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE day < ?1",
                params![cutoff_iso],
                |r| r.get(0),
            )
            .context("counting blocks past cutoff")?;
        // Simulates the post-block-delete state: an event only survives
        // if it's linked to a block that would survive (day >= cutoff).
        let events_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events
                 WHERE datetime(started_at) < datetime(?1)
                   AND id NOT IN (
                       SELECT event_id FROM block_events
                        WHERE block_id IN (SELECT id FROM blocks WHERE day >= ?2)
                   )",
                params![instant_iso, cutoff_iso],
                |r| r.get(0),
            )
            .context("counting orphan events past cutoff")?;
        // sessions.started_at is UTC, like events — compare against the
        // same instant, never the local cutoff string.
        let sessions_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE datetime(started_at) < datetime(?1)",
                params![instant_iso],
                |r| r.get(0),
            )
            .context("counting sessions past cutoff")?;
        // Simulates the post-block-delete state for the ticket cache:
        // a manually-picked (external = 1) ticket only survives if some
        // surviving block (day >= cutoff) still references it.
        let tickets_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jira_tickets
                 WHERE external = 1
                   AND key NOT IN (SELECT jira_issue FROM blocks
                                    WHERE jira_issue IS NOT NULL AND day >= ?1)",
                params![cutoff_iso],
                |r| r.get(0),
            )
            .context("counting orphaned external jira tickets past cutoff")?;
        return Ok(PurgeReport {
            cutoff_date: cutoff_iso,
            blocks_deleted,
            blocks_deleted_unbilled,
            events_deleted,
            sessions_deleted,
            tickets_deleted,
            dry_run,
            ..Default::default()
        });
    }

    // Real run — one transaction. block_events cascades away with its
    // parent block (ON DELETE CASCADE in schema.sql, FK enforcement is
    // enabled in db::configure), so once the block delete lands, any
    // event still referenced by block_events belongs to a surviving
    // block; everything else strictly older than the cutoff is an
    // orphan and goes too. `execute()`'s rows-changed return value IS
    // the count — no separate counting query to drift from the delete.
    let tx = conn.unchecked_transaction()?;
    let blocks_deleted = tx
        .execute("DELETE FROM blocks WHERE day < ?1", params![cutoff_iso])
        .context("deleting blocks past cutoff")? as i64;
    let events_deleted = tx
        .execute(
            "DELETE FROM events
             WHERE datetime(started_at) < datetime(?1)
               AND id NOT IN (SELECT event_id FROM block_events)",
            params![instant_iso],
        )
        .context("deleting orphan events past cutoff")? as i64;
    // sessions.started_at is UTC, like events — compare against the same
    // instant. Nothing in worklog has ever deleted a sessions row before
    // this: `reap_stale` only ever sets `ended_at`.
    let sessions_deleted = tx
        .execute(
            "DELETE FROM sessions WHERE datetime(started_at) < datetime(?1)",
            params![instant_iso],
        )
        .context("deleting sessions past cutoff")? as i64;
    // Runs after the blocks delete, so only surviving blocks remain to
    // reference a ticket. Collector-owned entries (external = 0) are
    // never touched — that cache's lifecycle belongs to the collector.
    let tickets_deleted =
        tx.execute(
            "DELETE FROM jira_tickets
             WHERE external = 1
               AND key NOT IN (SELECT jira_issue FROM blocks WHERE jira_issue IS NOT NULL)",
            [],
        )
        .context("deleting orphaned external jira tickets past cutoff")? as i64;
    tx.commit()?;

    Ok(PurgeReport {
        cutoff_date: cutoff_iso,
        blocks_deleted,
        blocks_deleted_unbilled,
        events_deleted,
        sessions_deleted,
        tickets_deleted,
        dry_run,
        ..Default::default()
    })
}

/// Options for [`run`], the orchestrator that wraps [`purge_rows`] with a
/// pre-delete snapshot and post-delete space reclamation.
pub struct PruneOptions<'a> {
    pub cutoff: NaiveDate,
    pub dry_run: bool,
    /// Where to write the pre-prune snapshot. `None` disables snapshotting.
    pub snapshot_to: Option<&'a std::path::Path>,
    /// The live database file, for measuring `bytes_freed`. `None` skips
    /// measurement.
    pub db_path: Option<&'a std::path::Path>,
}

/// Orchestrates a full prune: snapshot, delete, reclaim disk. Order matters
/// (spec 002 §5.4):
///
/// 1. `dry_run` skips both the snapshot and the reclaim step entirely — it
///    delegates straight to [`purge_rows`] and returns its simulated
///    report.
/// 2. Otherwise, the database is snapshotted FIRST via `VACUUM INTO`,
///    before any delete. `VACUUM INTO` refuses to overwrite an existing
///    file, so a stale snapshot from a previous prune is removed first. If
///    the snapshot cannot be written, this returns `Err` without deleting
///    anything — aborting is the specified behaviour.
/// 3. The real transactional delete ([`purge_rows`]) runs.
/// 4. A plain `VACUUM` shrinks the file in place. It cannot run inside a
///    transaction (SQLite restriction), so it must follow the delete's
///    commit. A `VACUUM` failure is not fatal — the deletes already
///    landed — so `bytes_freed` simply stays `0`.
/// 5. `bytes_freed` is the database file's size before step 2 minus its
///    size after step 4, floored at zero, and only computed when
///    `db_path` is `Some`.
pub fn run(conn: &Connection, opts: &PruneOptions) -> Result<PurgeReport> {
    if opts.dry_run {
        return purge_rows(conn, opts.cutoff, true);
    }

    // Measured before the snapshot (step 2) per spec 002 §5.4 — the
    // snapshot itself never touches `db_path`, so this is also the size
    // immediately before the delete transaction.
    let size_before = opts
        .db_path
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len());

    if let Some(target) = opts.snapshot_to {
        // VACUUM INTO refuses to overwrite an existing file — remove any
        // stale snapshot from a previous prune first.
        match std::fs::remove_file(target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("removing stale snapshot at {}", target.display()))
            }
        }
        let target_str = target
            .to_str()
            .with_context(|| format!("snapshot path {} is not valid UTF-8", target.display()))?;
        conn.execute("VACUUM INTO ?1", params![target_str])
            .with_context(|| format!("writing pre-prune snapshot to {}", target.display()))?;
    }

    // The real transactional delete. If the snapshot step above failed,
    // we never reach here — nothing has been deleted.
    let mut report = purge_rows(conn, opts.cutoff, false)?;

    // Cannot run inside a transaction, so it follows purge_rows's commit.
    // Not fatal on failure: the deletes already landed, so bytes_freed
    // just stays at its default of 0.
    if conn.execute_batch("VACUUM").is_ok() {
        if let (Some(before), Some(p)) = (size_before, opts.db_path) {
            if let Ok(meta) = std::fs::metadata(p) {
                report.bytes_freed = before.saturating_sub(meta.len()) as i64;
            }
        }
    }

    report.snapshot_path = opts.snapshot_to.map(|p| p.display().to_string());

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;
    use tempfile::tempdir;

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

    /// Insert a block carrying `filler` in its `description` — used to
    /// bulk up a file-backed database with enough pages for a `VACUUM` to
    /// meaningfully shrink (B21).
    fn insert_bulky_block(conn: &Connection, day: &str, filler: &str) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
             VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00', 1800, ?2)",
            params![day, filler],
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

    /// Insert a block that references a Jira ticket by key (B14/B15) — the
    /// column blocks and jira_tickets join on is `jira_issue` /
    /// `jira_tickets.key`, confirmed against `billing::ticket_summary`.
    fn insert_block_with_ticket(conn: &Connection, day: &str, jira_issue: &str) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES (?1, ?2, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00', 1800)",
            params![day, jira_issue],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Insert a session row (B13). Nothing in worklog has ever deleted a
    /// sessions row before this feature, so there is no existing helper.
    fn insert_session(conn: &Connection, session_id: &str, started_at: &str) -> i64 {
        conn.execute(
            "INSERT INTO sessions (session_id, started_at) VALUES (?1, ?2)",
            params![session_id, started_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Insert a Jira ticket cache row with an explicit `external` flag
    /// (B14/B15/B16).
    fn insert_ticket(conn: &Connection, key: &str, external: i64) {
        conn.execute(
            "INSERT INTO jira_tickets (key, summary, external) VALUES (?1, 'Test ticket', ?2)",
            params![key, external],
        )
        .unwrap();
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

    /// B13: a session before the cutoff and one after — only the older
    /// one is deleted.
    #[test]
    fn b13_session_before_cutoff_deleted_session_after_survives() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_session(&conn, "sess-old", "2026-02-10T09:00:00+00:00");
        insert_session(&conn, "sess-new", "2026-06-25T09:00:00+00:00");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.sessions_deleted, 1);
        assert_eq!(count(&conn, "sessions"), 1);
    }

    /// B14: an `external = 1` ticket whose only referencing block is older
    /// than the cutoff is deleted once that block is gone.
    #[test]
    fn b14_orphaned_external_ticket_deleted() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_ticket(&conn, "EXT-1", 1);
        insert_block_with_ticket(&conn, "2026-02-10", "EXT-1");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.tickets_deleted, 1);
        assert_eq!(count(&conn, "jira_tickets"), 0);
    }

    /// B15: an `external = 1` ticket referenced by a block NEWER than the
    /// cutoff is kept.
    #[test]
    fn b15_external_ticket_referenced_by_surviving_block_kept() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_ticket(&conn, "EXT-2", 1);
        insert_block_with_ticket(&conn, "2026-06-25", "EXT-2");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.tickets_deleted, 0);
        assert_eq!(count(&conn, "jira_tickets"), 1);
    }

    /// B16: an `external = 0` cached ticket, ancient and unreferenced, is
    /// never touched — the collector owns that cache's lifecycle.
    #[test]
    fn b16_cached_external_zero_ticket_never_touched() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_ticket(&conn, "CACHE-1", 0);

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.tickets_deleted, 0);
        assert_eq!(count(&conn, "jira_tickets"), 1);
    }

    /// B38: `billing_customers` and `billing_folder_map` are persistent,
    /// UI-edited registry tables — CLAUDE.md forbids pruning them under
    /// any cutoff, regardless of how old their rows are.
    #[test]
    fn b38_billing_registry_survives_every_prune() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        let ancient = "2020-01-01T00:00:00.000Z";
        conn.execute(
            "INSERT INTO billing_customers (name, aliases, created_at)
             VALUES ('Acme', 'acme,acme-corp', ?1)",
            params![ancient],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO billing_folder_map (folder, customer, verkefni, billable, created_at)
             VALUES ('acme-website', 'Acme', 'ACME-1', 1, ?1)",
            params![ancient],
        )
        .unwrap();

        let before_customers = count(&conn, "billing_customers");
        let before_folder_map = count(&conn, "billing_folder_map");

        purge_rows(&conn, cutoff, false).unwrap();

        assert_eq!(count(&conn, "billing_customers"), before_customers);
        assert_eq!(count(&conn, "billing_folder_map"), before_folder_map);
    }

    /// Plus: a dry run reports the sessions and tickets counts without
    /// changing either table.
    #[test]
    fn dry_run_reports_sessions_and_tickets_without_changing_tables() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_session(&conn, "sess-old", "2026-02-10T09:00:00+00:00");
        insert_session(&conn, "sess-new", "2026-06-25T09:00:00+00:00");
        insert_ticket(&conn, "EXT-1", 1);
        insert_block_with_ticket(&conn, "2026-02-10", "EXT-1");
        insert_ticket(&conn, "CACHE-1", 0);

        let before_sessions = count(&conn, "sessions");
        let before_tickets = count(&conn, "jira_tickets");

        let report = purge_rows(&conn, cutoff, true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.sessions_deleted, 1);
        assert_eq!(report.tickets_deleted, 1);
        assert_eq!(count(&conn, "sessions"), before_sessions);
        assert_eq!(count(&conn, "jira_tickets"), before_tickets);
    }

    /// B17: a deleting prune writes a snapshot that is itself a valid,
    /// openable SQLite database CONTAINING the pre-prune rows — proof of a
    /// real recovery artifact, not an empty placeholder file.
    #[test]
    fn b17_snapshot_is_a_valid_openable_db_containing_pre_prune_rows() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);
        insert_block(&conn, "2026-02-11", None, None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let report = run(&conn, &opts).unwrap();
        assert_eq!(report.blocks_deleted, 2);
        assert!(snapshot_path.is_file());

        // The snapshot is a self-contained, openable db with the rows the
        // real database is about to lose.
        let snap_conn = Connection::open(&snapshot_path).unwrap();
        let blocks: i64 = snap_conn
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(blocks, 2);
    }

    /// B18: a pre-existing (stale) file at the snapshot path is replaced —
    /// `VACUUM INTO` refuses to overwrite, so `run` must remove it first —
    /// and exactly one generation exists afterwards.
    #[test]
    fn b18_pre_existing_snapshot_replaced_exactly_one_generation() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        std::fs::write(&snapshot_path, b"stale snapshot from a previous prune").unwrap();

        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        run(&conn, &opts).unwrap();

        // Replaced with a fresh, valid snapshot — not left as the stale
        // placeholder and not appended to.
        let snap_conn = Connection::open(&snapshot_path).unwrap();
        let blocks: i64 = snap_conn
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(blocks, 1);

        // Exactly one generation: no numbered/backup sibling files.
        let siblings: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("worklog.db.preprune"))
            .collect();
        assert_eq!(
            siblings.len(),
            1,
            "expected exactly one snapshot generation, got {siblings:?}"
        );
    }

    /// B19: an unwritable snapshot target (parent directory does not
    /// exist) makes `run` return `Err` AND leaves every table's row count
    /// unchanged — the most important test in the slice, proving the
    /// abort-before-delete ordering.
    #[test]
    fn b19_unwritable_snapshot_target_aborts_before_any_delete() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);
        insert_session(&conn, "sess-old", "2026-02-10T09:00:00+00:00");
        insert_ticket(&conn, "EXT-1", 1);
        insert_block_with_ticket(&conn, "2026-02-10", "EXT-1");

        let before_blocks = count(&conn, "blocks");
        let before_sessions = count(&conn, "sessions");
        let before_tickets = count(&conn, "jira_tickets");

        let snapshot_path = tmp
            .path()
            .join("does-not-exist")
            .join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let result = run(&conn, &opts);
        assert!(result.is_err());
        assert_eq!(count(&conn, "blocks"), before_blocks);
        assert_eq!(count(&conn, "sessions"), before_sessions);
        assert_eq!(count(&conn, "jira_tickets"), before_tickets);
    }

    /// B20: a dry run writes no snapshot file at all.
    #[test]
    fn b20_dry_run_writes_no_snapshot() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: true,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let report = run(&conn, &opts).unwrap();
        assert!(report.dry_run);
        assert!(report.snapshot_path.is_none());
        assert!(!snapshot_path.exists());
    }

    /// B21: after a prune that deleted a meaningful number of rows,
    /// `bytes_freed` is populated and non-negative, and the file is no
    /// larger than before the prune. Bulked with enough filler data to
    /// give `VACUUM` real pages to reclaim.
    #[test]
    fn b21_bytes_freed_populated_and_file_not_larger_after_prune() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();

        let filler = "x".repeat(2000);
        for i in 0..300 {
            insert_bulky_block(&conn, "2026-02-10", &format!("{filler}-{i}"));
        }

        let size_before_delete = std::fs::metadata(&db_path).unwrap().len();

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let report = run(&conn, &opts).unwrap();

        assert_eq!(report.blocks_deleted, 300);
        assert!(report.bytes_freed >= 0);
        let size_after = std::fs::metadata(&db_path).unwrap().len();
        assert!(
            size_after <= size_before_delete,
            "file grew: before={size_before_delete} after={size_after}"
        );
    }
}
