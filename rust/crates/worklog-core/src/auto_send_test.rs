//! Tests for T009 — auto-send at 17:00, readiness, review and confirm (spec 017 FR-26..FR-35).

use super::*;
use crate::collectors::tempo::SyncResult;
use crate::db::open_memory;
use crate::verdict_contract::{DecisionKind, DecisionRow, DecisionSource};
use chrono::{FixedOffset, TimeZone};
use rusqlite::params;

const DAY: &str = "2026-10-06";
const PASSED: Option<(&str, Option<&str>)> = Some(("generated", Some("passed")));

fn local(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .unwrap()
}

/// One 30 min block plus, when `text` is given, the line's stored text row
/// as `(origin, check_status)`.
fn seed(
    conn: &Connection,
    day: &str,
    issue: &str,
    ticket_origin: Option<&str>,
    text: Option<(&str, Option<&str>)>,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, ticket_origin)
         VALUES (?1, ?2, ?3, ?3, 1800, ?4)",
        params![day, issue, format!("{day}T10:00:00Z"), ticket_origin],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    if let Some((origin, check)) = text {
        conn.execute(
            "INSERT OR IGNORE INTO tempo_line_texts
               (day, jira_issue, text, text_origin, check_status, updated_at)
             VALUES (?1, ?2, 'Fixed the login redirect', ?3, ?4, '2026-10-06T00:00:00Z')",
            params![day, issue, origin, check],
        )
        .unwrap();
    }
    id
}

fn decide(conn: &Connection, block: i64, source: DecisionSource, chosen: Option<&str>, at: &str) {
    crate::verdict_decisions::record(
        conn,
        &DecisionRow {
            kind: DecisionKind::Ticket,
            source,
            subject: block.to_string(),
            state_json: "{}".into(),
            options: vec![],
            ranking: None,
            chosen: chosen.map(str::to_string),
            previous: None,
            decided_at: at.into(),
        },
    )
    .unwrap();
}

fn set(conn: &Connection, day: &str, issue: &str, assignment: &str) {
    conn.execute(
        &format!("UPDATE tempo_line_texts SET {assignment} WHERE day = ?1 AND jira_issue = ?2"),
        params![day, issue],
    )
    .unwrap();
}

fn column(conn: &Connection, day: &str, issue: &str, column: &str) -> Option<String> {
    conn.query_row(
        &format!("SELECT {column} FROM tempo_line_texts WHERE day = ?1 AND jira_issue = ?2"),
        params![day, issue],
        |r| r.get(0),
    )
    .unwrap()
}

// ───────────────────────── readiness (FR-28, FR-34) ─────────────────────────

