//! Compression run — blocks are kept, their raw rows are not.
//!
//! Everything older than the horizon (`block_digest::horizon`, 90 days)
//! is squeezed into one card per block, then the raw rows the card
//! replaces are deleted: events, sessions, session pins, the transcript
//! cache, and orphaned manually-picked (`external = 1`) `jira_tickets`
//! entries. `blocks` are never deleted, so `tempo_worklog_id`,
//! `exported_at`, `estimated_by`, `dirty` and `is_personal` are untouched
//! by construction. [`purge_rows`] does the work in one transaction —
//! a card that fails rolls the whole run back — and [`run`] snapshots the
//! database first (`VACUUM INTO`), so recoverability comes from that
//! snapshot.
//!
//! `billing_customers` and `billing_folder_map` are never touched by any
//! cutoff — they are persistent, UI-edited registry tables, not time
//! data (CLAUDE.md).

use anyhow::{Context, Result};
use chrono::NaiveDate;

use crate::block_digest;
use crate::collectors::claude_transcript_cache;
use rusqlite::{params, Connection, OptionalExtension};

/// What the purge did (or would have done, if `dry_run`). Deserialize is
/// needed alongside Serialize so a report persisted to `meta` (see
/// [`LAST_REPORT_KEY`], [`last_prune`]) round-trips.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PurgeReport {
    /// ISO `YYYY-MM-DD` — the horizon; anything before it is compressed.
    pub cutoff_date: String,
    /// Always 0: blocks are never deleted. Kept so persisted reports
    /// from before the compression run still deserialize.
    pub blocks_deleted: i64,
    /// Blocks that received (or would receive) a card this run.
    #[serde(default)]
    pub blocks_carded: i64,
    /// Total size in bytes of the card JSON written (or that would be).
    #[serde(default)]
    pub card_bytes: i64,
    /// Median size in bytes of the cards written this run (0 when none).
    #[serde(default)]
    pub card_bytes_median: i64,
    /// Largest card written this run (0 when none).
    #[serde(default)]
    pub card_bytes_max: i64,
    /// Transcript-cache rows deleted (or that would be).
    #[serde(default)]
    pub cache_rows_deleted: i64,
    /// Always 0, like `blocks_deleted`.
    pub blocks_deleted_unbilled: i64,
    /// Events (orphan or cascaded) that were (or would be) deleted.
    pub events_deleted: i64,
    /// Sessions that were (or would be) deleted.
    pub sessions_deleted: i64,
    /// Manually-picked (`external = 1`) ticket cache entries deleted
    /// because no block references them.
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
/// day before `cycle_start_day` in the following month. Configurable via
/// the environment or `.env` file — see [`configured_cycle_start_day`].
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
/// `cycle_start_on_or_before(today - grace days)` is the start of the
/// newest closed-or-open cycle, and the cycle before it is kept too as
/// evidence for the last invoice — so only data older than that
/// previous cycle's start is fair game.
pub fn cutoff_for_cycle(today: NaiveDate, cycle_start_day: u32, close_day: u32) -> NaiveDate {
    let grace = close_day.saturating_sub(cycle_start_day) + 1;
    let anchor = today - chrono::Duration::days(i64::from(grace));
    let current = cycle_start_on_or_before(anchor, cycle_start_day);
    cycle_start_on_or_before(current - chrono::Duration::days(1), cycle_start_day)
}

/// A plain rolling-window cutoff, `days` before `today` — the
/// `--days` CLI override. No cycle alignment, but still rail-free once
/// `purge_rows` runs against it.
pub fn cutoff_for_days(today: NaiveDate, days: i64) -> NaiveDate {
    today - chrono::Duration::days(days)
}

/// Predicate on `events e`: past the horizon instant and not linked to a
/// block that survives (`day >= horizon`). `?1` is the horizon instant,
/// `?2` the horizon date, `?3` the index pre-filter bound.
const EVENT_IS_EXPIRED: &str = "e.started_at < ?3
     AND datetime(e.started_at) < datetime(?1)
     AND e.id NOT IN (SELECT event_id FROM block_events
                       WHERE block_id IN (SELECT id FROM blocks WHERE day >= ?2))";

/// Size totals of the cards a run builds.
#[derive(Default)]
struct CardStats {
    carded: i64,
    bytes: i64,
    median: i64,
    max: i64,
}

impl CardStats {
    fn of(mut sizes: Vec<i64>) -> Self {
        sizes.sort_unstable();
        CardStats {
            carded: sizes.len() as i64,
            bytes: sizes.iter().sum(),
            median: sizes.get(sizes.len() / 2).copied().unwrap_or(0),
            max: sizes.last().copied().unwrap_or(0),
        }
    }
}

fn transcript_cache_rows(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM transcript_file_cache", [], |r| {
        r.get(0)
    })
    .context("counting transcript cache rows")
}

fn pragma_i64(conn: &Connection, name: &str) -> Result<i64> {
    conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
        .with_context(|| format!("reading PRAGMA {name}"))
}

/// Builds and writes the card of every block before `horizon` that has none.
fn card_old_blocks(conn: &Connection, horizon_iso: &str) -> Result<CardStats> {
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT id FROM blocks
             WHERE day < ?1 AND id NOT IN (SELECT block_id FROM block_digest)
             ORDER BY id",
        )?
        .query_map(params![horizon_iso], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()
        .context("listing blocks past the horizon without a card")?;
    let mut sizes = Vec::with_capacity(ids.len());
    for &id in &ids {
        let card = block_digest::build_digest(conn, id)
            .with_context(|| format!("building the card for block {id}"))?;
        sizes.push(serde_json::to_string(&card)?.len() as i64);
        block_digest::write_digest(conn, id, &card)
            .with_context(|| format!("writing the card for block {id}"))?;
    }
    Ok(CardStats::of(sizes))
}

