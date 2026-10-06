//! Tests for T031 — the recap built inside the 17:00 run (spec 018 FR-43..45).

use super::*;
use crate::daily_helpers_contract::{HeldBackLine, RecapGap, RecapLine};
use crate::db::open_memory;
use crate::{tempo_lines, tempo_match};
use chrono::{DateTime, FixedOffset, NaiveTime, TimeZone};
use rusqlite::params;

const DAY: &str = "2026-10-06"; // a Tuesday
const SATURDAY: &str = "2026-10-10";
const HOURS: &str = "Mon-Fri 09:00-17:00";

fn utc() -> std::sync::MutexGuard<'static, ()> {
    let g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    std::env::set_var("WORKLOG_ENV_FILE", "/nonexistent/worklog.env");
    g
}

fn at(day: &str, hm: &str) -> String {
    format!("{day}T{hm}:00+00:00")
}

fn secs(from: &str, to: &str) -> i64 {
    let t = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").unwrap();
    (t(to) - t(from)).num_seconds()
}

fn block(conn: &Connection, day: &str, from: &str, to: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, ?3, ?4)",
        params![day, at(day, from), at(day, to), secs(from, to)],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn recap(conn: &Connection, day: &str) -> Recap {
    build(conn, day, HOURS).unwrap().unwrap()
}

fn gap(from: &str, to: &str, minutes: i64) -> RecapGap {
    let z = |hm: &str| format!("{DAY}T{hm}:00Z");
    RecapGap {
        started_at: z(from),
        ended_at: z(to),
        minutes,
    }
}

// ───────────────────────── coverage (FR-43) ─────────────────────────

#[test]
fn coverage_is_union_inside_window_over_window_seconds() {
    let _g = utc();
    let conn = open_memory().unwrap();
    // 08:00-09:30 clips to 30 min (no clipping would count 90).
    block(&conn, DAY, "08:00", "09:30");
    // Two overlapping blocks count 10:00-12:00 once (a naive sum would give 3h).
    block(&conn, DAY, "10:00", "11:30");
    block(&conn, DAY, "10:30", "12:00");
    // Ignored blocks do not cover anything.
    let ignored = block(&conn, DAY, "13:00", "17:00");
    conn.execute(
        "UPDATE blocks SET is_personal = 1, ignored_at = '2026-10-06T18:00:00Z' WHERE id = ?1",
        [ignored],
    )
    .unwrap();
    // 30 min + 120 min = 150 min of 480 = 31.25 %.
    assert_eq!(recap(&conn, DAY).coverage_percent, 31);
}

#[test]
fn coverage_rounds_half_up_not_down() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "10:00"); // 60/480 = 12.5 %
    assert_eq!(recap(&conn, DAY).coverage_percent, 13);
}

#[test]
fn full_day_is_100_and_empty_day_is_0() {
    let _g = utc();
    let conn = open_memory().unwrap();
    assert_eq!(recap(&conn, DAY).coverage_percent, 0);
    block(&conn, DAY, "08:00", "18:00");
    assert_eq!(recap(&conn, DAY).coverage_percent, 100);
}

#[test]
fn no_recap_outside_work_days_or_with_unreadable_hours() {
    let _g = utc();
    let conn = open_memory().unwrap();
    assert!(build(&conn, SATURDAY, HOURS).unwrap().is_none());
    assert!(build(&conn, DAY, "whenever").unwrap().is_none());
}

#[test]
fn window_uses_the_owner_offset() {
    let _g = utc();
    std::env::set_var("WORKLOG_TZ", "+02:00");
    let conn = open_memory().unwrap();
    // 09:00-17:00 at +02:00 is 07:00-15:00 UTC: this block fills it exactly.
    block(&conn, DAY, "07:00", "15:00");
    let r = recap(&conn, DAY);
    std::env::remove_var("WORKLOG_TZ");
    assert_eq!(r.coverage_percent, 100);
}

// ───────────────────────── gaps (FR-43) ─────────────────────────

