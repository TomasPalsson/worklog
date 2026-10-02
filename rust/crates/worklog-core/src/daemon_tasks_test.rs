// Tests for T009 — daemon task routes (spec 012, B5, B6, B11).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use httpmock::prelude::*;
use tower::ServiceExt;

use crate::collectors::jira::JiraAuth;
use crate::daemon::daemon_tasks::{apply_transition, draft_ticket};
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::estimate::{FixedInvoker, ModelInvoker};
use crate::tempo_hub_contract::StatusCategory;

const MONDAY: &str = "2026-09-28";

fn state_with_ticket() -> Shared {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at)
         VALUES ('APRO-1', 'Fix the thing', 'To Do', 'new', 0, '2026-09-30T08:00:00Z')",
        [],
    )
    .unwrap();
    state_from_conn(conn)
}

fn auth(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "me@example.com".to_string(),
        token: "token".to_string(),
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

async fn cached_status(state: &Shared) -> (String, Option<String>) {
    state
        .conn
        .lock()
        .await
        .query_row(
            "SELECT status, status_category FROM jira_tickets WHERE key = 'APRO-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn transition_stores_the_status_jira_reports_afterwards() {
    let state = state_with_ticket();
    let server = MockServer::start();
    let moved = server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue/APRO-1/transitions")
            .json_body(serde_json::json!({"transition": {"id": "31"}}));
        then.status(204);
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/APRO-1");
        then.status(200).json_body(serde_json::json!({
            "fields": {"status": {"name": "Done", "statusCategory": {"key": "done"}}}
        }));
    });

    let status = apply_transition(
        state.clone(),
        auth(&server),
        "APRO-1".to_string(),
        "31".to_string(),
    )
    .await
    .unwrap_or_else(|_| panic!("transition should succeed"));

    moved.assert();
    assert_eq!(status.key, "APRO-1");
    assert_eq!(status.status, "Done");
    assert_eq!(status.status_category, Some(StatusCategory::Done));
    assert_eq!(
        cached_status(&state).await,
        ("Done".to_string(), Some("done".to_string()))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn jira_rejecting_a_transition_is_a_502_with_jiras_body_and_no_cache_change() {
    let state = state_with_ticket();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue/APRO-1/transitions");
        then.status(400).body("Transition 99 is not valid");
    });

    let error = apply_transition(
        state.clone(),
        auth(&server),
        "APRO-1".to_string(),
        "99".to_string(),
    )
    .await
    .expect_err("jira's 400 must fail the route");
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "Jira said 400: Transition 99 is not valid");
    assert_eq!(
        cached_status(&state).await,
        ("To Do".to_string(), Some("new".to_string()))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn comment_that_is_empty_blank_or_too_long_is_a_400() {
    let state = state_with_ticket();
    let too_long = "x".repeat(5001);
    for text in ["", "   \n ", too_long.as_str()] {
        let body = serde_json::json!({ "text": text }).to_string();
        let (status, json) = call(&state, post("/tickets/APRO-1/comment", &body)).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "text of {} chars",
            text.len()
        );
        assert!(json["error"].as_str().is_some_and(|m| !m.is_empty()));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_malformed_ticket_key_is_a_400_on_every_ticket_route() {
    let state = state_with_ticket();
    let (status, _) = call(&state, get("/tickets/not-a-key/transitions")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &state,
        post("/tickets/not-a-key/transition", r#"{"transition_id":"31"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&state, post("/tickets/not-a-key/draft", "{}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&state, get("/tickets/not-a-key/detail")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn tasks_lists_the_cached_assigned_ticket_for_a_monday() {
    let state = state_with_ticket();
    let (status, json) = call(&state, get(&format!("/tasks?monday={MONDAY}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["monday"], MONDAY);
    assert_eq!(json["tasks"][0]["key"], "APRO-1");
    assert_eq!(json["tasks"][0]["assigned"], true);
}

#[tokio::test(flavor = "current_thread")]
async fn tasks_rejects_a_monday_that_is_not_a_monday() {
    let state = state_with_ticket();
    let (status, _) = call(&state, get("/tasks?monday=2026-09-29")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&state, get("/tasks?monday=soon")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn jira_offers_two_transitions(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/APRO-1/transitions");
        then.status(200).json_body(serde_json::json!({
            "transitions": [
                {"id": "21", "name": "Start", "to": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}}},
                {"id": "31", "name": "Finish", "to": {"name": "Done", "statusCategory": {"key": "done"}}}
            ]
        }));
    });
}

fn invoker_suggesting(id: &'static str) -> impl FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> {
    move || {
        Ok(Box::new(FixedInvoker(serde_json::json!({
            "comment": "Fixed the thing",
            "suggested_transition_id": id,
        }))))
    }
}

#[tokio::test(flavor = "current_thread")]
async fn draft_returns_the_comment_with_live_transitions_and_a_listed_suggestion() {
    let state = state_with_ticket();
    let server = MockServer::start();
    jira_offers_two_transitions(&server);

    let draft = draft_ticket(
        state,
        auth(&server),
        "APRO-1".to_string(),
        invoker_suggesting("31"),
    )
    .await
    .unwrap_or_else(|_| panic!("draft should succeed"));

    assert_eq!(draft.comment, "Fixed the thing");
    assert_eq!(draft.suggested_transition_id.as_deref(), Some("31"));
    let ids: Vec<_> = draft.transitions.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["21", "31"]);
}

#[tokio::test(flavor = "current_thread")]
async fn draft_drops_a_suggestion_jira_does_not_offer() {
    let state = state_with_ticket();
    let server = MockServer::start();
    jira_offers_two_transitions(&server);

    let draft = draft_ticket(
        state,
        auth(&server),
        "APRO-1".to_string(),
        invoker_suggesting("999"),
    )
    .await
    .unwrap_or_else(|_| panic!("draft should succeed"));

    assert_eq!(draft.suggested_transition_id, None);
}