#[test]
fn readiness_follows_ticket_origin_and_text_check() {
    let conn = open_memory().unwrap();
    seed(&conn, DAY, "R-OWNER", Some("manual"), PASSED);
    seed(&conn, DAY, "R-EVENT", Some("event"), PASSED);
    let picked = seed(&conn, DAY, "R-PICK", Some("auto"), PASSED);
    decide(
        &conn,
        picked,
        DecisionSource::Verdict,
        Some("R-PICK"),
        "2026-10-06T08:00:00Z",
    );
    seed(
        &conn,
        DAY,
        "R-HANDTEXT",
        Some("manual"),
        Some(("manual", None)),
    );
    let dirty = seed(&conn, DAY, "R-EDITED", Some("manual"), PASSED);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '9', dirty = 1 WHERE id = ?1",
        [dirty],
    )
    .unwrap();

    seed(&conn, DAY, "N-CLOUD", Some("auto"), PASSED);
    let other = seed(&conn, DAY, "N-OTHER", Some("auto"), PASSED);
    decide(
        &conn,
        other,
        DecisionSource::Verdict,
        Some("X-1"),
        "2026-10-06T08:00:00Z",
    );
    let abstained = seed(&conn, DAY, "N-ABSTAINED", Some("auto"), PASSED);
    decide(
        &conn,
        abstained,
        DecisionSource::Verdict,
        None,
        "2026-10-06T08:00:00Z",
    );
    let overridden = seed(&conn, DAY, "N-OWNERLATEST", Some("auto"), PASSED);
    decide(
        &conn,
        overridden,
        DecisionSource::Verdict,
        Some("N-OWNERLATEST"),
        "2026-10-06T08:00:00Z",
    );
    decide(
        &conn,
        overridden,
        DecisionSource::Owner,
        Some("N-OWNERLATEST"),
        "2026-10-06T09:00:00Z",
    );
    let no_origin = seed(&conn, DAY, "N-NOORIGIN", None, PASSED);
    decide(
        &conn,
        no_origin,
        DecisionSource::Verdict,
        Some("N-NOORIGIN"),
        "2026-10-06T08:00:00Z",
    );
    seed(
        &conn,
        DAY,
        "N-NEEDSLOOK",
        Some("manual"),
        Some(("generated", Some("needs_look"))),
    );
    seed(
        &conn,
        DAY,
        "N-UNCHECKED",
        Some("manual"),
        Some(("generated", None)),
    );
    seed(&conn, DAY, "N-NOTEXT", Some("manual"), None);
    seed(&conn, DAY, "N-SENT", Some("manual"), PASSED);
    set(
        &conn,
        DAY,
        "N-SENT",
        "auto_sent_at = '2026-10-05T17:00:00Z'",
    );
    let synced = seed(&conn, DAY, "N-SYNCED", Some("manual"), PASSED);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '9' WHERE id = ?1",
        [synced],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, ticket_origin)
         VALUES (?1, 'N-ZERO', ?2, ?2, 600, 'manual')",
        params![DAY, format!("{DAY}T12:00:00Z")],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, text, text_origin, check_status, updated_at)
         VALUES (?1, 'N-ZERO', 't', 'generated', 'passed', 'x')",
        [DAY],
    )
    .unwrap();

    let ready: Vec<String> = ready_lines(&conn, DAY)
        .unwrap()
        .into_iter()
        .map(|l| l.jira_issue)
        .collect();

    // Ready, by wrong implementation: R-OWNER/R-EVENT missing = only auto ever ready;
    // R-PICK missing = auto never ready; R-HANDTEXT missing = Owner text checked;
    // R-EDITED missing = a dirty synced line is still unsent work.
    // Not ready: N-CLOUD auto with no Verdict row (trusting origin alone); N-OTHER chosen
    // differs (checking only that a Verdict row exists); N-ABSTAINED (chosen None);
    // N-OWNERLATEST (matching any source, not the latest); N-NOORIGIN (NULL as Verdict-picked);
    // N-NEEDSLOOK, N-UNCHECKED (check optional); N-NOTEXT (no text row); N-SENT (second send,
    // FR-34); N-SYNCED (nothing left to send); N-ZERO (rounds to 0h, would POST 0s).
    assert_eq!(
        ready,
        ["R-EDITED", "R-EVENT", "R-HANDTEXT", "R-OWNER", "R-PICK"]
    );
}

#[test]
fn one_unready_block_makes_the_whole_line_unready() {
    let conn = open_memory().unwrap();
    let picked = seed(&conn, DAY, "L-1", Some("auto"), PASSED);
    decide(
        &conn,
        picked,
        DecisionSource::Verdict,
        Some("L-1"),
        "2026-10-06T08:00:00Z",
    );
    seed(&conn, DAY, "L-1", Some("auto"), None);
    // catches: judging a line by its first block only
    assert!(ready_lines(&conn, DAY).unwrap().is_empty());
}

// ───────────────────────── the 17:00 run (FR-27, FR-27a, FR-27b) ─────────────────────────

fn ok() -> SyncResult {
    SyncResult {
        block_id: 1,
        status: "synced",
        reason: None,
        tempo_id: Some("77".into()),
        payload: None,
        http_status: Some(200),
    }
}

