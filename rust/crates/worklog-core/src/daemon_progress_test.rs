use super::*;
use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;
use crate::estimate_progress_contract::RawWorklog;
use crate::http;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration, TimeZone};
use httpmock::prelude::*;
use rusqlite::params;
use serde_json::json;
use tower::ServiceExt;

const DAY: &str = "2026-10-09";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 9, 12, 0, 0).unwrap()
}

fn day() -> NaiveDate {
    DAY.parse().unwrap()
}

fn auth(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "a@b.is".into(),
        token: "tok".into(),
    }
}

fn block(conn: &Connection, issue: &str, personal: i64) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, is_personal)
         VALUES (?1, ?2, ?1 || 'T09:00:00Z', ?1 || 'T10:00:00Z', 3600, ?3)",
        params![DAY, issue, personal],
    )
    .unwrap();
}

fn cache(conn: &Connection, key: &str, estimate: i64, seconds: i64, age_secs: i64) {
    let log = RawWorklog {
        worklog_id: format!("c-{key}"),
        account_id: "me".into(),
        name: "Tomas".into(),
        day: day(),
        seconds,
    };
    let at = now() - Duration::seconds(age_secs);
    ticket_progress::store_ticket(conn, key, Some(estimate), &[log], at).unwrap();
}

fn mock_estimates(server: &MockServer, body: serde_json::Value) -> httpmock::Mock<'_> {
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/search/jql");
        then.status(200).json_body(body);
    })
}

fn mock_worklogs<'a>(server: &'a MockServer, key: &str, secs: i64) -> httpmock::Mock<'a> {
    server.mock(|when, then| {
        when.method(GET)
            .path(format!("/rest/api/3/issue/{key}/worklog"));
        then.status(200)
            .json_body(json!({"startAt": 0, "total": 1, "worklogs": [
                {"id": format!("w-{key}"), "author": {"accountId": "me", "displayName": "Tomas"},
                 "started": "2026-10-09T09:00:00.000+0000", "timeSpentSeconds": secs}
            ]}));
    })
}

fn run(conn: &Connection, server: &MockServer, refresh: Option<&str>) -> DayProgress {
    let c = http::client().unwrap();
    let a = auth(server);
    progress_with(conn, day(), refresh, Some(&a), Some("me"), &c, now()).unwrap()
}

#[test]
fn fresh_cache_makes_no_jira_call_and_answers_fast() {
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    cache(&conn, "G-1", 14400, 3600, 60);
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(json!({}));
    });
    let t0 = std::time::Instant::now();
    let got = run(&conn, &server, None);
    assert!(t0.elapsed() < std::time::Duration::from_millis(200)); // wrong: slow path on fresh
    assert_eq!(any.hits(), 0); // wrong: always refetch
    assert_eq!(got.tickets[0].estimate_seconds, Some(14400));
    assert_eq!(got.tickets[0].logged_seconds, 3600);
    assert_eq!(got.tickets[0].error, None);
}

#[test]
fn exactly_ten_minutes_is_fresh_eleven_is_stale() {
    let server = MockServer::start();
    let est = mock_estimates(&server, json!({"issues": []}));
    let wl = mock_worklogs(&server, "G-1", 100);
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    cache(&conn, "G-1", 1, 1, 600);
    run(&conn, &server, None);
    assert_eq!(est.hits(), 0); // wrong: >= instead of > at the boundary
    cache(&conn, "G-1", 1, 1, 660);
    run(&conn, &server, None);
    assert_eq!(est.hits(), 1);
    assert_eq!(wl.hits(), 1);
}

#[test]
fn only_stale_tickets_are_fetched_one_search_per_refresh() {
    let server = MockServer::start();
    let est = mock_estimates(
        &server,
        json!({"issues": [{"key": "G-2", "fields": {"timetracking": {"originalEstimateSeconds": 7200}}}]}),
    );
    let w1 = mock_worklogs(&server, "G-1", 100);
    let w2 = mock_worklogs(&server, "G-2", 5400);
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    cache(&conn, "G-1", 1, 1, 60);
    cache(&conn, "G-2", 1, 1, 700);
    let got = run(&conn, &server, None);
    assert_eq!(est.hits(), 1); // wrong: search per ticket
    assert_eq!(w1.hits(), 0); // wrong: refetch fresh tickets
    assert_eq!(w2.hits(), 1);
    let g2 = got.tickets.iter().find(|t| t.key == "G-2").unwrap();
    assert_eq!(g2.estimate_seconds, Some(7200));
    assert_eq!(g2.logged_seconds, 5400);
    assert_eq!(g2.pulled_at.as_deref(), Some(now().to_rfc3339().as_str()));
}

#[test]
fn jira_failure_keeps_cache_and_flags_the_ticket() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.any_request();
        then.status(500);
    });
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    cache(&conn, "G-1", 14400, 3600, 700);
    let got = run(&conn, &server, None);
    let t = &got.tickets[0];
    assert_eq!(t.error, Some(ProgressError::JiraUnavailable)); // wrong: swallow the error
    assert_eq!(t.estimate_seconds, Some(14400)); // wrong: wipe cache on failure
    assert_eq!(t.logged_seconds, 3600);
    let old = (now() - Duration::seconds(700)).to_rfc3339();
    assert_eq!(t.pulled_at.as_deref(), Some(old.as_str())); // wrong: bump pulled_at on failure
}

