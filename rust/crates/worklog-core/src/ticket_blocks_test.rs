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
    // a lone 40-minute block on another day rounds up to 1h.
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
        (2400, 3600)
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

fn seed_remote(conn: &Connection, id: &str, day: &str, issue: i64, secs: i64, owner: &str) {
    conn.execute(
        "INSERT INTO tempo_remote_worklogs
            (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            day,
            issue,
            secs,
            owner,
            format!("2026-10-02T08:0{id}:00Z")
        ],
    )
    .unwrap();
}

fn seed_ticket(conn: &Connection, key: &str, issue_id: Option<&str>) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id) VALUES (?1, 's', ?2)",
        params![key, issue_id],
    )
    .unwrap();
}

#[test]
fn nothing_pulled_means_no_totals() {
    let conn = open_memory().unwrap();
    seed_ticket(&conn, "APRO-1", Some("10001"));
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    let out = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap();
    assert_eq!((out.in_tempo_total_seconds, out.pulled_at), (None, None));
    assert_eq!(out.today.day, "2026-10-02");
    assert_eq!(out.today.in_tempo_seconds, None);
    assert_eq!(out.today.ticket_in_tempo_seconds, None);
}

#[test]
fn in_tempo_total_sums_every_pulled_day_for_the_issue_and_reports_latest_pull() {
    let conn = open_memory().unwrap();
    seed_ticket(&conn, "APRO-1", Some("10001"));
    seed_ticket(&conn, "APRO-2", None);
    seed_ticket(&conn, "APRO-3", Some("10003"));
    seed_remote(&conn, "1", "2026-10-02", 10001, 1800, "worklog");
    seed_remote(&conn, "2", "2026-09-01", 10001, 3600, "worklog");
    seed_remote(&conn, "3", "2026-10-01", 10002, 9999, "outside");
    let total = |key| ticket_blocks(&conn, key, today(), 14).unwrap();
    let out = total("APRO-1");
    assert_eq!(out.in_tempo_total_seconds, Some(5400));
    assert_eq!(out.pulled_at.as_deref(), Some("2026-10-02T08:03:00Z"));
    // Pulled table has rows but none for this issue: a real zero.
    assert_eq!(total("APRO-3").in_tempo_total_seconds, Some(0));
    // No issue id, or unknown ticket: unknown.
    assert_eq!(total("APRO-2").in_tempo_total_seconds, None);
    assert_eq!(total("NOPE-9").in_tempo_total_seconds, None);
}

#[test]
fn today_totals_union_overlaps_and_skip_personal_and_ignored() {
    let conn = open_memory().unwrap();
    seed_ticket(&conn, "APRO-1", Some("10001"));
    // APRO-1 09:00-10:00 and 09:30-10:30 overlap (90 min); APRO-2 10:00-11:00
    // extends the run to 11:00; an unticketed block 13:00-14:00 adds an hour.
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    seed(&conn, "2026-10-02", "APRO-1", "09:30", "b");
    seed(&conn, "2026-10-02", "APRO-2", "10:00", "c");
    let loose = seed(&conn, "2026-10-02", "APRO-9", "13:00", "d");
    conn.execute("UPDATE blocks SET jira_issue = NULL WHERE id = ?1", [loose])
        .unwrap();
    let personal = seed(&conn, "2026-10-02", "APRO-1", "15:00", "p");
    conn.execute(
        "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
        [personal],
    )
    .unwrap();
    let ignored = seed(&conn, "2026-10-02", "APRO-1", "16:00", "i");
    conn.execute(
        "UPDATE blocks SET ignored_at = '2026-10-02T17:00:00Z' WHERE id = ?1",
        [ignored],
    )
    .unwrap();
    seed(&conn, "2026-10-01", "APRO-1", "09:00", "yesterday");
    let t = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap().today;
    assert_eq!(t.worked_seconds, 3600 * 3); // 09:00-11:00 + 13:00-14:00
    assert_eq!(t.ticket_worked_seconds, 5400);
}

#[test]
fn today_in_tempo_is_any_owner_sum_and_ticket_share_is_separate() {
    let conn = open_memory().unwrap();
    seed_ticket(&conn, "APRO-1", Some("10001"));
    seed(&conn, "2026-10-02", "APRO-1", "09:00", "a");
    seed_remote(&conn, "1", "2026-10-02", 10001, 1800, "worklog");
    seed_remote(&conn, "2", "2026-10-02", 10002, 600, "outside");
    seed_remote(&conn, "3", "2026-10-01", 10001, 7200, "worklog");
    let t = ticket_blocks(&conn, "APRO-1", today(), 14).unwrap().today;
    assert_eq!(
        (t.in_tempo_seconds, t.ticket_in_tempo_seconds),
        (Some(2400), Some(1800))
    );
    // A pulled day with no blocks: zero worked, never an error.
    let yesterday = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    let other = ticket_blocks(&conn, "NOPE-9", yesterday, 14).unwrap().today;
    assert_eq!(
        (other.in_tempo_seconds, other.worked_seconds),
        (Some(7200), 0)
    );
}