fn rejected(why: &str) -> SyncResult {
    SyncResult {
        status: "error",
        reason: Some(why.into()),
        tempo_id: None,
        http_status: Some(400),
        ..ok()
    }
}

/// Runs one tick; returns the `day/issue` of every line it tried to send.
fn tick(
    conn: &Connection,
    at: DateTime<FixedOffset>,
    enabled: bool,
    reply: impl Fn(&str) -> Result<Vec<SyncResult>>,
) -> Vec<String> {
    let mut tried = Vec::new();
    run_if_due(conn, at, enabled, &mut |_, key| {
        tried.push(format!("{}/{}", key.day, key.jira_issue));
        reply(&key.jira_issue)
    })
    .unwrap();
    tried
}

fn accepts(_: &str) -> Result<Vec<SyncResult>> {
    Ok(vec![ok()])
}

fn unreachable(_: &str) -> Result<Vec<SyncResult>> {
    Err(anyhow::anyhow!("connection refused"))
}

fn ready_line(conn: &Connection, day: &str, issue: &str) {
    seed(conn, day, issue, Some("manual"), PASSED);
}

#[test]
fn nothing_is_sent_before_17_00_and_the_first_line_goes_at_exactly_17_00() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    let early = tick(&conn, local(6, 16, 59), true, accepts);
    let on_time = tick(&conn, local(6, 17, 0), true, accepts);
    // catches: sending at any hour (early non-empty) and `>` for `>=` (on_time empty)
    assert_eq!((early, on_time), (vec![], vec![format!("{DAY}/A-1")]));
}

#[test]
fn the_17_00_run_sends_a_ready_line_beside_a_red_checklist() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, ?2, 1800)",
        params![DAY, format!("{DAY}T11:00:00Z")],
    )
    .unwrap();
    let day = DAY.parse().unwrap();
    let rows = crate::preflight::check(&conn, day, day).unwrap();
    assert!(rows.iter().any(|r| !r.ok), "precondition: checklist is red");
    // catches: gating the auto-send on the pre-send checklist
    assert_eq!(
        tick(&conn, local(6, 17, 0), true, accepts),
        vec![format!("{DAY}/A-1")]
    );
    assert!(column(&conn, DAY, "A-1", "auto_sent_at").is_some());
}

#[test]
fn a_switched_off_auto_send_sends_nothing() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    // catches: ignoring the Settings switch
    assert!(tick(&conn, local(6, 17, 0), false, accepts).is_empty());
}

#[test]
fn only_the_local_day_is_sent_and_only_once() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    ready_line(&conn, "2026-10-05", "OLD-1");
    let first = tick(&conn, local(6, 17, 0), true, accepts);
    let again = tick(&conn, local(6, 17, 15), true, accepts);
    ready_line(&conn, "2026-10-07", "B-1");
    let next_day = tick(&conn, local(7, 17, 0), true, accepts);
    // catches: sending every day's ready lines (OLD-1 in first), no once-a-day latch
    // (A-1 again), a latch that never resets (next_day empty)
    assert_eq!(
        (first, again, next_day),
        (
            vec![format!("{DAY}/A-1")],
            vec![],
            vec!["2026-10-07/B-1".to_string()]
        )
    );
}

#[test]
fn a_line_ready_after_the_run_waits_for_a_manual_sync() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    tick(&conn, local(6, 17, 0), true, accepts);
    ready_line(&conn, DAY, "LATE-1");
    let later = tick(&conn, local(6, 17, 15), true, accepts);
    // catches: re-planning every tick
    assert!(later.is_empty());
    assert_eq!(column(&conn, DAY, "LATE-1", "auto_sent_at"), None);
}