#[test]
fn jira_failure_without_cache_has_no_pulled_at() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.any_request();
        then.status(500);
    });
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    let t = &run(&conn, &server, None).tickets[0];
    assert_eq!(t.error, Some(ProgressError::JiraUnavailable));
    assert_eq!(t.pulled_at, None);
    assert_eq!(t.estimate_seconds, None); // wrong: default estimate 0
}

#[test]
fn one_ticket_failing_does_not_fail_the_others() {
    let server = MockServer::start();
    mock_estimates(&server, json!({"issues": []}));
    mock_worklogs(&server, "G-1", 100);
    server.mock(|when, then| {
        when.path("/rest/api/3/issue/G-2/worklog");
        then.status(500);
    });
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    let got = run(&conn, &server, None);
    assert_eq!(got.tickets[0].error, None); // wrong: one error taints all
    assert_eq!(got.tickets[0].logged_seconds, 100);
    assert_eq!(got.tickets[1].error, Some(ProgressError::JiraUnavailable));
}

#[test]
fn no_credentials_is_not_configured_and_keeps_cache() {
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    cache(&conn, "G-1", 14400, 3600, 700);
    let c = http::client().unwrap();
    let got = progress_with(&conn, day(), None, None, None, &c, now()).unwrap();
    assert_eq!(got.tickets[0].error, Some(ProgressError::NotConfigured)); // wrong: JiraUnavailable
    assert_eq!(got.tickets[0].logged_seconds, 3600);
    assert_eq!(got.tickets[1].pulled_at, None);
}

#[test]
fn refresh_forces_only_that_fresh_ticket() {
    let server = MockServer::start();
    mock_estimates(&server, json!({"issues": []}));
    let w1 = mock_worklogs(&server, "G-1", 100);
    let w2 = mock_worklogs(&server, "G-2", 200);
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    cache(&conn, "G-1", 1, 1, 10);
    cache(&conn, "G-2", 1, 1, 10);
    let got = run(&conn, &server, Some("G-1"));
    assert_eq!(w1.hits(), 1); // wrong: ignore refresh when fresh
    assert_eq!(w2.hits(), 0); // wrong: refresh every ticket
    assert_eq!(got.tickets[0].logged_seconds, 100);
}

#[test]
fn refresh_of_a_ticket_not_on_the_day_fetches_nothing() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(json!({}));
    });
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    cache(&conn, "G-1", 1, 1, 10);
    run(&conn, &server, Some("OTHER-9"));
    assert_eq!(any.hits(), 0); // wrong: fetch any requested key
}

#[test]
fn personal_blocks_yield_no_tickets_and_no_calls() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(json!({}));
    });
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 1);
    let got = run(&conn, &server, None);
    assert!(got.tickets.is_empty());
    assert_eq!(any.hits(), 0); // wrong: search with an empty key list
}

#[test]
fn people_are_marked_you_by_account_id() {
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    cache(&conn, "G-1", 1, 3600, 10);
    let server = MockServer::start();
    let c = http::client().unwrap();
    let a = auth(&server);
    let got = progress_with(&conn, day(), None, Some(&a), Some("me"), &c, now()).unwrap();
    assert!(got.tickets[0].people[0].is_you);
    let got = progress_with(&conn, day(), None, Some(&a), None, &c, now()).unwrap();
    assert!(!got.tickets[0].people[0].is_you); // wrong: everyone is you
}

#[test]
fn sync_marks_one_ticket_stale_so_a_one_minute_old_cache_refetches() {
    let server = MockServer::start();
    let est = mock_estimates(&server, json!({"issues": []}));
    mock_worklogs(&server, "G-1", 900);
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    cache(&conn, "G-1", 1, 1, 60);
    cache(&conn, "G-2", 1, 1, 60);
    mark_day_stale(&conn, day(), Some("G-1")).unwrap();
    let got = run(&conn, &server, None);
    assert_eq!(est.hits(), 1); // wrong: mark nothing
    assert_eq!(got.tickets[0].logged_seconds, 900);
    assert_eq!(got.tickets[1].logged_seconds, 1); // wrong: mark every ticket on a one-ticket sync
}

#[test]
fn sync_of_the_whole_day_marks_every_ticket() {
    let server = MockServer::start();
    mock_estimates(&server, json!({"issues": []}));
    let w1 = mock_worklogs(&server, "G-1", 900);
    let w2 = mock_worklogs(&server, "G-2", 900);
    let conn = open_memory().unwrap();
    block(&conn, "G-1", 0);
    block(&conn, "G-2", 0);
    cache(&conn, "G-1", 1, 1, 60);
    cache(&conn, "G-2", 1, 1, 60);
    mark_day_stale(&conn, day(), None).unwrap();
    run(&conn, &server, None);
    assert_eq!((w1.hits(), w2.hits()), (1, 1)); // wrong: mark only the first ticket
}

async fn get(state: &crate::daemon::Shared, path: &str) -> (StatusCode, serde_json::Value) {
    let r = router(state.clone())
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = r.status();
    let bytes = body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn route_serves_the_day_and_rejects_a_bad_day() {
    let state = state_from_conn(open_memory().unwrap());
    let (status, body) = get(&state, "/progress/2026-10-09").await;
    assert_eq!(status, StatusCode::OK); // wrong: route not registered
    assert_eq!(body, json!({"day": "2026-10-09", "tickets": []}));
    let (status, _) = get(&state, "/progress/not-a-day").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
