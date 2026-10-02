use super::*;
use crate::db::open_memory;
use crate::estimate::{estimate_day_with, FixedInvoker};
use crate::infer::{build_blocks, load_day_events, persist_blocks};
use crate::models::Event;
use crate::tempo_hub_contract::HubError;
use crate::tempo_line_contract::TicketOrigin;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()
}

fn body(day: &str, start: &str, minutes: i64, description: &str) -> LogTimeBody {
    LogTimeBody {
        day: day.into(),
        start: start.into(),
        minutes,
        description: description.into(),
    }
}

fn rejected(conn: &Connection, b: LogTimeBody) -> String {
    let err = log_time(conn, "APRO-1", &b, today()).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<HubError>(),
        Some(HubError::InvalidInput(_))
    ));
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0, "nothing written on a rejected request");
    err.to_string()
}

#[test]
fn creates_a_manual_ticket_locked_block() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    let block = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "09:30", 90, "  Wrote the thing  "),
        today(),
    )
    .unwrap();
    assert_eq!(block.day, "2026-04-18");
    assert_eq!(block.jira_issue.as_deref(), Some("APRO-1"));
    assert_eq!(block.started_at, "2026-04-18T09:30:00+00:00");
    assert_eq!(block.ended_at, "2026-04-18T11:00:00+00:00");
    assert_eq!(block.duration_seconds, 5400);
    assert_eq!(block.description.as_deref(), Some("Wrote the thing"));
    assert_eq!(block.estimated_by.as_deref(), Some("manual"));
    assert_eq!(block.ticket_origin, Some(TicketOrigin::Manual));
    assert!(!block.is_personal && block.tempo_worklog_id.is_none());
    assert_eq!(repo::get_block(&conn, block.id).unwrap(), Some(block));
}

#[test]
fn start_is_local_time_converted_to_utc() {
    let _g = crate::tz::test_env_lock();
    std::env::set_var("WORKLOG_TZ", "+02:00");
    let conn = open_memory().unwrap();
    let block = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "00:30", 30, "x"),
        today(),
    );
    std::env::remove_var("WORKLOG_TZ");
    let block = block.unwrap();
    assert_eq!(block.day, "2026-04-18");
    assert_eq!(block.started_at, "2026-04-17T22:30:00+00:00");
    assert_eq!(block.ended_at, "2026-04-17T23:00:00+00:00");
}

#[test]
fn bad_requests_are_rejected_before_any_write() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    rejected(&conn, body("soon", "09:00", 30, "x"));
    rejected(&conn, body("2026-04-19", "09:00", 30, "x"));
    rejected(&conn, body("2026-04-18", "9am", 30, "x"));
    rejected(&conn, body("2026-04-18", "25:00", 30, "x"));
    rejected(&conn, body("2026-04-18", "09:00", 0, "x"));
    rejected(&conn, body("2026-04-18", "09:00", 721, "x"));
    rejected(&conn, body("2026-04-18", "09:00", 30, "   "));
    rejected(&conn, body("2026-04-18", "09:00", 30, &"a".repeat(501)));
    log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "09:00", 720, &"a".repeat(500)),
        today(),
    )
    .unwrap();
}

// -- A3: a hand-logged block survives every path that rebuilds a day --

fn seed_events(conn: &Connection) {
    for (id, at) in [("e1", "09:00:00"), ("e2", "09:05:00")] {
        repo::upsert_event(
            conn,
            &Event::minimal("github_commit", id, format!("2026-04-18T{at}+00:00"), id),
        )
        .unwrap();
    }
}

fn rebuild(conn: &Connection) {
    let day = today();
    let blocks = build_blocks(load_day_events(conn, day).unwrap());
    persist_blocks(conn, day, &blocks).unwrap();
}

fn manual_row(conn: &Connection, id: i64) -> Block {
    repo::get_block(conn, id)
        .unwrap()
        .expect("manual block survived")
}

#[test]
fn re_inference_keeps_the_block_even_when_the_day_has_no_events() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    let made = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "13:00", 60, "Meeting"),
        today(),
    )
    .unwrap();
    persist_blocks(&conn, today(), &[]).unwrap();
    assert_eq!(manual_row(&conn, made.id), made);
}

#[test]
fn re_inference_neither_merges_nor_leaks_state_into_an_overlapping_inferred_block() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    seed_events(&conn);
    let made = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "09:00", 60, "Hand logged"),
        today(),
    )
    .unwrap();
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '777' WHERE id = ?1",
        [made.id],
    )
    .unwrap();
    let before = manual_row(&conn, made.id);
    rebuild(&conn);
    rebuild(&conn);
    assert_eq!(manual_row(&conn, made.id), before);
    let all = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
    assert_eq!(all.len(), 2, "inferred block stays a separate block");
    let inferred = all.iter().find(|b| b.id != made.id).unwrap();
    assert_eq!(inferred.jira_issue, None);
    assert_eq!(inferred.description, None);
    assert_eq!(inferred.tempo_worklog_id, None);
    assert_eq!(inferred.estimated_by, None);
}

#[test]
fn auto_merge_and_estimate_leave_the_block_alone() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    let first = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "09:00", 30, "One"),
        today(),
    )
    .unwrap();
    let second = log_time(
        &conn,
        "APRO-1",
        &body("2026-04-18", "09:30", 30, "Two"),
        today(),
    )
    .unwrap();
    assert_eq!(
        crate::estimate::merge_same_ticket_adjacent(&conn, "2026-04-18").unwrap(),
        0
    );
    let invoker = FixedInvoker(serde_json::json!({
        "jira_issue": "APRO-9", "minutes": 5, "description": "AI"
    }));
    estimate_day_with(&conn, today(), "m", &invoker).unwrap();
    assert_eq!(manual_row(&conn, first.id), first);
    assert_eq!(manual_row(&conn, second.id), second);
}