#[test]
fn an_unreachable_tempo_is_retried_each_tick_with_the_original_plan() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    tick(&conn, local(6, 17, 0), true, unreachable);
    let after_failure = (
        column(&conn, DAY, "A-1", "auto_sent_at"),
        column(&conn, DAY, "A-1", "send_error"),
    );
    ready_line(&conn, DAY, "LATE-1");
    let retry = tick(&conn, local(6, 17, 15), true, accepts);
    // catches: latching the day on a failed run (retry empty), marking Not sent on the first
    // failure (after_failure.1), and re-planning on retry (LATE-1 in retry)
    assert_eq!(
        (after_failure, retry),
        ((None, None), vec![format!("{DAY}/A-1")])
    );
    assert!(column(&conn, DAY, "A-1", "auto_sent_at").is_some());
}

#[test]
fn lines_still_unreachable_when_the_day_ends_are_marked_not_sent() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-1");
    tick(&conn, local(6, 17, 0), true, unreachable);
    let last_tick = tick(&conn, local(6, 23, 45), true, unreachable);
    let marked_early = column(&conn, DAY, "A-1", "send_error");
    let tomorrow = tick(&conn, local(7, 0, 0), true, accepts);
    // catches: giving up before the day is over (marked_early), retrying into the next day
    // (tomorrow), never retrying at the last tick (last_tick empty)
    assert_eq!((last_tick.len(), marked_early, tomorrow), (1, None, vec![]));
    assert_eq!(
        column(&conn, DAY, "A-1", "send_error").as_deref(),
        Some("Tempo could not be reached")
    );
    assert_eq!(column(&conn, DAY, "A-1", "auto_sent_at"), None);
}

#[test]
fn a_rejected_line_records_tempos_message_and_the_others_still_go() {
    let conn = open_memory().unwrap();
    ready_line(&conn, DAY, "A-BAD");
    ready_line(&conn, DAY, "B-GOOD");
    let tried = tick(&conn, local(6, 17, 0), true, |issue| {
        Ok(vec![if issue == "A-BAD" {
            rejected("Issue is closed")
        } else {
            ok()
        }])
    });
    // catches: an error result counted as sent, the message dropped, one rejection aborting
    // the remaining lines
    assert_eq!(tried.len(), 2);
    assert_eq!(
        column(&conn, DAY, "A-BAD", "send_error").as_deref(),
        Some("Issue is closed")
    );
    assert_eq!(column(&conn, DAY, "A-BAD", "auto_sent_at"), None);
    assert!(column(&conn, DAY, "B-GOOD", "auto_sent_at").is_some());
    assert_eq!(column(&conn, DAY, "B-GOOD", "send_error"), None);
}

// ───────────────────────── review and confirm (FR-29..FR-33) ─────────────────────────

fn sent(conn: &Connection, day: &str, issue: &str) {
    ready_line(conn, day, issue);
    set(conn, day, issue, "auto_sent_at = '2026-10-05T17:00:00Z'");
}

fn failed(conn: &Connection, day: &str, issue: &str) {
    ready_line(conn, day, issue);
    set(conn, day, issue, "send_error = 'Issue is closed'");
}

#[test]
fn review_lists_unconfirmed_sent_and_failed_lines_from_earlier_days_newest_first() {
    let conn = open_memory().unwrap();
    sent(&conn, "2026-10-03", "OLD-1");
    sent(&conn, "2026-10-05", "NEW-1");
    set(
        &conn,
        "2026-10-05",
        "NEW-1",
        "hours_override_seconds = 3600",
    );
    failed(&conn, "2026-10-05", "FAIL-1");
    sent(&conn, "2026-10-04", "DONE-1");
    set(
        &conn,
        "2026-10-04",
        "DONE-1",
        "confirmed_at = '2026-10-05T08:00:00Z'",
    );
    ready_line(&conn, "2026-10-05", "WAITING-1");
    sent(&conn, "2026-10-06", "TODAY-1");

    let lines = review_lines(&conn, "2026-10-06").unwrap();

    // catches: oldest day first; confirmed lines listed (DONE-1); today's own lines (`>=` for
    // `>`); lines neither sent nor failed (WAITING-1); seconds from the union instead of the
    // override (1800 for NEW-1); the failure message lost
    let summary: Vec<_> = lines
        .iter()
        .map(|l| {
            (
                l.day.as_str(),
                l.jira_issue.as_str(),
                l.seconds,
                l.status.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "2026-10-05",
                "FAIL-1",
                1800,
                ReviewStatus::NotSent {
                    error: "Issue is closed".into()
                }
            ),
            ("2026-10-05", "NEW-1", 3600, ReviewStatus::Sent),
            ("2026-10-03", "OLD-1", 1800, ReviewStatus::Sent),
        ]
    );
    assert_eq!(lines[1].text, "Fixed the login redirect");
}

