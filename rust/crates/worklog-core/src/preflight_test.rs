//! Tests for T023 — preflight checklist and read-back (FR-11, FR-12, FR-14, B5).

use super::*;
use crate::db;
use rusqlite::params;

const DAY: &str = "2026-10-01";

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

/// Inserts a block starting at `hour:minute` UTC lasting `seconds`; returns its id.
fn block(
    conn: &Connection,
    issue: Option<&str>,
    hour: u32,
    minute: u32,
    seconds: i64,
    text: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
        params![
            DAY,
            issue,
            format!("{DAY}T{hour:02}:{minute:02}:00+00:00"),
            seconds,
            text
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn required(conn: &Connection, seconds: i64) {
    conn.execute(
        "INSERT INTO tempo_required_days (day, required_seconds, pulled_at) VALUES (?1, ?2, 'x')",
        params![DAY, seconds],
    )
    .unwrap();
}

fn remote(conn: &Connection, id: &str, seconds: i64, owner: &str) {
    conn.execute(
        "INSERT INTO tempo_remote_worklogs (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
         VALUES (?1, ?2, 1, ?3, ?4, 'x')",
        params![id, DAY, seconds, owner],
    )
    .unwrap();
}

fn rows(conn: &Connection) -> Vec<PreflightRow> {
    check(conn, date(DAY), date(DAY)).unwrap()
}

fn of(rows: &[PreflightRow], kind: PreflightCheck) -> Vec<&PreflightRow> {
    rows.iter().filter(|r| r.check == kind).collect()
}

fn red(rows: &[PreflightRow]) -> Vec<&PreflightRow> {
    rows.iter().filter(|r| !r.ok).collect()
}

fn clean_day(conn: &Connection) {
    block(conn, Some("GEN-1"), 9, 0, 3600, "work");
    required(conn, 3600);
}

#[test]
fn clean_day_is_one_green_row_per_check_without_target() {
    let conn = db::open_memory().unwrap();
    clean_day(&conn);
    let rows = rows(&conn);
    let kinds: Vec<_> = rows.iter().map(|r| r.check).collect();
    assert_eq!(
        kinds,
        vec![
            PreflightCheck::Ticketed,
            PreflightCheck::NoDoubleCount,
            PreflightCheck::DayHours,
            PreflightCheck::LineText
        ]
    );
    assert!(rows.iter().all(|r| r.ok && r.target.is_none()));
}

#[test]
fn one_failure_per_check_gives_four_red_rows_naming_the_fault() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "work");
    block(&conn, Some("GEN-2"), 9, 30, 3600, ""); // overlaps GEN-1, no text
    let untagged = block(&conn, None, 14, 0, 1800, "x");
    required(&conn, 8 * 3600);
    let rows = rows(&conn);
    assert_eq!(red(&rows).len(), 4);
    let target = |k| of(&rows, k)[0].target.clone().unwrap();
    assert_eq!(target(PreflightCheck::Ticketed), untagged.to_string());
    assert_eq!(target(PreflightCheck::NoDoubleCount), DAY);
    assert_eq!(target(PreflightCheck::DayHours), DAY);
    assert_eq!(target(PreflightCheck::LineText), format!("{DAY} GEN-2"));
    // FR-12: the row text itself carries the id/day/line, not just the target field
    for row in red(&rows) {
        assert!(
            row.detail.contains(row.target.as_deref().unwrap()),
            "{row:?}"
        );
    }
}

#[test]
fn empty_string_ticket_is_as_ticketless_as_null() {
    let conn = db::open_memory().unwrap();
    let id = block(&conn, Some(""), 12, 0, 3600, "x");
    let rows = rows(&conn);
    assert_eq!(
        of(&rows, PreflightCheck::Ticketed)[0].target,
        Some(id.to_string())
    ); // treating only NULL as missing passes it
}

#[test]
fn each_ticketless_block_gets_its_own_red_row() {
    let conn = db::open_memory().unwrap();
    clean_day(&conn);
    let a = block(&conn, None, 12, 0, 1800, "x");
    let b = block(&conn, None, 13, 0, 1800, "x");
    let rows = rows(&conn);
    let targets: Vec<_> = of(&rows, PreflightCheck::Ticketed)
        .iter()
        .map(|r| r.target.clone().unwrap())
        .collect();
    assert_eq!(targets, vec![a.to_string(), b.to_string()]); // one row for all hides the second
}

#[test]
fn personal_block_needs_no_ticket() {
    let conn = db::open_memory().unwrap();
    clean_day(&conn);
    let id = block(&conn, None, 12, 0, 1800, "x");
    conn.execute("UPDATE blocks SET is_personal = 1 WHERE id = ?1", [id])
        .unwrap();
    assert!(red(&rows(&conn)).is_empty()); // a check that ignores is_personal flags it
}

#[test]
fn overlap_within_one_ticket_is_not_double_counted() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    block(&conn, Some("GEN-1"), 9, 30, 3600, "b");
    required(&conn, 5400);
    assert!(red(&rows(&conn)).is_empty()); // same ticket is unioned into one line
}

#[test]
fn back_to_back_blocks_on_two_tickets_are_not_double_counted() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    block(&conn, Some("GEN-2"), 10, 0, 3600, "b"); // `<=` instead of `<` flags the touch
    required(&conn, 7200);
    assert!(red(&rows(&conn)).is_empty());
}

