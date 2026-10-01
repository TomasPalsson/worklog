// Tests for T010 — daemon week routes (spec 012, B8, B10).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use httpmock::prelude::*;
use tower::ServiceExt;

use crate::collectors::tempo::TempoAuth;
use crate::daemon::daemon_week::pull_week;
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;

const MONDAY: &str = "2026-09-28";
const ACCOUNT: &str = "557058:abc";

fn auth(server: &MockServer) -> TempoAuth {
    TempoAuth {
        token: "tempo-token".to_string(),
        author: ACCOUNT.to_string(),
        base_url: server.base_url(),
    }
}

async fn call(state: &Shared, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

fn post(path: &str, json: &str) -> Request<Body> {
    Request::post(path)
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .unwrap()
}

fn get(path: &str) -> Request<Body> {
    Request::get(path).body(Body::empty()).unwrap()
}

fn monday() -> chrono::NaiveDate {
    MONDAY.parse().unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn pull_stores_the_weeks_worklogs_and_schedule_and_reports_counts() {
    let state = state_from_conn(open_memory().unwrap());
    let server = MockServer::start();
    let worklogs = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/worklogs/user/{ACCOUNT}"))
            .query_param("from", MONDAY)
            .query_param("to", "2026-10-04")
            .header("authorization", "Bearer tempo-token");
        then.status(200).json_body(serde_json::json!({"results": [
            {"tempoWorklogId": 11, "startDate": "2026-09-28", "issue": {"id": 100},
             "timeSpentSeconds": 3600, "description": "a"},
            {"tempoWorklogId": 12, "startDate": "2026-09-29", "issue": {"id": 101},
             "timeSpentSeconds": 1800}
        ]}));
    });
    let schedule = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/user-schedule/{ACCOUNT}"))
            .query_param("from", MONDAY)
            .query_param("to", "2026-10-04");
        then.status(200).json_body(serde_json::json!({"results": [
            {"date": "2026-09-28", "requiredSeconds": 28800},
            {"date": "2026-09-29", "requiredSeconds": 28800}
        ]}));
    });

    let report = pull_week(state.clone(), auth(&server), monday())
        .await
        .unwrap_or_else(|_| panic!("pull should succeed"));

    worklogs.assert();
    schedule.assert();
    assert_eq!(report.monday, MONDAY);
    assert_eq!(
        (report.worklogs, report.outside, report.schedule_days),
        (2, 2, 2)
    );
    let stored: i64 = state
        .conn
        .lock()
        .await
        .query_row("SELECT SUM(seconds) FROM tempo_remote_worklogs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, 5400);
}

#[tokio::test(flavor = "current_thread")]
async fn tempo_rejecting_the_pull_is_a_502_with_tempos_body_and_nothing_stored() {
    let state = state_from_conn(open_memory().unwrap());
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path(format!("/worklogs/user/{ACCOUNT}"));
        then.status(401).body("bad token");
    });

    let error = pull_week(state.clone(), auth(&server), monday())
        .await
        .expect_err("tempo's 401 must fail the pull");
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "Tempo said 401: bad token");
    let rows: i64 = state
        .conn
        .lock()
        .await
        .query_row("SELECT COUNT(*) FROM tempo_remote_worklogs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn a_pull_for_a_non_monday_is_a_400() {
    let state = state_from_conn(open_memory().unwrap());
    for body in [r#"{"monday":"2026-09-29"}"#, r#"{"monday":"soon"}"#] {
        let (status, json) = call(&state, post("/tempo/pull", body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(json["error"].as_str().unwrap().contains("not a Monday"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn closeout_returns_seven_days_and_the_latest_pull_time() {
    let state = state_from_conn(open_memory().unwrap());
    state
        .conn
        .lock()
        .await
        .execute(
            "INSERT INTO tempo_remote_worklogs
                 (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
             VALUES ('9', '2026-09-30', 1, 7200, 'outside', '2026-10-01T08:00:00Z')",
            [],
        )
        .unwrap();

    let (status, json) = call(&state, get(&format!("/weeks/{MONDAY}/closeout"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["monday"], MONDAY);
    assert_eq!(json["days"].as_array().unwrap().len(), 7);
    assert_eq!(json["days"][2]["day"], "2026-09-30");
    assert_eq!(json["days"][2]["tempo_seconds"], 7200);
    assert_eq!(json["pulled_at"], "2026-10-01T08:00:00Z");
}

#[tokio::test(flavor = "current_thread")]
async fn closeout_for_a_non_monday_is_a_400() {
    let state = state_from_conn(open_memory().unwrap());
    let (status, _) = call(&state, get("/weeks/2026-09-29/closeout")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