#[test]
fn confirm_marks_only_the_named_sent_line() {
    let conn = open_memory().unwrap();
    sent(&conn, "2026-10-05", "A-1");
    sent(&conn, "2026-10-05", "B-1");
    sent(&conn, "2026-10-04", "A-1");

    let n = confirm(&conn, "2026-10-05", Some("A-1")).unwrap();

    // catches: confirming the whole day (B-1), the same ticket on another day, a wrong count
    assert_eq!(n, 1);
    assert!(column(&conn, "2026-10-05", "A-1", "confirmed_at").is_some());
    assert_eq!(column(&conn, "2026-10-05", "B-1", "confirmed_at"), None);
    assert_eq!(column(&conn, "2026-10-04", "A-1", "confirmed_at"), None);
}

#[test]
fn confirm_without_a_ticket_covers_one_day_and_never_a_failed_line() {
    let conn = open_memory().unwrap();
    sent(&conn, "2026-10-05", "A-1");
    sent(&conn, "2026-10-05", "B-1");
    failed(&conn, "2026-10-05", "FAIL-1");
    sent(&conn, "2026-10-04", "C-1");

    let n = confirm(&conn, "2026-10-05", None).unwrap();

    // catches: confirming every day (C-1), confirming a Not sent line and hiding its failure
    assert_eq!(n, 2);
    assert_eq!(column(&conn, "2026-10-05", "FAIL-1", "confirmed_at"), None);
    assert_eq!(column(&conn, "2026-10-04", "C-1", "confirmed_at"), None);
}

#[test]
fn confirm_changes_nothing_in_tempo_state() {
    let conn = open_memory().unwrap();
    let block = seed(&conn, "2026-10-05", "A-1", Some("manual"), PASSED);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '9' WHERE id = ?1",
        [block],
    )
    .unwrap();
    set(
        &conn,
        "2026-10-05",
        "A-1",
        "auto_sent_at = '2026-10-05T17:00:00Z'",
    );
    confirm(&conn, "2026-10-05", Some("A-1")).unwrap();
    // catches: a confirm that marks blocks dirty or clears the Tempo id (never clear it)
    let (id, dirty): (String, i64) = conn
        .query_row(
            "SELECT tempo_worklog_id, dirty FROM blocks WHERE id = ?1",
            [block],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((id.as_str(), dirty), ("9", 0));
}

// ───────────────────────── routes ─────────────────────────

use crate::daemon::{router, state_from_conn, Shared};
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn call(state: &Shared, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test(flavor = "current_thread")]
async fn get_review_and_post_confirm_round_trip() {
    let conn = open_memory().unwrap();
    sent(&conn, "2020-01-02", "A-1");
    sent(&conn, "2020-01-02", "B-1");
    let state = state_from_conn(conn);

    let (status, body) = call(&state, Request::get("/review").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["jira_issue"], "A-1");
    assert_eq!(
        (body[0]["status"].as_str(), body.as_array().unwrap().len()),
        (Some("sent"), 2)
    );

    let post = Request::post("/review/confirm")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"day":"2020-01-02","jira_issue":"A-1"}"#))
        .unwrap();
    let (status, body) = call(&state, post).await;
    assert_eq!(
        (status, body["confirmed"].as_u64()),
        (StatusCode::OK, Some(1))
    );

    let (_, body) = call(&state, Request::get("/review").body(Body::empty()).unwrap()).await;
    // catches: a confirm route that ignores jira_issue (list would be empty)
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["jira_issue"], "B-1");
}