#[test]
fn gap_of_exactly_15_minutes_counts_14_does_not() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "10:00");
    block(&conn, DAY, "10:15", "11:00"); // 15 min gap: listed
    block(&conn, DAY, "11:14", "17:00"); // 14 min gap: not listed
    assert_eq!(recap(&conn, DAY).gaps, vec![gap("10:00", "10:15", 15)]);
}

#[test]
fn window_edges_are_gaps_and_blocks_outside_the_window_are_clipped() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "08:00", "09:20"); // clipped: window start is 09:00
    block(&conn, DAY, "10:00", "16:30");
    block(&conn, DAY, "16:30", "19:00"); // clipped: nothing after 17:00
    assert_eq!(
        recap(&conn, DAY).gaps,
        vec![gap("09:20", "10:00", 40)] // no gap before 09:20 or after 16:30
    );
    let conn = open_memory().unwrap();
    block(&conn, DAY, "10:00", "16:00");
    assert_eq!(
        recap(&conn, DAY).gaps,
        vec![gap("09:00", "10:00", 60), gap("16:00", "17:00", 60)]
    );
}

#[test]
fn only_the_three_longest_gaps_longest_first() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "10:00");
    block(&conn, DAY, "10:20", "11:00"); // 20
    block(&conn, DAY, "11:50", "12:00"); // 50
    block(&conn, DAY, "12:30", "13:00"); // 30
    block(&conn, DAY, "13:40", "16:40"); // 40
    let gaps = recap(&conn, DAY).gaps;
    let minutes: Vec<i64> = gaps.iter().map(|g| g.minutes).collect();
    assert_eq!(minutes, vec![50, 40, 30]); // 20 and the 20-min tail are cut
}

#[test]
fn a_recorded_break_removes_the_gap_or_shrinks_it() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "10:00");
    block(&conn, DAY, "11:00", "17:00");
    let add = |from: &str, to: &str| {
        conn.execute(
            "INSERT INTO recap_breaks (day, started_at, ended_at) VALUES (?1, ?2, ?3)",
            params![DAY, format!("{DAY}T{from}:00Z"), format!("{DAY}T{to}:00Z")],
        )
        .unwrap();
    };
    add("10:00", "10:20"); // leaves 40 min
    assert_eq!(recap(&conn, DAY).gaps, vec![gap("10:20", "11:00", 40)]);
    add("10:20", "10:50"); // leaves 10 min: below the minimum
    assert!(recap(&conn, DAY).gaps.is_empty());
    // A break is not time worked.
    assert_eq!(recap(&conn, DAY).coverage_percent, 88); // 420/480 = 87.5
}

// ───────────────────────── sent / held back (FR-43) ─────────────────────────

struct Line<'a> {
    issue: &'a str,
    origin: &'a str,
    text: Option<(&'a str, Option<&'a str>)>,
    sent: bool,
    error: Option<&'a str>,
    synced: bool,
}

fn line(issue: &str) -> Line<'_> {
    Line {
        issue,
        origin: "manual",
        text: Some(("manual", None)),
        sent: false,
        error: None,
        synced: false,
    }
}

fn seed(conn: &Connection, l: &Line) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds,
                             ticket_origin, tempo_worklog_id)
         VALUES (?1, ?2, ?3, ?3, 1800, ?4, ?5)",
        params![
            DAY,
            l.issue,
            at(DAY, "10:00"),
            l.origin,
            l.synced.then_some("5")
        ],
    )
    .unwrap();
    if let Some((origin, check)) = l.text {
        conn.execute(
            "INSERT INTO tempo_line_texts
               (day, jira_issue, text, text_origin, check_status, auto_sent_at, send_error, updated_at)
             VALUES (?1, ?2, 'Fixed the redirect', ?3, ?4, ?5, ?6, '2026-10-06T00:00:00Z')",
            params![
                DAY,
                l.issue,
                origin,
                check,
                l.sent.then_some("2026-10-06T17:00:00Z"),
                l.error
            ],
        )
        .unwrap();
    }
}

