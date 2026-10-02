use super::*;
use crate::db::open_memory;
use crate::tempo_line_contract::{SetTempoLineHours, SetTempoLineText};
use rusqlite::params;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
}

fn seed(conn: &Connection, day: &str, issue: &str, start: &str, desc: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?3, ?3, 3600, ?4)",
        params![day, issue, format!("{day}T{start}:00Z"), desc],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn window_is_newest_first_and_skips_empty_days() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    seed(&conn, "2026-09-30", "APRO-1", "10:00", "b");
    seed(&conn, "2026-09-30", "APRO-1", "08:00", "early");
    seed(&conn, "2026-09-18", "APRO-1", "09:00", "outside");
    seed(&conn, "2026-09-19", "APRO-1", "09:00", "edge");
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!(
        (out.from.as_str(), out.to.as_str()),
        ("2026-09-19", "2026-10-02")
    );
    let days: Vec<_> = out.days.iter().map(|d| d.day.as_str()).collect();
    assert_eq!(days, ["2026-10-02", "2026-09-30", "2026-09-19"]);
    let order: Vec<_> = out.days[1]
        .blocks
        .iter()
        .map(|b| b.description.clone().unwrap())
        .collect();
    assert_eq!(order, ["early", "b"]);
}

#[test]
fn personal_ignored_and_other_ticket_blocks_are_excluded() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "mine");
    seed(&conn, "2026-10-02", "APRO-2", "10:00", "other");
    let personal = seed(&conn, "2026-10-01", "APRO-1", "09:00", "personal");
    conn.execute(
        "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
        [personal],
    )
    .unwrap();
    let ignored = seed(&conn, "2026-10-01", "APRO-1", "10:00", "ignored");
    conn.execute(
        "UPDATE blocks SET ignored_at = '2026-10-01T12:00:00Z' WHERE id = ?1",
        [ignored],
    )
    .unwrap();
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!(out.days.len(), 1);
    assert_eq!(out.days[0].blocks.len(), 1);
    assert_eq!(out.days[0].blocks[0].description.as_deref(), Some("mine"));
}

#[test]
fn line_text_prefers_stored_over_fallback_and_seconds_honour_override() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "did stuff");
    seed(&conn, "2026-10-01", "APRO-1", "09:00", "fallback only");
    let before = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!(before.days[1].line_text, "fallback only");
    assert_eq!(before.days[1].line_seconds, 3600);
    tempo_lines::set_text(
        &conn,
        &SetTempoLineText {
            day: "2026-10-02".into(),
            jira_issue: "APRO-1".into(),
            text: "Stored text".into(),
        },
    )
    .unwrap();
    tempo_lines::set_hours(
        &conn,
        &SetTempoLineHours {
            day: "2026-10-02".into(),
            jira_issue: "APRO-1".into(),
            seconds: Some(5400),
        },
    )
    .unwrap();
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!(out.days[0].line_text, "Stored text");
    assert_eq!(out.days[0].line_seconds, 5400);
}

#[test]
fn tracked_seconds_is_the_unrounded_union_and_flags_hand_set_hours() {
    let conn = open_memory().unwrap();
    // Two overlapping 40-minute blocks (09:00, 09:20) = 60 min union;
    // a lone 40-minute block on another day rounds down to 30 min.
    for (day, start) in [
        ("2026-10-02", "09:00"),
        ("2026-10-02", "09:20"),
        ("2026-10-01", "09:00"),
    ] {
        let id = seed(&conn, day, "APRO-1", start, "x");
        conn.execute(
            "UPDATE blocks SET duration_seconds = 2400 WHERE id = ?1",
            [id],
        )
        .unwrap();
    }
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!(
        (out.days[0].tracked_seconds, out.days[0].line_seconds),
        (3600, 3600)
    );
    assert_eq!(
        (out.days[1].tracked_seconds, out.days[1].line_seconds),
        (2400, 1800)
    );
    assert!(!out.days[0].hours_set_by_hand && !out.days[1].hours_set_by_hand);
    let hours = SetTempoLineHours {
        day: "2026-10-01".into(),
        jira_issue: "APRO-1".into(),
        seconds: Some(5400),
    };
    tempo_lines::set_hours(&conn, &hours).unwrap();
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert!(out.days[1].hours_set_by_hand && !out.days[0].hours_set_by_hand);
    assert_eq!(
        (out.days[1].tracked_seconds, out.days[1].line_seconds),
        (2400, 5400)
    );
}

#[test]
fn in_tempo_seconds_is_sum_zero_or_none() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    seed(&conn, "2026-10-01", "APRO-1", "09:00", "b");
    seed(&conn, "2026-09-30", "APRO-1", "09:00", "c");
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id) VALUES ('APRO-1', 's', '10001')",
        [],
    )
    .unwrap();
    for (id, day, issue, secs, owner) in [
        ("1", "2026-10-02", 10001, 1800, "worklog"),
        ("2", "2026-10-02", 10001, 600, "outside"),
        ("3", "2026-10-02", 10002, 9999, "outside"),
        ("4", "2026-10-01", 10002, 3600, "outside"),
    ] {
        conn.execute(
            "INSERT INTO tempo_remote_worklogs
                (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '2026-10-02T08:00:00Z')",
            params![id, day, issue, secs, owner],
        )
        .unwrap();
    }
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    let got: Vec<_> = out.days.iter().map(|d| d.in_tempo_seconds).collect();
    assert_eq!(got, [Some(2400), Some(0), None]);
}

#[test]
fn unknown_key_has_no_days() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    let out = ticket_blocks(&conn, "NOPE-9", today(), 14).unwrap();
    assert!(out.days.is_empty());
    assert_eq!(out.key, "NOPE-9");
}
