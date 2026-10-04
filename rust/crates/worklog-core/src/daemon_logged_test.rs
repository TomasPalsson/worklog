// Tests for T004 — daemon logged routes (spec 014, B5–B8, B13).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use httpmock::prelude::*;
use tower::ServiceExt;

use crate::collectors::tempo::TempoAuth;
use crate::daemon::daemon_logged::pull_range_with;
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::tempo_hub_contract::{PulledWorklog, RequiredDay};
use crate::tempo_remote::store_range;

const ACCOUNT: &str = "557058:abc";
const UNDER_DAY: &str = "2026-09-01";

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

fn date(s: &str) -> chrono::NaiveDate {
    s.parse().unwrap()
}

/// A past day with 1h logged of 8h required.
async fn state_with_under_day() -> Shared {
    let state = state_from_conn(open_memory().unwrap());
    let worklog = PulledWorklog {
        tempo_worklog_id: "1".to_string(),
        day: UNDER_DAY.to_string(),
        issue_id: 7,
        seconds: 3600,
        description: "x".to_string(),
    };
    let schedule = RequiredDay {
        day: UNDER_DAY.to_string(),
        required_seconds: 28800,
    };
    store_range(
        &*state.conn.lock().await,
        date(UNDER_DAY),
        date(UNDER_DAY),
        &[worklog],
        &[schedule],
        "2026-09-02T08:00:00Z",
    )
    .unwrap();
    state
}

fn dismiss_body(reason: &str) -> String {
    serde_json::json!({"day": UNDER_DAY, "reason": reason}).to_string()
}

async fn day_json(state: &Shared) -> serde_json::Value {
    let path = format!("/logged?from={UNDER_DAY}&to={UNDER_DAY}");
    let (status, json) = call(state, get(&path)).await;
    assert_eq!(status, StatusCode::OK);
    json["days"][0].clone()
}

#[tokio::test(flavor = "current_thread")]
async fn get_returns_a_day_per_date_up_to_42_and_400s_on_43() {
    let state = state_from_conn(open_memory().unwrap());

    let (status, json) = call(&state, get("/logged?from=2026-09-01&to=2026-10-12")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["days"].as_array().unwrap().len(), 42);

    let (status, json) = call(&state, get("/logged?from=2026-09-01&to=2026-10-13")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].as_str().is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn get_with_from_after_to_or_a_bad_date_is_a_400() {
    let state = state_from_conn(open_memory().unwrap());
    for path in [
        "/logged?from=2026-09-05&to=2026-09-04",
        "/logged?from=soon&to=2026-09-04",
    ] {
        let (status, _) = call(&state, get(path)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pull_stores_the_range_and_returns_its_entries() {
    let state = state_from_conn(open_memory().unwrap());
    let server = MockServer::start();
    let worklogs = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/worklogs/user/{ACCOUNT}"))
            .query_param("from", "2026-09-01")
            .query_param("to", "2026-09-03");
        then.status(200).json_body(serde_json::json!({"results": [
            {"tempoWorklogId": 11, "startDate": "2026-09-01", "issue": {"id": 100},
             "timeSpentSeconds": 3600, "description": "a"}
        ]}));
    });
    let schedule = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/user-schedule/{ACCOUNT}"))
            .query_param("from", "2026-09-01")
            .query_param("to", "2026-09-03");
        then.status(200).json_body(serde_json::json!({"results": [
            {"date": "2026-09-01", "requiredSeconds": 28800}
        ]}));
    });

    let range = pull_range_with(
        state.clone(),
        auth(&server),
        date("2026-09-01"),
        date("2026-09-03"),
    )
    .await
    .unwrap_or_else(|_| panic!("pull should succeed"));

    worklogs.assert();
    schedule.assert();
    assert_eq!(range.days.len(), 3);
    assert_eq!(range.days[0].logged_seconds, 3600);
    assert_eq!(range.days[0].entries[0].tempo_worklog_id, "11");
    assert!(range.pulled_at.is_some());
    let path = "/logged?from=2026-09-01&to=2026-09-03";
    let (_, json) = call(&state, get(path)).await;
    assert_eq!(json["days"][0]["entries"][0]["tempo_worklog_id"], "11");
}

#[tokio::test(flavor = "current_thread")]
async fn tempo_500_is_a_502_and_stored_rows_read_back_unchanged() {
    let state = state_with_under_day().await;
    let before = day_json(&state).await;
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path(format!("/worklogs/user/{ACCOUNT}"));
        then.status(500).body("boom");
    });

    let error = pull_range_with(state.clone(), auth(&server), date(UNDER_DAY), date(UNDER_DAY))
        .await
        .expect_err("tempo's 500 must fail the pull");

    assert_eq!(error.into_response().status(), StatusCode::BAD_GATEWAY);
    assert_eq!(day_json(&state).await, before);
    assert_eq!(before["logged_seconds"], 3600);
}

#[tokio::test(flavor = "current_thread")]
async fn pull_with_a_bad_range_is_a_400() {
    let state = state_from_conn(open_memory().unwrap());
    let (status, _) = call(
        &state,
        post("/logged/pull", r#"{"from":"2026-09-05","to":"2026-09-04"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn dismiss_marks_an_under_day_and_the_read_back_agrees() {
    let state = state_with_under_day().await;
    assert_eq!(day_json(&state).await["state"], "under");

    let (status, json) = call(&state, post("/logged/dismiss", &dismiss_body("dentist"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["day"], UNDER_DAY);
    assert_eq!(json["state"], "dismissed");
    assert_eq!(json["dismissal_reason"], "dentist");
    assert_eq!(day_json(&state).await, json);
}

#[tokio::test(flavor = "current_thread")]
async fn dismiss_makes_no_tempo_call() {
    let state = state_with_under_day().await;
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let (status, _) = call(&state, post("/logged/dismiss", &dismiss_body("dentist"))).await;
    let (undo, _) = call(
        &state,
        post("/logged/undismiss", &format!(r#"{{"day":"{UNDER_DAY}"}}"#)),
    )
    .await;

    assert_eq!((status, undo), (StatusCode::OK, StatusCode::OK));
    any.assert_hits(0);
}

#[tokio::test(flavor = "current_thread")]
async fn dismiss_with_an_empty_blank_or_81_char_reason_is_a_400_and_stores_nothing() {
    let state = state_with_under_day().await;
    for reason in ["".to_string(), "   ".to_string(), "x".repeat(81)] {
        let (status, _) = call(&state, post("/logged/dismiss", &dismiss_body(&reason))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{reason:?}");
    }
    let day = day_json(&state).await;
    assert_eq!(day["state"], "under");
    assert_eq!(day["dismissal_reason"], serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn dismiss_with_a_bad_day_is_a_400() {
    let state = state_with_under_day().await;
    let (status, _) = call(
        &state,
        post("/logged/dismiss", r#"{"day":"yesterday","reason":"x"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn an_80_char_multibyte_reason_is_accepted() {
    let state = state_with_under_day().await;
    let reason = "ð".repeat(80);

    let (status, json) = call(&state, post("/logged/dismiss", &dismiss_body(&reason))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dismissal_reason"], reason);
}

#[tokio::test(flavor = "current_thread")]
async fn undismiss_returns_the_day_to_under() {
    let state = state_with_under_day().await;
    call(&state, post("/logged/dismiss", &dismiss_body("dentist"))).await;

    let (status, json) = call(
        &state,
        post("/logged/undismiss", &format!(r#"{{"day":"{UNDER_DAY}"}}"#)),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["state"], "under");
    assert_eq!(json["dismissal_reason"], serde_json::Value::Null);
}
