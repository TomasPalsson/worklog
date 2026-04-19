//! Gap-timeout block clustering.
//!
//! `build_blocks()` is a pure function over an event list;
//! `persist_blocks()` writes them to the db. Separated so tests can exercise
//! the clustering algorithm without any SQLite setup.
//!
//! Invariants:
//! * Calendar events are authoritative closed units — they never absorb
//!   code/commit activity and never get absorbed by it.
//! * Re-inference preserves `tempo_worklog_id`, `description`, and
//!   `estimated_by` per-block-start so syncing and AI estimates survive.
//! * Blocks shorter than MIN_BLOCK are dropped; longer than MAX_BLOCK are
//!   flagged for human review.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const TIMEOUT_MINUTES: i64 = 20;
const CREDIT_MINUTES: i64 = 2;
const MIN_BLOCK_MINUTES: i64 = 5;
const MAX_BLOCK_MINUTES: i64 = 4 * 60;

/// Event sources that are treated as authoritative calendar blocks.
pub fn is_calendar_source(s: &str) -> bool {
    matches!(s, "gcal")
}

/// A single event as consumed by the clustering algorithm. Kept separate
/// from [`crate::models::Event`] so we can drive tests without an owned
/// connection.
#[derive(Debug, Clone)]
pub struct InferEvent {
    pub ts: DateTime<Utc>,
    pub source: String,
    pub duration_seconds: Option<i64>,
    pub jira_issue: Option<String>,
    pub event_id: Option<i64>,
}

impl InferEvent {
    pub fn end(&self) -> DateTime<Utc> {
        if is_calendar_source(&self.source) {
            if let Some(d) = self.duration_seconds {
                return self.ts + Duration::seconds(d);
            }
        }
        self.ts + Duration::minutes(CREDIT_MINUTES)
    }

    pub fn is_calendar(&self) -> bool {
        is_calendar_source(&self.source)
    }
}

/// A finalized block ready for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferBlock {
    pub day: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_seconds: i64,
    pub event_count: u32,
    pub event_ids: Vec<i64>,
    pub jira_issue: Option<String>,
    pub flagged: bool,
    /// Track the source kind so the clustering pass can refuse to extend
    /// a calendar block. Skipped in serialization; not needed on disk.
    #[serde(skip)]
    is_calendar: bool,
    #[serde(skip)]
    events: Vec<InferEvent>,
}

fn new_block(e: &InferEvent) -> InferBlock {
    let end = e.end();
    InferBlock {
        // Bucket the block on the user's LOCAL day (driven by
        // `$WORKLOG_TZ`; defaults to UTC). Without this a developer in
        // UTC-5 sees 23:00-local work land on tomorrow's page.
        day: crate::tz::local_date(e.ts).to_string(),
        started_at: e.ts,
        ended_at: end,
        duration_seconds: (end - e.ts).num_seconds(),
        event_count: 1,
        event_ids: e.event_id.into_iter().collect(),
        jira_issue: e.jira_issue.clone(),
        flagged: false,
        is_calendar: e.is_calendar(),
        events: vec![e.clone()],
    }
}

fn extend_block(block: &mut InferBlock, e: &InferEvent) {
    let end = e.end().max(block.ended_at);
    block.ended_at = end;
    block.duration_seconds = (end - block.started_at).num_seconds();
    block.event_count += 1;
    if let Some(id) = e.event_id {
        block.event_ids.push(id);
    }
    block.events.push(e.clone());
}

fn finalize(mut block: InferBlock) -> Option<InferBlock> {
    let duration = block.ended_at - block.started_at;
    if duration < Duration::minutes(MIN_BLOCK_MINUTES) {
        return None;
    }
    if duration > Duration::minutes(MAX_BLOCK_MINUTES) {
        block.flagged = true;
    }
    let issues: HashSet<String> = block
        .events
        .iter()
        .filter_map(|e| e.jira_issue.clone())
        .collect();
    block.jira_issue = if issues.len() == 1 {
        issues.into_iter().next()
    } else {
        None
    };
    Some(block)
}

