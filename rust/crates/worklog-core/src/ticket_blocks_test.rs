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
fn unknown_key_has_no_days() {
    let conn = open_memory().unwrap();
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    let out = ticket_blocks(&conn, "NOPE-9", today(), 14).unwrap();
    assert!(out.days.is_empty());
    assert_eq!(out.key, "NOPE-9");
}