/// Compress everything before `horizon`: give every block on an older day
/// a card, then delete the raw rows that card replaces — events before the
/// horizon instant not linked to a surviving block (their `block_events`
/// links cascade), sessions with no event left, orphaned session pins, the
/// transcript cache, and orphaned manually-picked (`external = 1`) ticket
/// cache entries. Blocks are never deleted, whatever their sync, export,
/// edit or personal state. One transaction: a card that cannot be built
/// or written rolls the whole run back. `dry_run` does all of it, reports
/// what it did (with `bytes_freed` estimated from the pages released) and
/// rolls back.
pub fn purge_rows(conn: &Connection, horizon: NaiveDate, dry_run: bool) -> Result<PurgeReport> {
    let horizon_iso = horizon.to_string();
    // The exact UTC instant of local midnight at the horizon — events and
    // sessions store UTC timestamps, so comparing them against a bare
    // local-date string would skew by the configured offset. `day`, in
    // contrast, is itself a local-date string and compares directly.
    let instant_iso = crate::tz::utc_window_for_local_day(horizon).0.to_rfc3339();
    // Index-usable pre-filter for the `datetime(started_at) < datetime(?1)`
    // predicates below: that expression can't use idx_events_started/
    // idx_sessions_started because SQLite must call datetime() on every
    // row before it can compare. `started_at < date_bound_iso` is a plain
    // string comparison the index CAN drive, ANDed in front of the exact
    // predicate as a superset filter — it only has to be provably true for
    // every row the exact predicate matches, never exact itself.
    //
    // Proof: a row matches the exact predicate only if its UTC instant is
    // < the instant named by `instant_iso`, which is local midnight at
    // `horizon` — never later than 23:59:59 UTC on `horizon`'s own
    // calendar date (`utc_window_for_local_day` cannot shift local
    // midnight past the end of `horizon`'s UTC day). So a matching row's
    // UTC-instant date is <= `horizon`. `started_at` strings carry an
    // offset of at most ±14:00 (well under 24h), so the *literal* calendar
    // date written in the string can differ from the UTC-instant date by
    // at most one day, giving a literal date <= `horizon + 1 day`.
    // `date_bound_iso` below is `horizon + 2 days` formatted as a bare
    // `YYYY-MM-DD` (10 chars, no time part): its date is strictly greater
    // than `horizon + 1 day`, so the first 10 characters of any matching
    // row's `started_at` compare less than it — and once an earlier
    // character differs, whatever follows (a 'T'/space plus time and
    // offset) can't change the comparison back.
    let date_bound_iso = (horizon + chrono::Duration::days(2)).to_string();
    let bounds = params![instant_iso, horizon_iso, date_bound_iso];

    // One transaction for both modes, cards first so nothing is deleted that
    // a card has not captured. Dropping `tx` on any error rolls it all back,
    // and a dry run rolls back on purpose — so its counts are the real
    // run's. `execute()`'s rows-changed return value IS the count.
    let tx = conn.unchecked_transaction()?;
    let cards = card_old_blocks(&tx, &horizon_iso)?;
    let freelist_before = pragma_i64(&tx, "freelist_count")?;
    let events_deleted = tx
        .execute(
            &format!("DELETE FROM events WHERE id IN (SELECT e.id FROM events e WHERE {EVENT_IS_EXPIRED})"),
            bounds,
        )
        .context("deleting expired events")? as i64;
    // A branch name gets reused months later and would otherwise silently
    // inherit whatever customer it was pinned to last time (`pin_for_branch`
    // has no age bound of its own) — a pin lives exactly as long as its
    // session's events do. Runs after the events delete just above so this
    // sees the post-delete state. `from_at` is bound by the same horizon as
    // every sibling delete in this pass — without it, a fresh pin whose
    // session has no event row yet (written moments before the recorder's
    // first event, or a recorder that then failed) would be deleted on ANY
    // run, however recent its horizon.
    tx.execute(
        "DELETE FROM session_pins
         WHERE from_at < ?3 AND datetime(from_at) < datetime(?1)
           AND session_id NOT IN (SELECT session_id FROM events WHERE session_id IS NOT NULL)",
        bounds,
    )
    .context("deleting orphaned session pins past the horizon")?;
    let sessions_deleted = tx
        .execute(
            "DELETE FROM sessions
             WHERE started_at < ?3 AND datetime(started_at) < datetime(?1)
               AND session_id NOT IN (SELECT session_id FROM events WHERE session_id IS NOT NULL)",
            bounds,
        )
        .context("deleting sessions past the horizon")? as i64;
    // Collector-owned entries (external = 0) are never touched — that
    // cache's lifecycle belongs to the collector.
    let tickets_deleted = tx
        .execute(
            "DELETE FROM jira_tickets
             WHERE external = 1
               AND key NOT IN (SELECT jira_issue FROM blocks WHERE jira_issue IS NOT NULL)",
            [],
        )
        .context("deleting orphaned external jira tickets")? as i64;
    // A cached transcript fingerprint doesn't know the events delete above
    // just ran — without this, an untouched file stays skipped forever and
    // its deleted rows never come back on a later tick.
    let cache_rows_deleted = transcript_cache_rows(&tx)?;
    claude_transcript_cache::clear_after_events_delete(&tx)?;
    let freed_pages = pragma_i64(&tx, "freelist_count")? - freelist_before;
    let bytes_freed = if dry_run {
        tx.rollback()?;
        freed_pages.max(0) * pragma_i64(conn, "page_size")?
    } else {
        tx.commit()?;
        0
    };

    Ok(PurgeReport {
        cutoff_date: horizon_iso,
        blocks_carded: cards.carded,
        card_bytes: cards.bytes,
        card_bytes_median: cards.median,
        card_bytes_max: cards.max,
        events_deleted,
        sessions_deleted,
        tickets_deleted,
        cache_rows_deleted,
        bytes_freed,
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
    // Belt and braces, test builds only. `$WORKLOG_HOME` is process-global
    // and worklog-core's tests guard it with several mutexes that do not
    // lock against each other, so a test that resolves paths from the
    // environment can be raced by another test clearing that variable and
    // end up aimed at the owner's real database. That is not hypothetical:
    // it happened, and it deleted 783 blocks and ~392k events before this
    // guard existed. Production builds do not compile this.
    #[cfg(test)]
    {
        if let Some(p) = opts.db_path {
            if let Some(home) = dirs::home_dir() {
                let real = home.join(".local/share/worklog");
                if p.starts_with(&real) {
                    anyhow::bail!(
                        "refusing to prune the real data directory from a test: {}",
                        p.display()
                    );
                }
            }
        }
    }

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

/// Key under which [`prune_if_due`] records the cutoff it last
/// successfully pruned to, in the `meta` key/value table.
pub const LATCH_KEY: &str = "last_prune_cutoff";

/// Key under which [`prune_if_due`] persists the JSON-serialised
/// [`PurgeReport`] of the most recent successful automatic prune, in the
/// `meta` key/value table (spec 002 FR-018, FR-024 / B32, B34, B40).
pub const LAST_REPORT_KEY: &str = "last_prune_report";

/// Key under which [`prune_if_due`] persists the RFC3339 UTC timestamp of
/// the most recent successful automatic prune, in the `meta` key/value
/// table (spec 002 FR-018, FR-024 / B32, B34, B40).
pub const LAST_RUN_KEY: &str = "last_prune_at";

/// The most recently persisted automatic-prune outcome, as read back by
/// [`last_prune`] — the report itself plus when it ran.
#[derive(Debug, Clone)]
pub struct LastPrune {
    pub report: PurgeReport,
    pub ran_at: String,
}

/// Read back the last automatic prune's report and timestamp, persisted
/// by [`prune_if_due`] under [`LAST_REPORT_KEY`] / [`LAST_RUN_KEY`].
/// `None` when either key is absent — e.g. no automatic prune has ever
/// run (spec 002 FR-024 / B34, B35, B40).
pub fn last_prune(conn: &Connection) -> Result<Option<LastPrune>> {
    let report_json = meta_get(conn, LAST_REPORT_KEY)?;
    let ran_at = meta_get(conn, LAST_RUN_KEY)?;
    match (report_json, ran_at) {
        (Some(report_json), Some(ran_at)) => {
            let report: PurgeReport =
                serde_json::from_str(&report_json).context("deserialising last prune report")?;
            Ok(Some(LastPrune { report, ran_at }))
        }
        _ => Ok(None),
    }
}

/// Read a value from the `meta` table. `None` when `key` is absent.
pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .optional()
    .context("reading meta")
}

/// Insert or replace a value in the `meta` table (upsert on `key`).
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .context("writing meta")?;
    Ok(())
}

/// The daemon's due-check (spec 002 FR-009, Journey 1): run [`run`] only
/// if `opts.cutoff` differs from the recorded [`LATCH_KEY`] latch.
/// Returns `Ok(None)` — having done nothing at all — when the latch
/// already matches; the not-due path writes nothing at all, not even the
/// last-report/timestamp keys (spec 002 §5.6 / B33). Otherwise delegates
/// to [`run`] and, only on success, records the new cutoff as the latch
/// plus the JSON-serialised report ([`LAST_REPORT_KEY`]) and an RFC3339
/// UTC timestamp ([`LAST_RUN_KEY`]) — both readable back via
/// [`last_prune`] (spec 002 FR-018, FR-024 / B32). A failure propagates
/// with the latch and both keys left untouched so the next check
/// retries the same work.
pub fn prune_if_due(conn: &Connection, opts: &PruneOptions) -> Result<Option<PurgeReport>> {
    let cutoff_iso = opts.cutoff.to_string();
    if meta_get(conn, LATCH_KEY)?.as_deref() == Some(cutoff_iso.as_str()) {
        return Ok(None);
    }
    let report = run(conn, opts)?;
    meta_set(conn, LATCH_KEY, &cutoff_iso)?;
    let report_json = serde_json::to_string(&report).context("serialising last prune report")?;
    meta_set(conn, LAST_REPORT_KEY, &report_json)?;
    meta_set(conn, LAST_RUN_KEY, &chrono::Utc::now().to_rfc3339())?;
    Ok(Some(report))
}

/// Whether automatic pruning is enabled. Reads `WORKLOG_PRUNE_ENABLED`
/// from the process env, falling back to the persisted `.env` file,
/// defaulting to `true`. Mirrors `tz::configured_tz`'s env-then-file
/// precedence, including its `#[cfg(not(test))]` guard on the file
/// fallback so tests stay hermetic. `"0"`/`"false"`/`"no"`/`"off"`
/// (case-insensitive) disable; anything else (including unset) leaves
/// pruning enabled.
pub fn pruning_enabled() -> bool {
    let raw: Option<String> = match std::env::var("WORKLOG_PRUNE_ENABLED") {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => {
            #[cfg(not(test))]
            {
                crate::envfile::read("WORKLOG_PRUNE_ENABLED")
            }
            #[cfg(test)]
            {
                None
            }
        }
    };
    !matches!(
        raw.as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0") | Some("false") | Some("no") | Some("off")
    )
}

/// Whether `day` is legal for either pruner cycle-day setting
/// (`cycle_start_day` or `close_day`): the plain calendar range
/// `1..=31`. [`configured_cycle_start_day`] and [`configured_close_day`]
/// silently fall back to their defaults (with a `tracing::warn!`) for
/// anything outside this range; `daemon::post_settings` uses it to 400 a
/// bad write before persisting it (spec 002 FR-017 / AC-018).
pub fn is_valid_cycle_day(day: u32) -> bool {
    (1..=31).contains(&day)
}

/// Shared resolution for both pruner-cycle-day settings: process env
/// wins, then the persisted `.env` file, then `default` — mirroring
/// `tz::configured_tz`'s precedence exactly, including its
/// `#[cfg(not(test))]` guard on the file fallback so unit tests stay
/// hermetic regardless of the dev's real `.env`. An unparseable or
/// out-of-range stored value (from either source) falls back to
/// `default` and emits a `tracing::warn!` naming the setting and the
/// fallback, mirroring `tz::day_offset`'s handling of a bad
/// `$WORKLOG_TZ` rather than failing silently (spec 002 §5.4).
fn configured_cycle_day(env_key: &str, default: u32) -> u32 {
    let raw: Option<String> = match std::env::var(env_key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => {
            #[cfg(not(test))]
            {
                crate::envfile::read(env_key)
            }
            #[cfg(test)]
            {
                None
            }
        }
    };
    match raw {
        None => default,
        Some(s) => match s.trim().parse::<u32>() {
            Ok(day) if is_valid_cycle_day(day) => day,
            _ => {
                tracing::warn!(
                    "{env_key}={s:?} is not a valid day 1..=31. \
                     Falling back to {default}."
                );
                default
            }
        },
    }
}

/// Day-of-month the billing cycle starts (`WORKLOG_BILLING_CYCLE_START_DAY`).
/// See [`configured_cycle_day`] for the resolution and fallback rules.
/// Defaults to [`DEFAULT_CYCLE_START_DAY`].
pub fn configured_cycle_start_day() -> u32 {
    configured_cycle_day("WORKLOG_BILLING_CYCLE_START_DAY", DEFAULT_CYCLE_START_DAY)
}

/// Last day-of-month the just-closed cycle can still take hours
/// (`WORKLOG_BILLING_CLOSE_DAY`). Same resolution and fallback rules as
/// [`configured_cycle_start_day`]; defaults to [`DEFAULT_CLOSE_DAY`].
pub fn configured_close_day() -> u32 {
    configured_cycle_day("WORKLOG_BILLING_CLOSE_DAY", DEFAULT_CLOSE_DAY)
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

    /// Insert an orphan event whose title is `filler` — used to bulk up a
    /// file-backed database with enough pages for a `VACUUM` to
    /// meaningfully shrink (B21).
    fn insert_bulky_event(conn: &Connection, started_at: &str, source_id: &str, filler: &str) {
        repo::upsert_event(
            conn,
            &Event::minimal("github_commit", source_id, started_at, filler),
        )
        .unwrap();
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

    /// Insert an orphan (unlinked) event carrying `session_id` — the
    /// `session_pins` purge test's stand-in for a session's activity.
    fn insert_event_with_session(
        conn: &Connection,
        started_at: &str,
        source_id: &str,
        session_id: &str,
    ) -> i64 {
        let mut e = Event::minimal("github_commit", source_id, started_at, "commit");
        e.session_id = Some(session_id.to_string());
        repo::upsert_event(conn, &e).unwrap()
    }

    /// Insert a `session_pins` row (Finding D). Column values beyond
    /// `session_id`/`from_at` are irrelevant to the purge predicate, so
    /// they're fixed placeholders.
    fn insert_session_pin(conn: &Connection, session_id: &str, from_at: &str) {
        conn.execute(
            "INSERT INTO session_pins (session_id, customer, from_at, folder, branch, source)
             VALUES (?1, 'Acme', ?2, 'acme-website', NULL, 'claude')",
            params![session_id, from_at],
        )
        .unwrap();
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

    fn blocks_snapshot(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT id || '|' || day || '|' || started_at || '|' || ended_at || '|' ||
                        duration_seconds || '|' || is_personal || '|' ||
                        COALESCE(tempo_worklog_id, '~') || '|' || COALESCE(exported_at, '~')
                 FROM blocks ORDER BY id",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    /// FR-01/FR-02: every old block gets a card, no block row changes, and
    /// the report's byte total is the card JSON actually stored.
    #[test]
    fn compress_cards_every_old_block_and_leaves_block_rows_byte_identical() {
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);
        insert_block(&conn, "2026-02-11", None, None, None);
        insert_block_with_flags(&conn, "2026-02-12", None, 0, 1);
        let recent = insert_block(&conn, "2026-06-25", None, None, None);
        let before = blocks_snapshot(&conn);

        let report = purge_rows(&conn, date("2026-06-20"), false).unwrap();

        assert_eq!(blocks_snapshot(&conn), before);
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 3);
        assert_eq!(count(&conn, "block_digest"), 3);
        let has_recent_card: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM block_digest WHERE block_id = ?1)",
                [recent],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has_recent_card);
        let stored_bytes: i64 = conn
            .query_row("SELECT SUM(LENGTH(json)) FROM block_digest", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(report.card_bytes, stored_bytes);
    }

    /// FR-03: an event linked only to a carded block goes with its link
    /// row, while the block and its card stay; the card holds the event
    /// count that was computed before the delete.
    #[test]
    fn compress_deletes_old_events_and_links_but_keeps_block_and_card() {
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn, "2026-02-10", None, None, None);
        let eid = insert_event(&conn, "2026-02-10T09:05:00+00:00", "old-linked");
        link(&conn, bid, eid);

        let report = purge_rows(&conn, date("2026-06-20"), false).unwrap();

        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "events"), 0);
        assert_eq!(count(&conn, "block_events"), 0);
        assert_eq!(count(&conn, "blocks"), 1);
        let card = crate::block_digest::digest_for_block(&conn, bid)
            .unwrap()
            .expect("card must exist");
        assert_eq!(card.event_count, 1);
    }

    /// FR-03: a session that still has a surviving event is kept.
    #[test]
    fn compress_keeps_session_that_still_has_an_event() {
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn, "2026-06-25", None, None, None);
        insert_session(&conn, "sess-kept", "2026-02-10T09:00:00+00:00");
        let old = insert_event_with_session(&conn, "2026-02-10T09:00:00+00:00", "e1", "sess-kept");
        link(&conn, bid, old);
        insert_session(&conn, "sess-gone", "2026-02-10T09:00:00+00:00");

        let report = purge_rows(&conn, date("2026-06-20"), false).unwrap();

        assert_eq!(report.events_deleted, 0);
        assert_eq!(report.sessions_deleted, 1);
        let left: String = conn
            .query_row("SELECT session_id FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, "sess-kept");
    }

    /// Spec section 3 error path: a card that fails to write rolls the
    /// whole run back — zero cards, zero deletes.
    #[test]
    fn compress_card_failure_rolls_back_cards_and_deletes() {
        let conn = open_memory().unwrap();
        let first = insert_block(&conn, "2026-02-10", None, None, None);
        let second = insert_block(&conn, "2026-02-11", None, None, None);
        let eid = insert_event(&conn, "2026-02-10T09:05:00+00:00", "old");
        link(&conn, first, eid);
        conn.execute_batch(&format!(
            "CREATE TRIGGER fail_card BEFORE INSERT ON block_digest
             WHEN NEW.block_id = {second}
             BEGIN SELECT RAISE(ABORT, 'card write failed'); END;"
        ))
        .unwrap();

        let result = purge_rows(&conn, date("2026-06-20"), false);

        assert!(result.is_err());
        assert_eq!(count(&conn, "block_digest"), 0);
        assert_eq!(count(&conn, "events"), 1);
        assert_eq!(count(&conn, "block_events"), 1);
        assert_eq!(count(&conn, "blocks"), 2);
    }

    /// A run over already-carded blocks writes no second card.
    #[test]
    fn compress_second_run_cards_nothing_new() {
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", None, None, None);
        purge_rows(&conn, date("2026-06-20"), false).unwrap();

        let again = purge_rows(&conn, date("2026-06-20"), false).unwrap();

        assert_eq!(again.blocks_carded, 0);
        assert_eq!(again.card_bytes, 0);
        assert_eq!(count(&conn, "block_digest"), 1);
    }

    /// FR-15: a dry run reports what a real run then does, and writes
    /// nothing itself.
    #[test]
    fn compress_dry_run_report_matches_the_real_run() {
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn, "2026-02-10", None, None, None);
        let eid = insert_event_with_session(&conn, "2026-02-10T09:05:00+00:00", "e", "s1");
        link(&conn, bid, eid);
        insert_session(&conn, "s1", "2026-02-10T09:00:00+00:00");
        let cutoff = date("2026-06-20");

        let dry = purge_rows(&conn, cutoff, true).unwrap();
        assert_eq!(count(&conn, "block_digest"), 0);
        assert_eq!(count(&conn, "events"), 1);
        let real = purge_rows(&conn, cutoff, false).unwrap();

        assert!(dry.card_bytes > 0);
        assert_eq!(dry.blocks_carded, real.blocks_carded);
        assert_eq!(dry.card_bytes, real.card_bytes);
        assert_eq!(dry.events_deleted, real.events_deleted);
        assert_eq!(dry.sessions_deleted, real.sessions_deleted);
    }

    /// B7: a dry run estimates the bytes a real run frees from the pages its
    /// deletes release, then rolls everything back — no table changes.
    #[test]
    fn compress_dry_run_estimates_bytes_freed() {
        let conn = open_memory().unwrap();
        insert_block(&conn, "2026-02-10", None, None, None);
        let filler = "x".repeat(2000);
        for i in 0..200 {
            let id = format!("old-{i}");
            let s = if i == 0 { "s1" } else { "s2" };
            let eid = insert_event_with_session(&conn, "2026-02-10T09:05:00+00:00", &id, s);
            conn.execute(
                "UPDATE events SET title = ?1 WHERE id = ?2",
                params![filler, eid],
            )
            .unwrap();
        }
        insert_session(&conn, "s1", "2026-02-10T09:00:00+00:00");
        insert_session_pin(&conn, "s1", "2026-02-10T09:00:00+00:00");
        conn.execute(
            "INSERT INTO transcript_file_cache
                 (path, since_ts, until_ts, size, mtime_ns, claimed_uuids_json)
             VALUES ('/t.jsonl', 0, 1, 1, 1, '[]')",
            [],
        )
        .unwrap();
        let tables = [
            "blocks",
            "events",
            "sessions",
            "block_digest",
            "session_pins",
            "transcript_file_cache",
            "block_events",
        ];
        let before: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
        let blocks_before = blocks_snapshot(&conn);

        let dry = purge_rows(&conn, date("2026-06-20"), true).unwrap();

        assert!(dry.dry_run);
        assert_eq!(dry.events_deleted, 200);
        assert!(dry.bytes_freed > 0, "bytes_freed was {}", dry.bytes_freed);
        let after: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
        assert_eq!(after, before);
        assert_eq!(blocks_snapshot(&conn), blocks_before);
        assert!(conn.is_autocommit());
    }

    /// Spec section 3 error path: an unwritable snapshot leaves zero cards.
    #[test]
    fn compress_unwritable_snapshot_leaves_zero_cards() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        insert_block(&conn, "2026-02-10", None, None, None);
        let unwritable = tmp.path().join("nope").join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(unwritable.as_path()),
            db_path: Some(db_path.as_path()),
        };

        assert!(run(&conn, &opts).is_err());
        assert_eq!(count(&conn, "block_digest"), 0);
    }

    /// FR-15: the report carries the median and max card size, and the
    /// number of transcript-cache rows the run clears.
    #[test]
    fn compress_report_has_card_median_max_and_cache_rows() {
        let conn = open_memory().unwrap();
        let small = insert_block(&conn, "2026-02-10", None, None, None);
        let big = insert_block(&conn, "2026-02-11", None, None, None);
        let third = insert_block(&conn, "2026-02-12", None, None, None);
        for i in 0..5 {
            let e = insert_event(
                &conn,
                &format!("2026-02-11T09:0{i}:00+00:00"),
                &format!("e{i}"),
            );
            link(&conn, big, e);
        }
        let _ = (small, third);
        for path in ["/a", "/b"] {
            conn.execute(
                "INSERT INTO transcript_file_cache
                 (path, since_ts, until_ts, size, mtime_ns, claimed_uuids_json)
                 VALUES (?1, 0, 1, 1, 1, '[]')",
                [path],
            )
            .unwrap();
        }
        let dry = purge_rows(&conn, date("2026-06-20"), true).unwrap();
        assert_eq!(dry.cache_rows_deleted, 2);
        assert_eq!(count(&conn, "transcript_file_cache"), 2);

        let report = purge_rows(&conn, date("2026-06-20"), false).unwrap();

        let mut sizes: Vec<i64> = conn
            .prepare("SELECT LENGTH(json) FROM block_digest ORDER BY LENGTH(json)")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(sizes.len(), 3);
        assert_eq!(report.card_bytes_max, sizes.pop().unwrap());
        assert_eq!(report.card_bytes_median, sizes[1]);
        assert_eq!(report.cache_rows_deleted, 2);
        assert_eq!(count(&conn, "transcript_file_cache"), 0);
        assert_eq!(dry.card_bytes_max, report.card_bytes_max);
    }

    /// B7: a block on a surviving day keeps an event that started before
    /// the horizon instant (local midnight in a non-UTC zone); an
    /// unlinked event at the same instant goes.
    #[test]
    fn compress_block_spanning_midnight_keeps_its_events() {
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn, "2026-06-20", None, None, None);
        let linked = insert_event(&conn, "2026-06-20T03:00:00+00:00", "spanning");
        link(&conn, bid, linked);
        insert_event(&conn, "2026-06-20T03:00:00+00:00", "unlinked");

        let report = purge_rows(&conn, date("2026-06-20"), false);
        std::env::remove_var("WORKLOG_TZ");
        let report = report.unwrap();

        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "events"), 1);
        assert_eq!(count(&conn, "block_events"), 1);
        assert_eq!(count(&conn, "block_digest"), 0);
    }

    /// B8/FR-16: with default cycle settings and blocks aged 30 to 89
    /// days, nothing is carded and nothing is deleted.
    #[test]
    fn compress_default_cycle_settings_delete_no_block() {
        let conn = open_memory().unwrap();
        let today = chrono::Utc::now().date_naive();
        for age in [30, 45, 60, 89] {
            let day = (today - chrono::Duration::days(age)).to_string();
            let bid = insert_block(&conn, &day, None, None, None);
            let e = insert_event(
                &conn,
                &format!("{day}T09:05:00+00:00"),
                &format!("age-{age}"),
            );
            link(&conn, bid, e);
        }
        let opts = PruneOptions {
            cutoff: crate::block_digest::horizon(today),
            dry_run: false,
            snapshot_to: None,
            db_path: None,
        };

        let report = prune_if_due(&conn, &opts).unwrap().unwrap();

        assert_eq!(report.blocks_carded, 0);
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(count(&conn, "blocks"), 4);
        assert_eq!(count(&conn, "block_digest"), 0);
        assert_eq!(count(&conn, "events"), 4);
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

    /// B1-B6: the spec Appendix A cutoff table, shifted one cycle back
    /// because the just-closed cycle is retained as evidence, checked
    /// date-by-date at the documented defaults.
    #[test]
    fn b1_through_b6_cutoff_for_cycle_table() {
        let cases: &[(&str, &str)] = &[
            ("2026-07-24", "2026-06-20"),
            ("2026-07-23", "2026-05-20"),
            ("2026-07-05", "2026-05-20"),
            ("2026-07-19", "2026-05-20"),
            ("2026-07-20", "2026-05-20"),
            ("2026-08-19", "2026-06-20"),
            ("2026-08-24", "2026-07-20"),
            ("2026-03-05", "2026-01-20"),
            ("2026-01-02", "2025-11-20"),
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
    /// exported-but-unsynced — blocks are ALL carded once past the
    /// horizon, and none is deleted.
    #[test]
    fn b8_all_block_classes_carded_none_deleted() {
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
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 6);
        assert_eq!(count(&conn, "blocks"), 6);
        assert_eq!(count(&conn, "block_digest"), 6);
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
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 2);
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "blocks"), before_blocks);
        assert_eq!(count(&conn, "events"), before_events);
        assert_eq!(count(&conn, "block_digest"), 0);
    }

    /// B11: `cutoff_for_cycle` and `cutoff_for_days` compute different,
    /// independently-correct values from the same `today`.
    #[test]
    fn b11_cutoff_for_cycle_and_cutoff_for_days_differ() {
        let today = date("2026-07-24");
        let cycle_cutoff = cutoff_for_cycle(today, DEFAULT_CYCLE_START_DAY, DEFAULT_CLOSE_DAY);
        let days_cutoff = cutoff_for_days(today, 90);
        assert_eq!(cycle_cutoff, date("2026-06-20"));
        assert_eq!(days_cutoff, date("2026-04-25"));
        assert_ne!(cycle_cutoff, days_cutoff);
    }

    /// B12: `purge_rows` cards a manual block under a `--days`-style
    /// cutoff too and never deletes it.
    #[test]
    fn b12_purge_rows_cards_manual_block_under_days_style_cutoff() {
        let conn = open_memory().unwrap();
        let today = date("2026-07-24");
        let cutoff = cutoff_for_days(today, 30);
        insert_block(&conn, "2026-05-01", Some("tempo-9"), Some("manual"), None);

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 1);
        assert_eq!(count(&conn, "blocks"), 1);
    }

    /// B37: no block is deleted, so the never-billed loss counter stays
    /// zero even for a block with neither marker.
    #[test]
    fn b37_unbilled_block_is_kept_and_loss_counter_stays_zero() {
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
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_deleted_unbilled, 0);
        assert_eq!(count(&conn, "blocks"), 3);
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

    /// Finding D: a `session_pins` row whose session's events are ALL
    /// purged is purged with them — else a branch name reused months
    /// later would silently inherit the stale pin (`pin_for_branch` has
    /// no age bound of its own). A pin whose session still has a
    /// surviving event keeps its pin.
    #[test]
    fn session_pin_purged_when_its_session_events_are_gone() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_event_with_session(&conn, "2026-02-10T09:00:00+00:00", "old-1", "sess-old");
        insert_session_pin(&conn, "sess-old", "2026-02-10T08:00:00+00:00");

        insert_event_with_session(&conn, "2026-06-25T09:00:00+00:00", "recent-1", "sess-new");
        insert_session_pin(&conn, "sess-new", "2026-06-25T08:00:00+00:00");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.events_deleted, 1);
        assert_eq!(count(&conn, "session_pins"), 1);
        let remaining: String = conn
            .query_row("SELECT session_id FROM session_pins", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, "sess-new");
    }

    /// Finding P1: a pin whose session has no event row YET (written
    /// moments before the recorder's first event, or a recorder that then
    /// failed) must not be a purge target on ANY cutoff — the delete needs
    /// the same `from_at` age bound every sibling delete in this pass
    /// already has, else a brand-new pin is wiped by a purge whose cutoff
    /// is years in the past.
    #[test]
    fn fresh_session_pin_without_events_survives_an_old_cutoff() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_session_pin(&conn, "sess-fresh", "2026-07-01T09:00:00+00:00");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.events_deleted, 0);
        assert_eq!(count(&conn, "session_pins"), 1);
    }

    /// B14: an `external = 1` ticket no block references is deleted; one
    /// still referenced by a (carded) old block is kept.
    #[test]
    fn b14_orphaned_external_ticket_deleted_referenced_one_kept() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        insert_ticket(&conn, "EXT-1", 1);
        insert_ticket(&conn, "EXT-9", 1);
        insert_block_with_ticket(&conn, "2026-02-10", "EXT-9");

        let report = purge_rows(&conn, cutoff, false).unwrap();
        assert_eq!(report.tickets_deleted, 1);
        assert_eq!(count(&conn, "jira_tickets"), 1);
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

    /// T9: the index-usable `started_at < date_bound_iso` pre-filter must
    /// never exclude a row the original `datetime(started_at) <
    /// datetime(?1)` predicate matches. `WORKLOG_TZ=-23:59` pushes local
    /// midnight to the very end of `cutoff`'s UTC day, and a `+14:00`
    /// offset on the stored timestamp then pushes its LITERAL calendar
    /// date to `cutoff + 1 day` — the exact ceiling the safety proof
    /// relies on — even though its UTC instant (`cutoff` 23:00) is
    /// genuinely before the cutoff. A `date_bound_iso` one day short of
    /// what the code computes (`cutoff + 2 days`) would make the
    /// pre-filter wrongly spare this row, so this test fails if that
    /// bound regresses.
    #[test]
    fn t9_purge_prefilter_deletes_row_at_offset_shifted_literal_date_ceiling() {
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-23:59");
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        // UTC instant 2026-06-20T23:00:00Z (before the shifted threshold
        // of 2026-06-20T23:59:00Z) written with a +14:00 offset: literal
        // date 2026-06-21, i.e. `cutoff + 1 day`.
        insert_event(&conn, "2026-06-21T13:00:00+14:00", "offset-ceiling-event");
        insert_session(&conn, "sess-offset-ceiling", "2026-06-21T13:00:00+14:00");

        let report = purge_rows(&conn, cutoff, false);
        std::env::remove_var("WORKLOG_TZ");
        let report = report.unwrap();

        assert_eq!(report.events_deleted, 1);
        assert_eq!(report.sessions_deleted, 1);
        assert_eq!(count(&conn, "events"), 0);
        assert_eq!(count(&conn, "sessions"), 0);
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
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 2);
        assert!(snapshot_path.is_file());

        // The snapshot is a self-contained, openable db with the rows the
        // real database is about to lose.
        let snap_conn = Connection::open(&snapshot_path).unwrap();
        let blocks: i64 = snap_conn
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(blocks, 2);
        let snap_cards: i64 = snap_conn
            .query_row("SELECT COUNT(*) FROM block_digest", [], |r| r.get(0))
            .unwrap();
        assert_eq!(snap_cards, 0, "the snapshot is taken before any card");
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
            insert_bulky_event(
                &conn,
                "2026-02-10T09:00:00+00:00",
                &format!("bulk-{i}"),
                &filler,
            );
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

        assert_eq!(report.events_deleted, 300);
        assert!(report.bytes_freed >= 0);
        let size_after = std::fs::metadata(&db_path).unwrap().len();
        assert!(
            size_after <= size_before_delete,
            "file grew: before={size_before_delete} after={size_after}"
        );
    }

    /// Serialises mutation of `WORKLOG_PRUNE_ENABLED`, mirroring
    /// `tz::test_env_lock` / `schedule::ENV_LOCK` — this env var is
    /// process-global, so concurrent tests flipping it would otherwise
    /// race.
    fn prune_enabled_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        crate::envfile::ENV_TEST_LOCK.blocking_lock()
    }

    #[test]
    fn meta_get_missing_key_returns_none() {
        let conn = open_memory().unwrap();
        assert_eq!(meta_get(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn meta_set_then_get_round_trips_and_upserts() {
        let conn = open_memory().unwrap();
        meta_set(&conn, "k", "v1").unwrap();
        assert_eq!(meta_get(&conn, "k").unwrap(), Some("v1".to_string()));
        meta_set(&conn, "k", "v2").unwrap();
        assert_eq!(
            meta_get(&conn, "k").unwrap(),
            Some("v2".to_string()),
            "meta_set must upsert, not duplicate"
        );
    }

    /// B23: the latch already equals the freshly computed cutoff — a
    /// total no-op. No deletion, no meta write, and (critically) no
    /// snapshot file — proving the no-op is total, not merely
    /// delete-free.
    #[test]
    fn b23_latch_equal_to_cutoff_is_a_total_noop() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        let cutoff = date("2026-06-20");
        meta_set(&conn, LATCH_KEY, "2026-06-20").unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff,
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let result = prune_if_due(&conn, &opts).unwrap();
        assert!(result.is_none());
        assert_eq!(count(&conn, "blocks"), 1);
        assert!(
            !snapshot_path.exists(),
            "a total no-op must not even attempt a snapshot"
        );
    }

    /// B24: a successful `prune_if_due` leaves the latch at the new
    /// cutoff.
    #[test]
    fn b24_successful_prune_leaves_latch_at_new_cutoff() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        let cutoff = date("2026-06-20");
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff,
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let report = prune_if_due(&conn, &opts).unwrap().unwrap();
        assert_eq!(report.blocks_deleted, 0);
        assert_eq!(report.blocks_carded, 1);
        assert_eq!(
            meta_get(&conn, LATCH_KEY).unwrap(),
            Some("2026-06-20".to_string())
        );
    }

    /// Plus: calling `prune_if_due` twice with the same cutoff does the
    /// work exactly once — the second call is B23's no-op.
    #[test]
    fn prune_if_due_called_twice_does_the_work_once() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        let cutoff = date("2026-06-20");
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff,
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let first = prune_if_due(&conn, &opts).unwrap();
        assert!(first.is_some());
        assert_eq!(count(&conn, "blocks"), 1);
        assert_eq!(count(&conn, "block_digest"), 1);

        let second = prune_if_due(&conn, &opts).unwrap();
        assert!(
            second.is_none(),
            "same cutoff twice must do the work only once"
        );
    }

    /// B26: a prune that fails (unwritable snapshot target, reusing
    /// B19's trick) leaves the latch at whatever it was before — so the
    /// next check retries the exact same work instead of silently
    /// skipping it.
    #[test]
    fn b26_failed_prune_leaves_latch_at_previous_value() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        meta_set(&conn, LATCH_KEY, "2026-05-20").unwrap();
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);

        let unwritable = tmp
            .path()
            .join("does-not-exist")
            .join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff: date("2026-06-20"),
            dry_run: false,
            snapshot_to: Some(unwritable.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let result = prune_if_due(&conn, &opts);
        assert!(result.is_err());
        assert_eq!(count(&conn, "blocks"), 1, "delete must not have run");
        assert_eq!(
            meta_get(&conn, LATCH_KEY).unwrap(),
            Some("2026-05-20".to_string()),
            "latch must remain at its previous value so the next check retries"
        );
        assert_eq!(
            meta_get(&conn, LAST_REPORT_KEY).unwrap(),
            None,
            "a failed prune must not persist a report either"
        );
        assert_eq!(
            meta_get(&conn, LAST_RUN_KEY).unwrap(),
            None,
            "a failed prune must not persist a timestamp either"
        );
    }

    /// B32: a successful automatic prune persists a report to
    /// [`LAST_REPORT_KEY`] whose cutoff and counts match what
    /// `prune_if_due` returned, plus a parseable RFC3339 timestamp to
    /// [`LAST_RUN_KEY`] — both readable back via [`last_prune`].
    #[test]
    fn b32_successful_prune_persists_last_report_and_timestamp() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("worklog.db");
        let conn = crate::db::open(&db_path).unwrap();
        let cutoff = date("2026-06-20");
        insert_block(&conn, "2026-02-10", Some("tempo-1"), None, None);
        insert_block(&conn, "2026-02-11", None, None, None);

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let opts = PruneOptions {
            cutoff,
            dry_run: false,
            snapshot_to: Some(snapshot_path.as_path()),
            db_path: Some(db_path.as_path()),
        };
        let returned = prune_if_due(&conn, &opts).unwrap().unwrap();

        let stored_json = meta_get(&conn, LAST_REPORT_KEY).unwrap().unwrap();
        let stored: PurgeReport = serde_json::from_str(&stored_json).unwrap();
        assert_eq!(stored.cutoff_date, returned.cutoff_date);
        assert_eq!(stored.blocks_carded, returned.blocks_carded);
        assert_eq!(stored.blocks_carded, 2);
        assert_eq!(stored.blocks_deleted, 0);

        let ran_at = meta_get(&conn, LAST_RUN_KEY).unwrap().unwrap();
        chrono::DateTime::parse_from_rfc3339(&ran_at)
            .expect("last_prune_at must be a parseable RFC3339 timestamp");

        let lp = last_prune(&conn).unwrap().unwrap();
        assert_eq!(lp.report.cutoff_date, returned.cutoff_date);
        assert_eq!(lp.report.blocks_carded, returned.blocks_carded);
        assert_eq!(lp.ran_at, ran_at);
    }

    /// B33: a not-due `prune_if_due` (latch already equals the computed
    /// cutoff) writes neither the report nor the timestamp key, and
    /// leaves the latch itself untouched — the not-due path is a total
    /// no-op, not merely delete-free.
    #[test]
    fn b33_not_due_prune_writes_neither_report_nor_timestamp() {
        let conn = open_memory().unwrap();
        let cutoff = date("2026-06-20");
        meta_set(&conn, LATCH_KEY, "2026-06-20").unwrap();

        let opts = PruneOptions {
            cutoff,
            dry_run: false,
            snapshot_to: None,
            db_path: None,
        };
        let result = prune_if_due(&conn, &opts).unwrap();
        assert!(result.is_none());
        assert_eq!(meta_get(&conn, LAST_REPORT_KEY).unwrap(), None);
        assert_eq!(meta_get(&conn, LAST_RUN_KEY).unwrap(), None);
        assert_eq!(
            meta_get(&conn, LATCH_KEY).unwrap(),
            Some("2026-06-20".to_string()),
            "latch itself must stay untouched by a not-due check"
        );
    }

    /// `last_prune` reports `None` when no automatic prune has ever run
    /// — the absent-keys case B35's CLI rendering depends on.
    #[test]
    fn last_prune_returns_none_when_never_run() {
        let conn = open_memory().unwrap();
        assert!(last_prune(&conn).unwrap().is_none());
    }

    /// A `PurgeReport` serialised to `meta` JSON and read back equals
    /// the original, field by field — the round-trip `Deserialize`
    /// exists for.
    #[test]
    fn purge_report_round_trips_through_meta_json() {
        let conn = open_memory().unwrap();
        let report = PurgeReport {
            cutoff_date: "2026-06-20".to_string(),
            blocks_deleted: 12,
            blocks_carded: 7,
            card_bytes: 900,
            card_bytes_median: 100,
            card_bytes_max: 400,
            cache_rows_deleted: 5,
            blocks_deleted_unbilled: 3,
            events_deleted: 145,
            sessions_deleted: 4,
            tickets_deleted: 2,
            bytes_freed: 4096,
            snapshot_path: Some("/tmp/worklog.db.preprune".to_string()),
            dry_run: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        meta_set(&conn, LAST_REPORT_KEY, &json).unwrap();

        let stored_json = meta_get(&conn, LAST_REPORT_KEY).unwrap().unwrap();
        let round_tripped: PurgeReport = serde_json::from_str(&stored_json).unwrap();
        assert_eq!(round_tripped.cutoff_date, report.cutoff_date);
        assert_eq!(round_tripped.blocks_deleted, report.blocks_deleted);
        assert_eq!(round_tripped.blocks_carded, report.blocks_carded);
        assert_eq!(round_tripped.card_bytes, report.card_bytes);
        assert_eq!(round_tripped.card_bytes_median, report.card_bytes_median);
        assert_eq!(round_tripped.card_bytes_max, report.card_bytes_max);
        assert_eq!(round_tripped.cache_rows_deleted, report.cache_rows_deleted);
        assert_eq!(
            round_tripped.blocks_deleted_unbilled,
            report.blocks_deleted_unbilled
        );
        assert_eq!(round_tripped.events_deleted, report.events_deleted);
        assert_eq!(round_tripped.sessions_deleted, report.sessions_deleted);
        assert_eq!(round_tripped.tickets_deleted, report.tickets_deleted);
        assert_eq!(round_tripped.bytes_freed, report.bytes_freed);
        assert_eq!(round_tripped.snapshot_path, report.snapshot_path);
        assert_eq!(round_tripped.dry_run, report.dry_run);
    }

    #[test]
    fn pruning_enabled_defaults_to_true_when_unset() {
        let _g = prune_enabled_env_lock();
        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
        assert!(pruning_enabled());
    }

    #[test]
    fn pruning_enabled_false_values_disable_case_insensitively() {
        let _g = prune_enabled_env_lock();
        for v in ["0", "false", "FALSE", "no", "NO", "off", "OFF"] {
            std::env::set_var("WORKLOG_PRUNE_ENABLED", v);
            assert!(!pruning_enabled(), "expected disabled for {v:?}");
        }
        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
    }

    #[test]
    fn pruning_enabled_true_for_other_values() {
        let _g = prune_enabled_env_lock();
        for v in ["1", "true", "yes", "on"] {
            std::env::set_var("WORKLOG_PRUNE_ENABLED", v);
            assert!(pruning_enabled(), "expected enabled for {v:?}");
        }
        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
    }

    /// Serialises mutation of the two pruner-cycle-day env vars,
    /// mirroring `prune_enabled_env_lock` above — process-global, so
    /// concurrent tests setting them would otherwise race.
    fn billing_day_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        crate::envfile::ENV_TEST_LOCK.blocking_lock()
    }

    /// B28-adjacent: the shared range predicate both settings validate
    /// against.
    #[test]
    fn is_valid_cycle_day_accepts_1_through_31_only() {
        assert!(is_valid_cycle_day(1));
        assert!(is_valid_cycle_day(20));
        assert!(is_valid_cycle_day(31));
        assert!(!is_valid_cycle_day(0));
        assert!(!is_valid_cycle_day(32));
    }

    /// B27: nothing configured anywhere — both accessors report their
    /// documented defaults.
    #[test]
    fn configured_cycle_start_day_defaults_when_unset() {
        let _g = billing_day_env_lock();
        std::env::remove_var("WORKLOG_BILLING_CYCLE_START_DAY");
        assert_eq!(configured_cycle_start_day(), DEFAULT_CYCLE_START_DAY);
    }

    #[test]
    fn configured_close_day_defaults_when_unset() {
        let _g = billing_day_env_lock();
        std::env::remove_var("WORKLOG_BILLING_CLOSE_DAY");
        assert_eq!(configured_close_day(), DEFAULT_CLOSE_DAY);
    }

    /// B29-adjacent: the process env is consulted at all (the settings
    /// API round trip itself is proven at the daemon layer).
    #[test]
    fn configured_cycle_start_day_reads_process_env() {
        let _g = billing_day_env_lock();
        std::env::set_var("WORKLOG_BILLING_CYCLE_START_DAY", "15");
        assert_eq!(configured_cycle_start_day(), 15);
        std::env::remove_var("WORKLOG_BILLING_CYCLE_START_DAY");
    }

    /// Unparseable or out-of-range stored values fall back to the
    /// default rather than propagating (spec 002 §5.4's "unparseable
    /// pruner setting" row).
    #[test]
    fn configured_close_day_falls_back_on_unparseable_or_out_of_range_value() {
        let _g = billing_day_env_lock();
        for bad in ["abc", "0", "32", "-1"] {
            std::env::set_var("WORKLOG_BILLING_CLOSE_DAY", bad);
            assert_eq!(
                configured_close_day(),
                DEFAULT_CLOSE_DAY,
                "bad value {bad:?} must fall back to the default"
            );
        }
        std::env::remove_var("WORKLOG_BILLING_CLOSE_DAY");
    }
}