/// Pure clustering over a day's events. Input order doesn't matter.
pub fn build_blocks(events: Vec<InferEvent>) -> Vec<InferBlock> {
    let mut usable = events;
    usable.sort_by_key(|e| e.ts);

    let mut blocks: Vec<InferBlock> = Vec::new();
    let mut current: Option<InferBlock> = None;

    for e in usable {
        let Some(mut c) = current.take() else {
            current = Some(new_block(&e));
            continue;
        };

        let gap = e.ts - c.ended_at;
        let closed = e.is_calendar() || c.is_calendar || gap > Duration::minutes(TIMEOUT_MINUTES);
        if closed {
            if let Some(f) = finalize(c) {
                blocks.push(f);
            }
            current = Some(new_block(&e));
        } else {
            extend_block(&mut c, &e);
            current = Some(c);
        }
    }
    if let Some(c) = current {
        if let Some(f) = finalize(c) {
            blocks.push(f);
        }
    }
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

// ───────────────────────── db glue ─────────────────────────

pub fn load_day_events(conn: &Connection, day: NaiveDate) -> Result<Vec<InferEvent>> {
    // `day` is interpreted in the user's local TZ (`$WORKLOG_TZ`); the
    // SQL range is the UTC window that covers it.
    let (start_utc, end_utc) = crate::tz::utc_window_for_local_day(day);
    let start = start_utc.to_rfc3339();
    let end = end_utc.to_rfc3339();
    let mut stmt = conn.prepare(
        "SELECT id, source, started_at, duration_seconds, jira_issue
           FROM events
          WHERE started_at >= ?1 AND started_at < ?2
          ORDER BY started_at",
    )?;
    // started_at is ISO-8601 string; we compare lexicographically which works
    // because the format is fixed-width. Use the fixed-width prefix to match
    // the Python code exactly.
    let start = iso_prefix(&start);
    let end = iso_prefix(&end);
    let iter = stmt.query_map(params![start, end], |r| {
        let iso: String = r.get(2)?;
        let ts = chrono::DateTime::parse_from_rfc3339(&iso)
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        Ok(InferEvent {
            event_id: Some(r.get(0)?),
            source: r.get(1)?,
            ts,
            duration_seconds: r.get(3)?,
            jira_issue: r.get(4)?,
        })
    })?;
    iter.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Use the same string form Python emits (`datetime.isoformat()` without
/// offset suffix) so the text comparison works either way. rfc3339 adds
/// `+00:00`; strip it for parity with the Python code.
fn iso_prefix(s: &str) -> String {
    s.trim_end_matches("+00:00").to_owned()
}

pub fn persist_blocks(conn: &Connection, day: NaiveDate, blocks: &[InferBlock]) -> Result<()> {
    let day_iso = day.to_string();

    // Collect "carry" state so re-inference preserves tempo_worklog_id,
    // description, estimated_by, and manual ticket edits. We keep two
    // views of the prior state: a strict started_at→CarryRow map for the
    // common case (no time shift), and an ordered Vec of (start, end, row)
    // for the fallback case where a backfilled earlier event shifts the
    // block's started_at. See `carry_for_block` for the matching rules.
    let mut prior: std::collections::HashMap<String, CarryRow> = std::collections::HashMap::new();
    let mut prior_list: Vec<CarryRow> = Vec::new();
    {
        // ORDER BY started_at matters: the overlap-fallback claims rows
        // in iteration order, so two new blocks that both overlap the
        // same prior would otherwise race nondeterministically for its
        // carry state (tempo_worklog_id in particular). Stable order
        // ensures the earliest-starting new block claims the earliest
        // prior.
        let mut stmt = conn.prepare(
            "SELECT started_at, ended_at, jira_issue, description, estimated_by, tempo_worklog_id
               FROM blocks WHERE day = ?1 ORDER BY started_at",
        )?;
        let iter = stmt.query_map(params![day_iso], |r| {
            Ok(CarryRow {
                started_at: r.get(0)?,
                ended_at: r.get(1)?,
                jira_issue: r.get(2)?,
                description: r.get(3)?,
                estimated_by: r.get(4)?,
                tempo_worklog_id: r.get(5)?,
            })
        })?;
        for row in iter {
            let row = row?;
            prior.insert(row.started_at.clone(), row.clone());
            prior_list.push(row);
        }
    }
    // Track which fallback rows we've already claimed so two new blocks
    // can't both inherit the same prior state.
    let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();

    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM blocks WHERE day = ?1", params![day_iso])
        .context("clearing stale blocks")?;

    for b in blocks {
        let started_key = block_iso(b.started_at);
        let ended_key = block_iso(b.ended_at);
        let carry: Option<&CarryRow> = prior.get(&started_key).or_else(|| {
            // Overlap fallback: if no exact-start match, find one prior
            // block whose time range overlaps the new block's. Must not
            // already be claimed by a different new block.
            prior_list
                .iter()
                .find(|c| {
                    !claimed.contains(&c.started_at)
                        && ranges_overlap(
                            &c.started_at,
                            &c.ended_at,
                            &started_key,
                            &ended_key,
                        )
                })
        });
        if let Some(c) = carry {
            claimed.insert(c.started_at.clone());
        }
        let tempo_id = carry.and_then(|c| c.tempo_worklog_id.clone());
        let description = carry.and_then(|c| c.description.clone());
        let estimated_by = carry.and_then(|c| c.estimated_by.clone());
        // Preserve manual ticket override if present; otherwise trust inference.
        let jira_issue = carry
            .and_then(|c| c.jira_issue.clone())
            .or_else(|| b.jira_issue.clone());

        tx.execute(
            "INSERT INTO blocks (
                day, jira_issue, started_at, ended_at,
                duration_seconds, description, estimated_by, flagged,
                tempo_worklog_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                b.day,
                jira_issue,
                block_iso(b.started_at),
                block_iso(b.ended_at),
                b.duration_seconds,
                description,
                estimated_by,
                if b.flagged { 1 } else { 0 },
                tempo_id,
            ],
        )
        .context("inserting block")?;
        let block_id = tx.last_insert_rowid();
        for eid in &b.event_ids {
            tx.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![block_id, eid],
            )
            .context("inserting block_events row")?;
        }
    }
    tx.commit().context("committing block persistence")?;
    Ok(())
}

/// Format timestamps the way the Python code does (`datetime.isoformat()`
/// with offset). Python emits sub-seconds iff microsecond != 0; we use
/// `SecondsFormat::AutoSi` which behaves the same way (no fractional
/// when zero). This maximises the odds of exact-string match with rows
/// Python wrote; the overlap fallback handles the remaining cases.
/// The `false` argument forces `+00:00` instead of `Z` for the offset.
fn block_iso(dt: DateTime<Utc>) -> String {
    use chrono::SecondsFormat;
    dt.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

#[derive(Debug, Clone)]
struct CarryRow {
    started_at: String,
    ended_at: String,
    jira_issue: Option<String>,
    description: Option<String>,
    estimated_by: Option<String>,
    tempo_worklog_id: Option<String>,
}

/// Overlap check on ISO-8601 timestamps. Parses each string to a
/// `DateTime` before comparing so we're robust to cross-format
/// differences — e.g. Python emits `T09:00:00+00:00` (no sub-seconds
/// when microsecond == 0) while Rust's `block_iso` historically emitted
/// `T09:00:00.000000+00:00` (forced microseconds). Lexicographic
/// comparison of those two strings gives the wrong answer because
/// `.` (0x2E) sorts after `+` (0x2B). Parsing them both sidesteps the
/// issue entirely.
fn ranges_overlap(a_start: &str, a_end: &str, b_start: &str, b_end: &str) -> bool {
    let Some((a_s, a_e)) = parse_pair(a_start, a_end) else {
        return false;
    };
    let Some((b_s, b_e)) = parse_pair(b_start, b_end) else {
        return false;
    };
    a_s < b_e && b_s < a_e
}

fn parse_pair(start: &str, end: &str) -> Option<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
    // Accept the common variants our codebase writes:
    //   * `+00:00` offset (what block_iso emits)
    //   * `Z` (chrono's default to_rfc3339 on some builds)
    //   * naive (no offset) — treat as UTC
    let s = parse_maybe_utc(start)?;
    let e = parse_maybe_utc(end)?;
    Some((s, e))
}

fn parse_maybe_utc(s: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Fallback: naive ISO → assume UTC.
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(naive.and_utc());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;
    use chrono::TimeZone;

    #[test]
    fn ranges_overlap_handles_cross_format_timestamps() {
        // The concrete bug: Python emits `09:00:00+00:00` (no
        // sub-seconds when microsecond==0), Rust's old block_iso emitted
        // `09:00:00.000000+00:00`. Lexicographic compare said "Python
        // start > Rust end" because `.` > `+`, so the overlap fallback
        // silently failed and the carry state was lost.
        //
        // Parsed-DateTime compare doesn't care about formatting.
        assert!(ranges_overlap(
            "2026-04-18T09:00:00.000000+00:00", // Rust micro-precision
            "2026-04-18T09:30:00.000000+00:00",
            "2026-04-18T09:00:00+00:00",        // Python second-precision
            "2026-04-18T09:30:00+00:00",
        ));

        // And Z suffix vs +00:00 suffix, both should parse fine.
        assert!(ranges_overlap(
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            "2026-04-18T09:15:00+00:00",
            "2026-04-18T09:45:00+00:00",
        ));

        // Non-overlapping ranges still return false.
        assert!(!ranges_overlap(
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            "2026-04-18T09:30:00.000000+00:00",
            "2026-04-18T10:00:00.000000+00:00",
        ));

        // Unparseable strings fail closed (no overlap).
        assert!(!ranges_overlap("garbage", "garbage", "also", "also"));
    }

    #[test]
    fn block_iso_matches_python_for_whole_seconds() {
        // Python: `datetime(2026,4,18,9,0,0,tzinfo=UTC).isoformat()`
        //   → "2026-04-18T09:00:00+00:00"
        // Rust block_iso with SecondsFormat::AutoSi must produce the
        // same string (no `.000000` padding).
        let dt = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        assert_eq!(block_iso(dt), "2026-04-18T09:00:00+00:00");
    }

    #[test]
    fn block_iso_emits_sub_seconds_when_non_zero() {
        // Matches Python: isoformat emits sub-seconds iff non-zero.
        use chrono::Timelike;
        let dt = Utc
            .with_ymd_and_hms(2026, 4, 18, 9, 0, 0)
            .unwrap()
            .with_nanosecond(123_456_789)
            .unwrap();
        let s = block_iso(dt);
        assert!(
            s.starts_with("2026-04-18T09:00:00.")
                && s.ends_with("+00:00"),
            "block_iso should emit sub-seconds for non-zero nanos: {s}"
        );
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 18, h, m, 0).unwrap()
    }

    fn ev(h: u32, m: u32, source: &str) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: source.into(),
            duration_seconds: None,
            jira_issue: None,
            event_id: None,
        }
    }

    fn calendar(h: u32, m: u32, secs: i64) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: "gcal".into(),
            duration_seconds: Some(secs),
            jira_issue: None,
            event_id: None,
        }
    }

    #[test]
    fn single_event_blocks_are_dropped_when_shorter_than_min() {
        // Point event extends by CREDIT (2m) < MIN_BLOCK (5m) → dropped.
        let blocks = build_blocks(vec![ev(10, 0, "github_commit")]);
        assert!(blocks.is_empty());
    }

    #[test]
    fn close_events_form_one_block() {
        let events: Vec<_> = (0..5)
            .map(|i| ev(10, 5 * i as u32, "github_commit"))
            .collect();
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].event_count, 5);
    }

    #[test]
    fn gap_over_timeout_starts_new_block() {
        let a = ev(9, 0, "github_commit");
        let b = ev(9, 5, "github_commit");
        // 30-minute gap exceeds TIMEOUT (20m).
        let c = ev(9, 35, "github_commit");
        let d = ev(9, 40, "github_commit");
        let blocks = build_blocks(vec![a, b, c, d]);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn calendar_event_is_authoritative_and_isolated() {
        let code_before = ev(9, 0, "github_commit");
        let meeting = calendar(9, 10, 45 * 60); // 45-min meeting
        let code_after = ev(10, 0, "github_commit");
        let blocks = build_blocks(vec![code_before, meeting, code_after]);
        // Three separate blocks: code block before is <MIN so dropped;
        // meeting is its own block; code after is <MIN so dropped.
        // Result: exactly one block (the meeting).
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].duration_seconds, 45 * 60);
    }

    #[test]
    fn jira_issue_is_kept_when_all_events_agree() {
        let mut a = ev(10, 0, "github_commit");
        a.jira_issue = Some("PROJ-1".into());
        let mut b = ev(10, 5, "github_commit");
        b.jira_issue = Some("PROJ-1".into());
        let blocks = build_blocks(vec![a, b]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].jira_issue.as_deref(), Some("PROJ-1"));
    }

    #[test]
    fn jira_issue_cleared_when_events_disagree() {
        let mut a = ev(10, 0, "github_commit");
        a.jira_issue = Some("PROJ-1".into());
        let mut b = ev(10, 5, "github_commit");
        b.jira_issue = Some("PROJ-2".into());
        let blocks = build_blocks(vec![a, b]);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].jira_issue.is_none());
    }

    #[test]
    fn flags_blocks_over_max() {
        let start = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        let end = start + Duration::hours(5); // > MAX_BLOCK
        let events = vec![
            InferEvent {
                ts: start,
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
            },
            InferEvent {
                ts: end - Duration::minutes(1),
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
            },
        ];
        // Gap is > TIMEOUT, so these become two separate blocks.
        // Change: use densely packed events to form a single long block.
        let mut dense = Vec::new();
        let mut t = start;
        while t < end {
            dense.push(InferEvent {
                ts: t,
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
            });
            t += Duration::minutes(10);
        }
        let blocks = build_blocks(dense);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].flagged, "long block must be flagged");
        // Unused test vars — silence compiler.
        let _ = events;
    }

    #[test]
    fn persist_round_trip_creates_rows() {
        let conn = open_memory().unwrap();
        // Insert two raw events for the day.
        let e1 = repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "aaa", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        let e2 = repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "bbb",
                "2026-04-18T10:05:00+00:00",
                "second",
            ),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        assert_eq!(events.len(), 2);
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].duration_seconds, blocks[0].duration_seconds);

        // block_events must have both rows.
        let junction: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM block_events WHERE block_id = ?1",
                params![stored[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(junction, 2);
        let _ = (e1, e2);
    }

    #[test]
    fn reinference_preserves_tempo_id_and_description() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x2", "2026-04-18T10:05:00+00:00", "second"),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        // Simulate the user hand-editing.
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '98765', description = 'custom', \
             jira_issue = 'PROJ-7', estimated_by = 'manual' WHERE day = ?1",
            params!["2026-04-18"],
        )
        .unwrap();

        // Add another event and re-infer.
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x3", "2026-04-18T10:08:00+00:00", "third"),
        )
        .unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].tempo_worklog_id.as_deref(), Some("98765"));
        assert_eq!(stored[0].description.as_deref(), Some("custom"));
        assert_eq!(stored[0].jira_issue.as_deref(), Some("PROJ-7"));
        assert_eq!(stored[0].estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn load_day_events_window_respects_worklog_tz() {
        // The round-1 phase-5 test only exercised new_block's local_date
        // bucketing. The actual query path (utc_window_for_local_day
        // driving the SQL range) was untested with a non-UTC TZ. This
        // test covers it: an event at 04:30 UTC lands on local Apr 18
        // in UTC-5, so asking for local Apr 18 must return that event.
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let conn = open_memory().unwrap();
        // 04:30 UTC = 23:30 local on Apr 18 in UTC-5.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "late-commit",
                "2026-04-19T04:30:00+00:00",
                "late night work",
            ),
        )
        .unwrap();
        // 14:00 UTC on Apr 18 = 09:00 local — also on Apr 18.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "morning-commit",
                "2026-04-18T14:00:00+00:00",
                "morning work",
            ),
        )
        .unwrap();

        let events = load_day_events(&conn, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap())
            .expect("load_day_events");
        assert_eq!(events.len(), 2, "both events must land on local Apr 18");

        // And asking for local Apr 19 returns NOTHING (the 04:30Z event
        // is local Apr 18, not Apr 19).
        let none = load_day_events(&conn, NaiveDate::from_ymd_opt(2026, 4, 19).unwrap())
            .expect("load_day_events");
        assert_eq!(
            none.len(),
            0,
            "Apr 19 local should be empty — the 04:30Z event is Apr 18 local"
        );

        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn block_day_respects_worklog_tz() {
        // Regression for H4: without WORKLOG_TZ, a 23:30 local event in
        // UTC-5 (=04:30Z the next day) would land on the WRONG day's
        // review page. With WORKLOG_TZ=-05:00, it lands on the local day.
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let ts = chrono::Utc.with_ymd_and_hms(2026, 4, 19, 4, 30, 0).unwrap();
        let event = InferEvent {
            event_id: None,
            source: "manual".into(),
            ts,
            duration_seconds: Some(600),
            jira_issue: None,
        };
        let block = new_block(&event);
        assert_eq!(
            block.day, "2026-04-18",
            "04:30 UTC in UTC-5 is 23:30 on Apr 18 local"
        );
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn reinference_preserves_state_when_start_shifts_backward() {
        // Regression test for the infer carry bug: a backfilled earlier
        // event shifts a block's started_at. Strict-key equality loses
        // tempo_worklog_id (and other carry state) — the CLAUDE.md
        // canary invariant "tempo_worklog_id MUST NEVER be cleared" is
        // violated. The carry logic must fall back to overlap matching.
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "late1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "late2", "2026-04-18T10:05:00+00:00", "second"),
        )
        .unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        // User reviews and syncs this block.
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '42424', jira_issue = 'PROJ-3', \
             description = 'reviewed', estimated_by = 'manual' WHERE day = '2026-04-18'",
            [],
        )
        .unwrap();

        // A GitHub backfill now adds an EARLIER event, shifting the
        // block's started_at by several minutes. Strict-key match misses.
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "early1", "2026-04-18T09:55:00+00:00", "earlier"),
        )
        .unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].tempo_worklog_id.as_deref(),
            Some("42424"),
            "tempo_worklog_id MUST be preserved across started_at shift"
        );
        assert_eq!(stored[0].jira_issue.as_deref(), Some("PROJ-3"));
        assert_eq!(stored[0].description.as_deref(), Some("reviewed"));
        assert_eq!(stored[0].estimated_by.as_deref(), Some("manual"));
    }
}