#[test]
fn sent_and_held_back_lines_carry_a_plain_reason() {
    let _g = utc();
    let conn = open_memory().unwrap();
    let sent = Line {
        sent: true,
        synced: true,
        ..line("S-1")
    };
    seed(&conn, &sent);
    let errored = Line {
        error: Some("Tempo said no"),
        ..line("E-1")
    };
    seed(&conn, &errored);
    seed(
        &conn,
        &Line {
            text: None,
            ..line("T-1")
        },
    );
    let unchecked = Line {
        text: Some(("generated", Some("needs_look"))),
        ..line("L-1")
    };
    seed(&conn, &unchecked);
    let unconfirmed = Line {
        origin: "auto",
        text: Some(("generated", Some("passed"))),
        ..line("C-1")
    };
    seed(&conn, &unconfirmed);
    // Synced on an earlier run: in neither list.
    seed(
        &conn,
        &Line {
            synced: true,
            ..line("D-1")
        },
    );
    // Ready and still unsent.
    seed(&conn, &line("P-1"));
    seed(&conn, &line("A-1"));
    let key = crate::tempo_line_contract::TempoLineKey {
        day: DAY.into(),
        jira_issue: "A-1".into(),
    };
    let a1 = tempo_lines::line_for(&conn, &key).unwrap().unwrap();
    tempo_match::mark_already(&conn, &a1, "77").unwrap();

    let r = recap(&conn, DAY);
    assert_eq!(
        r.sent,
        vec![RecapLine {
            jira_issue: "S-1".into(),
            seconds: 1800
        }]
    );
    let held = |issue: &str, reason: &str| HeldBackLine {
        jira_issue: issue.into(),
        reason: reason.into(),
    };
    assert_eq!(
        r.held_back,
        vec![
            held("A-1", "already in Tempo"),
            held("C-1", "ticket not confirmed"),
            held("E-1", "Tempo said no"),
            held("L-1", "text needs a look"),
            held("P-1", "not sent"),
            held("T-1", "no text"),
        ]
    );
}

// ───────────────────────── store ─────────────────────────

#[test]
fn build_and_store_round_trips_and_replaces_the_day() {
    let _g = utc();
    let conn = open_memory().unwrap();
    assert_eq!(latest(&conn).unwrap(), None);
    build_and_store(&conn, DAY, HOURS).unwrap();
    assert_eq!(latest(&conn).unwrap(), Some(recap(&conn, DAY)));
    block(&conn, DAY, "09:00", "17:00");
    build_and_store(&conn, DAY, HOURS).unwrap();
    assert_eq!(latest(&conn).unwrap().unwrap().coverage_percent, 100);
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM recaps", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1);
}

// ───────────────────────── gap actions (FR-44) ─────────────────────────

fn day_with_one_gap() -> Connection {
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "10:00");
    block(&conn, DAY, "10:30", "17:00");
    build_and_store(&conn, DAY, HOURS).unwrap();
    conn
}

const GAP_START: &str = "2026-10-06T10:00:00Z";

fn act(conn: &Connection, action: GapAction) -> Result<()> {
    apply_gap_with(conn, DAY, GAP_START, action, HOURS)
}

