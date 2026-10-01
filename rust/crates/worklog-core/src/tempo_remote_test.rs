//! Tests for T004 — stored Tempo read-back and owner classification (spec 012, B8).

use super::*;
use crate::db;
use rusqlite::params;

const PULLED_AT: &str = "2026-10-01T09:00:00Z";

fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
}

fn next_monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
}

fn pulled(id: &str, day: &str, issue_id: i64, seconds: i64) -> PulledWorklog {
    PulledWorklog {
        tempo_worklog_id: id.to_string(),
        day: day.to_string(),
        issue_id,
        seconds,
        description: format!("work {id}"),
    }
}

fn required(day: &str, seconds: i64) -> RequiredDay {
    RequiredDay {
        day: day.to_string(),
        required_seconds: seconds,
    }
}

fn seed_block(conn: &Connection, tempo_id: Option<&str>) {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, tempo_worklog_id)
         VALUES ('2026-09-29', '2026-09-29T09:00:00Z', '2026-09-29T10:00:00Z', 3600, ?1)",
        params![tempo_id],
    )
    .unwrap();
}

fn seed_ticket(conn: &Connection, key: &str, issue_id: &str) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id) VALUES (?1, 'summary', ?2)",
        params![key, issue_id],
    )
    .unwrap();
}

fn required_days(conn: &Connection) -> Vec<(String, i64)> {
    conn.prepare("SELECT day, required_seconds FROM tempo_required_days ORDER BY day")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn classifies_owner_by_block_tempo_worklog_id() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, Some("11"));
    seed_block(&conn, Some("11"));
    seed_block(&conn, None);
    let rows = [
        pulled("11", "2026-09-29", 1, 3600),
        pulled("12", "2026-09-29", 1, 1800),
    ];
    let report = store_week(&conn, monday(), &rows, &[], PULLED_AT).unwrap();

    let got = list_week(&conn, monday()).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].tempo_worklog_id, "11");
    assert_eq!(got[0].owner, WorklogOwner::Worklog);
    assert_eq!(got[1].tempo_worklog_id, "12");
    assert_eq!(got[1].owner, WorklogOwner::Outside);
    assert_eq!((report.worklogs, report.outside), (2, 1));
}

#[test]
fn resolves_jira_issue_from_cached_ticket_issue_id() {
    let conn = db::open_memory().unwrap();
    seed_ticket(&conn, "APRO-7", "10007");
    let rows = [
        pulled("1", "2026-09-29", 10007, 3600),
        pulled("2", "2026-09-29", 99999, 1800),
    ];
    store_week(&conn, monday(), &rows, &[], PULLED_AT).unwrap();

    let got = list_week(&conn, monday()).unwrap();
    assert_eq!(got[0].jira_issue.as_deref(), Some("APRO-7"));
    assert_eq!(got[1].jira_issue, None);
    assert_eq!(got[0].seconds, 3600);
    assert_eq!(got[0].description, "work 1");
}

#[test]
fn repull_replaces_only_that_weeks_rows() {
    let conn = db::open_memory().unwrap();
    let first = [
        pulled("1", "2026-09-27", 1, 600),
        pulled("2", "2026-09-28", 1, 600),
    ];
    let schedule = [required("2026-09-27", 1), required("2026-10-04", 2)];
    store_week(&conn, monday(), &first, &schedule, PULLED_AT).unwrap();
    store_week(
        &conn,
        next_monday(),
        &[pulled("3", "2026-10-05", 1, 600)],
        &[],
        PULLED_AT,
    )
    .unwrap();

    let report = store_week(
        &conn,
        monday(),
        &[pulled("4", "2026-10-04", 1, 600)],
        &[required("2026-09-29", 28800)],
        PULLED_AT,
    )
    .unwrap();

    let ids: Vec<String> = list_week(&conn, monday())
        .unwrap()
        .into_iter()
        .map(|w| w.tempo_worklog_id)
        .collect();
    assert_eq!(
        ids,
        ["4"],
        "id 2 was in the week and is gone; id 1 is Sunday before"
    );
    let all: i64 = conn
        .query_row("SELECT COUNT(*) FROM tempo_remote_worklogs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(all, 3, "rows 1 (prior Sunday), 3 (next week) and 4 survive");
    assert_eq!(list_week(&conn, next_monday()).unwrap().len(), 1);
    assert_eq!(
        required_days(&conn),
        [
            ("2026-09-27".to_string(), 1),
            ("2026-09-29".to_string(), 28800)
        ]
    );
    assert_eq!(report.schedule_days, 1);
    assert_eq!(report.monday, "2026-09-28");
    assert_eq!(report.pulled_at, PULLED_AT);
}

#[test]
fn outside_exists_matches_day_issue_and_owner() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, Some("1"));
    let rows = [
        pulled("1", "2026-09-29", 5, 600),
        pulled("2", "2026-09-30", 6, 600),
    ];
    store_week(&conn, monday(), &rows, &[], PULLED_AT).unwrap();

    assert!(outside_exists(&conn, "2026-09-30", 6).unwrap());
    assert!(
        !outside_exists(&conn, "2026-09-29", 5).unwrap(),
        "worklog-owned row is not outside"
    );
    assert!(!outside_exists(&conn, "2026-09-30", 5).unwrap());
    assert!(!outside_exists(&conn, "2026-10-01", 6).unwrap());
}

#[test]
fn repull_is_idempotent_when_rows_move_weeks_or_fall_outside() {
    let conn = db::open_memory().unwrap();
    let first = [pulled("9", "2026-09-27", 5, 600)];
    store_week(&conn, monday(), &first, &[], PULLED_AT).unwrap();

    let moved = [
        pulled("9", "2026-09-29", 5, 900),
        pulled("8", "2026-09-27", 5, 300),
    ];
    let sched = [required("2026-10-06", 100)];
    store_week(&conn, monday(), &moved, &sched, PULLED_AT).unwrap();
    store_week(&conn, monday(), &moved, &sched, PULLED_AT).unwrap();
    store_week(&conn, next_monday(), &[], &sched, PULLED_AT).unwrap();

    let week = list_week(&conn, monday()).unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!(
        (week[0].tempo_worklog_id.as_str(), week[0].seconds),
        ("9", 900)
    );
    assert_eq!(required_days(&conn), vec![("2026-10-06".to_string(), 100)]);
}
