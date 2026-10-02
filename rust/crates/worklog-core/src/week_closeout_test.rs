//! Tests for T007 — week close-out query (spec 012, B10).

use super::*;
use crate::db;
use rusqlite::params;

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

/// A one-hour block; `hour` keeps same-day blocks from overlapping.
fn block(conn: &Connection, day: &str, issue: &str, hour: u32, worklog_id: &str, dirty: i64) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds,
                             tempo_worklog_id, dirty)
         VALUES (?1, ?2, ?3, ?4, 3600, ?5, ?6)",
        params![
            day,
            issue,
            format!("{day}T{hour:02}:00:00+00:00"),
            format!("{day}T{:02}:00:00+00:00", hour + 1),
            worklog_id,
            dirty
        ],
    )
    .unwrap();
}

fn remote(conn: &Connection, id: &str, day: &str, seconds: i64, owner: &str, pulled_at: &str) {
    conn.execute(
        "INSERT INTO tempo_remote_worklogs
             (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
         VALUES (?1, ?2, 1, ?3, ?4, ?5)",
        params![id, day, seconds, owner, pulled_at],
    )
    .unwrap();
}

fn closeout(conn: &Connection) -> WeekCloseout {
    week_closeout(conn, date("2026-09-28")).unwrap()
}

#[test]
fn empty_week_has_seven_zero_days_monday_to_sunday() {
    let conn = db::open_memory().unwrap();
    let result = closeout(&conn);
    assert_eq!(result.monday, "2026-09-28");
    assert_eq!(result.pulled_at, None);
    let days: Vec<&str> = result.days.iter().map(|d| d.day.as_str()).collect();
    assert_eq!(
        days,
        vec![
            "2026-09-28",
            "2026-09-29",
            "2026-09-30",
            "2026-10-01",
            "2026-10-02",
            "2026-10-03",
            "2026-10-04"
        ]
    );
    for day in &result.days {
        assert_eq!(
            (
                day.logged_seconds,
                day.synced_seconds,
                day.tempo_seconds,
                day.outside_seconds,
                day.unticketed_seconds,
                day.pending_lines
            ),
            (0, 0, 0, 0, 0, 0)
        );
        assert_eq!(day.required_seconds, None);
    }
}

#[test]
fn logged_sums_ticket_lines_and_synced_counts_only_clean_lines() {
    let conn = db::open_memory().unwrap();
    block(&conn, "2026-09-28", "APRO-1", 9, "w1", 0);
    block(&conn, "2026-09-28", "APRO-2", 11, "w2", 1);
    let monday = &closeout(&conn).days[0];
    assert_eq!(monday.logged_seconds, 7200);
    assert_eq!(monday.synced_seconds, 3600);
    assert_eq!(monday.pending_lines, 1);
}

#[test]
fn line_with_one_unsynced_block_is_pending_and_not_synced() {
    let conn = db::open_memory().unwrap();
    block(&conn, "2026-09-29", "APRO-1", 9, "w1", 0);
    block(&conn, "2026-09-29", "APRO-1", 11, "", 0);
    block(&conn, "2026-09-29", "APRO-2", 13, "w2", 0);
    let tuesday = &closeout(&conn).days[1];
    assert_eq!(tuesday.logged_seconds, 10_800);
    assert_eq!(tuesday.synced_seconds, 3600);
    assert_eq!(tuesday.pending_lines, 1);
}

#[test]
fn null_worklog_id_counts_as_unsynced() {
    let conn = db::open_memory().unwrap();
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES ('2026-09-30', 'APRO-1', '2026-09-30T09:00:00+00:00',
                 '2026-09-30T10:00:00+00:00', 3600)",
        [],
    )
    .unwrap();
    let wednesday = &closeout(&conn).days[2];
    assert_eq!((wednesday.synced_seconds, wednesday.pending_lines), (0, 1));
}

#[test]
fn unticketed_sums_raw_seconds_of_untagged_work_blocks_only() {
    let conn = db::open_memory().unwrap();
    conn.execute_batch(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
           VALUES ('2026-10-01', NULL, '2026-10-01T09:00:00+00:00', '2026-10-01T09:30:00+00:00', 1800);
         INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
           VALUES ('2026-10-01', '', '2026-10-01T10:00:00+00:00', '2026-10-01T10:10:00+00:00', 600);
         INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, is_personal)
           VALUES ('2026-10-01', NULL, '2026-10-01T11:00:00+00:00', '2026-10-01T12:00:00+00:00', 3600, 1);
         INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, is_personal, ignored_at)
           VALUES ('2026-10-01', NULL, '2026-10-01T13:00:00+00:00', '2026-10-01T14:00:00+00:00', 3600, 1, '2026-10-01T15:00:00Z');",
    )
    .unwrap();
    block(&conn, "2026-10-01", "APRO-1", 16, "w1", 0);
    let thursday = &closeout(&conn).days[3];
    assert_eq!(thursday.unticketed_seconds, 2400);
    assert_eq!(thursday.logged_seconds, 3600);
}

#[test]
fn tempo_and_outside_seconds_and_latest_pull_come_from_pulled_rows() {
    let conn = db::open_memory().unwrap();
    remote(
        &conn,
        "10",
        "2026-10-02",
        7200,
        "worklog",
        "2026-10-03T08:00:00Z",
    );
    remote(
        &conn,
        "11",
        "2026-10-02",
        1800,
        "outside",
        "2026-10-03T09:00:00Z",
    );
    remote(
        &conn,
        "12",
        "2026-10-09",
        900,
        "outside",
        "2026-10-10T09:00:00Z",
    );
    conn.execute(
        "INSERT INTO tempo_required_days (day, required_seconds, pulled_at)
         VALUES ('2026-10-02', 28800, '2026-10-03T07:00:00Z'), ('2026-10-03', 0, '2026-10-03T07:00:00Z')",
        [],
    )
    .unwrap();
    let result = closeout(&conn);
    let friday = &result.days[4];
    assert_eq!((friday.tempo_seconds, friday.outside_seconds), (9000, 1800));
    assert_eq!(friday.required_seconds, Some(28800));
    assert_eq!(result.days[5].required_seconds, Some(0));
    assert_eq!(result.days[0].required_seconds, None);
    assert_eq!(result.pulled_at.as_deref(), Some("2026-10-03T09:00:00Z"));
}

#[test]
fn rows_outside_the_week_are_ignored() {
    let conn = db::open_memory().unwrap();
    block(&conn, "2026-09-27", "APRO-1", 9, "", 0);
    block(&conn, "2026-10-05", "APRO-1", 9, "", 0);
    let result = closeout(&conn);
    assert!(result.days.iter().all(|d| d.logged_seconds == 0));
}