fn blocks(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn personal_adds_a_personal_manual_block_and_no_billable_line() {
    let _g = utc();
    let conn = day_with_one_gap();
    act(&conn, GapAction::Personal).unwrap();
    let (started, secs, personal, by, issue): (String, i64, bool, String, Option<String>) = conn
        .query_row(
            "SELECT started_at, duration_seconds, is_personal, estimated_by, jira_issue
               FROM blocks WHERE is_personal = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        (started.as_str(), secs, personal, by.as_str(), issue),
        ("2026-10-06T10:00:00+00:00", 1800, true, "manual", None)
    );
    assert!(tempo_lines::lines_for_day(&conn, DAY).unwrap().is_empty());
    let r = latest(&conn).unwrap().unwrap();
    assert!(r.gaps.is_empty());
    assert_eq!(r.coverage_percent, 100);
}

#[test]
fn break_records_the_gap_without_adding_a_block_or_coverage() {
    let _g = utc();
    let conn = day_with_one_gap();
    let before = blocks(&conn);
    act(&conn, GapAction::Break).unwrap();
    assert_eq!(blocks(&conn), before);
    let recorded: (String, String) = conn
        .query_row(
            "SELECT started_at, ended_at FROM recap_breaks WHERE day = ?1",
            [DAY],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(recorded, (GAP_START.into(), "2026-10-06T10:30:00Z".into()));
    let r = latest(&conn).unwrap().unwrap();
    assert!(r.gaps.is_empty());
    assert_eq!(r.coverage_percent, 94); // 450/480 = 93.75: the break is not covered
}

#[test]
fn pick_ticket_adds_a_billable_manual_block_on_that_key() {
    let _g = utc();
    let conn = day_with_one_gap();
    act(
        &conn,
        GapAction::PickTicket {
            jira_issue: "GENAI-12".into(),
        },
    )
    .unwrap();
    let lines = tempo_lines::lines_for_day(&conn, DAY).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].jira_issue.as_str(), lines[0].effective_seconds),
        ("GENAI-12", 1800)
    );
    let r = latest(&conn).unwrap().unwrap();
    assert!(r.gaps.is_empty());
    assert_eq!(r.coverage_percent, 100);
}

#[test]
fn an_unknown_gap_is_refused_and_changes_nothing() {
    let _g = utc();
    let conn = day_with_one_gap();
    let before = blocks(&conn);
    let err = apply_gap_with(
        &conn,
        DAY,
        "2026-10-06T12:00:00Z",
        GapAction::Personal,
        HOURS,
    );
    assert!(err.is_err());
    assert_eq!(blocks(&conn), before);
}

// ───────────────────────── inside the 17:00 run (FR-45) ─────────────────────────

fn local(hour: u32, minute: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 10, 6, hour, minute, 0)
        .unwrap()
}

fn tick(conn: &Connection, at: DateTime<FixedOffset>, enabled: bool) {
    let mut send = |_: &Connection, _: &crate::tempo_line_contract::TempoLineKey| Ok(vec![]);
    crate::auto_send::run_if_due(conn, at, enabled, &mut send).unwrap();
}

#[test]
fn the_17_00_run_builds_the_recap_once_and_later_ticks_do_not_rebuild() {
    let _g = utc();
    let conn = open_memory().unwrap();
    tick(&conn, local(16, 45), true);
    assert_eq!(
        latest(&conn).unwrap(),
        None,
        "before 17:00 nothing is built"
    );
    block(&conn, DAY, "09:00", "12:00");
    tick(&conn, local(17, 0), true);
    assert_eq!(latest(&conn).unwrap().unwrap().coverage_percent, 38); // 180/480 = 37.5
    block(&conn, DAY, "12:00", "17:00");
    tick(&conn, local(17, 15), true);
    assert_eq!(latest(&conn).unwrap().unwrap().coverage_percent, 38);
}

#[test]
fn switched_off_builds_no_recap() {
    let _g = utc();
    let conn = open_memory().unwrap();
    block(&conn, DAY, "09:00", "12:00");
    tick(&conn, local(17, 0), false);
    assert_eq!(latest(&conn).unwrap(), None);
}

#[test]
fn an_unreachable_tempo_defers_the_recap_to_the_tick_that_finishes() {
    let _g = utc();
    let conn = open_memory().unwrap();
    let ready = Line {
        text: Some(("generated", Some("passed"))),
        ..line("R-1")
    };
    seed(&conn, &ready);
    let mut down = |_: &Connection, _: &crate::tempo_line_contract::TempoLineKey| {
        Err(anyhow::anyhow!("offline"))
    };
    crate::auto_send::run_if_due(&conn, local(17, 0), true, &mut down).unwrap();
    assert_eq!(latest(&conn).unwrap(), None);
    tick(&conn, local(17, 15), true);
    assert!(latest(&conn).unwrap().is_some());
}