#[test]
fn one_second_overlap_across_tickets_is_double_counted() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3601, "a");
    block(&conn, Some("GEN-2"), 10, 0, 3600, "b");
    required(&conn, 7200);
    let rows = rows(&conn);
    assert_eq!(
        of(&rows, PreflightCheck::NoDoubleCount)[0]
            .target
            .as_deref(),
        Some(DAY)
    );
}

#[test]
fn day_hours_at_required_is_green_and_half_hour_short_is_red() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    required(&conn, 3600);
    assert!(red(&rows(&conn)).is_empty()); // `<=` instead of `<`
    conn.execute("UPDATE tempo_required_days SET required_seconds = 5400", [])
        .unwrap();
    let rows = rows(&conn);
    assert_eq!(
        of(&rows, PreflightCheck::DayHours)[0].target.as_deref(),
        Some(DAY)
    );
}

#[test]
fn day_hours_use_rounded_line_hours_not_raw_seconds() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 1000, "a"); // bills 1800
    required(&conn, 1800);
    assert!(red(&rows(&conn)).is_empty()); // raw sum 1000 < 1800 would flag
}

#[test]
fn day_hours_count_entries_logged_outside_worklog() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    required(&conn, 7200);
    remote(&conn, "o1", 3600, "outside");
    assert!(red(&rows(&conn)).is_empty()); // ignoring outside time reports a false shortfall
}

#[test]
fn day_without_required_row_or_with_zero_required_is_green() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    assert!(red(&rows(&conn)).is_empty()); // missing must not read as a shortfall
    required(&conn, 0);
    assert!(red(&rows(&conn)).is_empty());
}

#[test]
fn stored_text_satisfies_a_line_with_blank_descriptions() {
    let conn = db::open_memory().unwrap();
    clean_day(&conn);
    block(&conn, Some("GEN-2"), 12, 0, 3600, "");
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, text, text_origin, updated_at)
         VALUES (?1, 'GEN-2', 'wrote it', 'manual', 'x')",
        [DAY],
    )
    .unwrap();
    let rows = check(&conn, date(DAY), date(DAY)).unwrap();
    assert!(of(&rows, PreflightCheck::LineText).iter().all(|r| r.ok));
}

#[test]
fn range_is_inclusive_at_both_ends() {
    let conn = db::open_memory().unwrap();
    clean_day(&conn);
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES ('2026-10-02', NULL, '2026-10-02T09:00:00+00:00', '2026-10-02T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    // `to` exclusive would miss the 10-02 fault
    assert_eq!(
        red(&check(&conn, date(DAY), date("2026-10-02")).unwrap()).len(),
        1
    );
    // `from` exclusive would miss it too
    assert_eq!(
        red(&check(&conn, date("2026-10-02"), date("2026-10-03")).unwrap()).len(),
        1
    );
    assert!(red(&check(&conn, date(DAY), date(DAY)).unwrap()).is_empty());
}

#[test]
fn read_back_matches_when_tempo_total_equals_sent() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    remote(&conn, "w1", 3600, "worklog");
    let row = read_back(&conn, date(DAY)).unwrap();
    assert_eq!(
        (row.check, row.ok, row.target),
        (PreflightCheck::ReadBack, true, None)
    );
}

#[test]
fn read_back_mismatch_is_red_and_names_the_day() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    remote(&conn, "w1", 1800, "worklog");
    let row = read_back(&conn, date(DAY)).unwrap();
    assert!(!row.ok);
    assert_eq!(row.target.as_deref(), Some(DAY));
    assert!(row.detail.contains(DAY));
}

#[test]
fn read_back_is_red_when_nothing_came_back() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    assert!(!read_back(&conn, date(DAY)).unwrap().ok); // missing must not read as match
}

#[test]
fn read_back_ignores_outside_entries() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    remote(&conn, "w1", 3600, "worklog");
    remote(&conn, "o1", 7200, "outside");
    assert!(read_back(&conn, date(DAY)).unwrap().ok); // summing every owner mismatches
}

fn mark_first_line_already(conn: &Connection) {
    let line = &tempo_lines::lines_for_day(conn, DAY).unwrap()[0];
    crate::tempo_match::mark_already(conn, line, "o1").unwrap();
}

#[test]
fn an_already_in_tempo_line_is_not_counted_on_top_of_its_outside_entry() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    block(&conn, Some("GEN-2"), 10, 0, 3600, "b");
    required(&conn, 7200);
    remote(&conn, "o1", 3600, "outside");
    mark_first_line_already(&conn);
    assert!(red(&rows(&conn)).is_empty());
    // counting the marked line too would hide this shortfall (10800 >= 9000)
    conn.execute("UPDATE tempo_required_days SET required_seconds = 9000", [])
        .unwrap();
    assert_eq!(red(&rows(&conn)).len(), 1);
}

#[test]
fn read_back_leaves_out_an_already_in_tempo_line() {
    let conn = db::open_memory().unwrap();
    block(&conn, Some("GEN-1"), 9, 0, 3600, "a");
    remote(&conn, "o1", 3600, "outside");
    mark_first_line_already(&conn);
    // sent 3600 against 0 worklog-owned seconds would be a false red
    assert!(read_back(&conn, date(DAY)).unwrap().ok);
}
