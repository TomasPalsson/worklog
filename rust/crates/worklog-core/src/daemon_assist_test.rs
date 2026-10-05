// Tests for T007 — daemon assist routes (spec 013).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use httpmock::prelude::*;
use serde_json::json;
use tower::ServiceExt;

use crate::collectors::jira::JiraAuth;
use crate::daemon::daemon_assist::{
    allowed_list, create_assisted, move_status, relearn_accounts, start, suggest_accounts,
    view_ticket,
};
use crate::daemon::{router, state_from_conn, ApiError, Shared};
use crate::db::open_memory;
use crate::jira_assist_contract::{AssistCreateBody, StartOutcome};
use crate::tempo_hub_contract::StatusCategory;

const FIELD: &str = "customfield_10100";

fn auth(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "me@example.com".to_string(),
        token: "token".to_string(),
    }
}

fn state() -> Shared {
    state_from_conn(open_memory().unwrap())
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

fn mock_ticket(server: &MockServer, key: &str, status: &str, category: &str) {
    let path = format!("/rest/api/3/issue/{key}");
    server.mock(|when, then| {
        when.method(GET).path(&path).query_param("fields", FIELD);
        then.status(200)
            .json_body(json!({ "fields": { FIELD: { "id": 42, "name": "Acme" } } }));
    });
    server.mock(|when, then| {
        when.method(GET).path(&path);
        then.status(200).json_body(json!({ "fields": {
            "summary": "Do the thing",
            "status": { "name": status, "statusCategory": { "key": category } },
            "issuetype": { "name": "Story" }
        }}));
    });
}

fn mock_createmeta(server: &MockServer, accounts: serde_json::Value) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
        then.status(200)
            .json_body(json!({ "issueTypes": [{ "id": "7", "name": "Story" }], "isLast": true }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
        then.status(200).json_body(json!({
            "fields": [{ "fieldId": FIELD, "allowedValues": accounts }], "isLast": true
        }));
    });
}

fn ok<T>(result: Result<T, ApiError>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => panic!("api call failed"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn routes_refuse_malformed_keys_before_touching_jira() {
    let state = state();
    for path in ["/tickets/nope/start", "/tickets/nope/move"] {
        let (status, _) = call(&state, post(path, r#"{"to_status":"Done"}"#)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
    let (status, _) = call(
        &state,
        Request::get("/tickets/nope/view")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn move_route_refuses_a_foreign_project_with_400() {
    let (status, body) = call(
        &state(),
        post("/tickets/GOJ-1/move", r#"{"to_status":"Done"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.to_string().contains("GENAI"), "{body}");
}

#[tokio::test(flavor = "current_thread")]
async fn view_returns_detail_and_account() {
    let server = MockServer::start();
    mock_ticket(&server, "APRO-9", "To Do", "new");
    let view = ok(view_ticket(auth(&server), Some(FIELD.into()), "APRO-9".into()).await);
    assert_eq!(view.detail.status.as_deref(), Some("To Do"));
    assert_eq!(view.account_id.as_deref(), Some("42"));
    assert_eq!(view.account_name.as_deref(), Some("Acme"));
}

#[tokio::test(flavor = "current_thread")]
async fn start_moves_a_genai_ticket() {
    let server = MockServer::start();
    mock_ticket(&server, "GENAI-1", "To Do", "new");
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/GENAI-1/transitions");
        then.status(200).json_body(json!({ "transitions": [
            { "id": "11", "name": "Start", "to": { "name": "In Progress", "statusCategory": { "key": "indeterminate" } } }
        ]}));
    });
    let post = server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue/GENAI-1/transitions");
        then.status(204);
    });
    let got = ok(start(auth(&server), Some(FIELD.into()), "GENAI-1".into()).await);
    post.assert_hits(1);
    assert_eq!(got.outcome, StartOutcome::Moved);
}

#[tokio::test(flavor = "current_thread")]
async fn move_stores_the_new_status_in_the_ticket_cache() {
    let state = state();
    state
        .conn
        .lock()
        .await
        .execute(
            "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at, updated)
             VALUES ('GENAI-6', 's', 'In Progress', 'indeterminate', 0, '2026-09-30T08:00:00Z', '2026-09-29T10:00:00.000+0000')",
            [],
        )
        .unwrap();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/GENAI-6/transitions");
        then.status(200).json_body(json!({ "transitions": [
            { "id": "21", "name": "Finish", "to": { "name": "Done", "statusCategory": { "key": "done" } } }
        ]}));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue/GENAI-6/transitions");
        then.status(204);
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/GENAI-6")
            .query_param("fields", "status");
        then.status(200).json_body(json!({ "fields": {
            "status": { "name": "Done", "statusCategory": { "key": "done" } }
        }}));
    });
    let got = ok(move_status(
        state.clone(),
        auth(&server),
        "GENAI-6".into(),
        "done".into(),
    )
    .await);
    assert_eq!(got.status_category, Some(StatusCategory::Done));
    let cached: (String, String) = state
        .conn
        .lock()
        .await
        .query_row(
            "SELECT status, status_category FROM jira_tickets WHERE key = 'GENAI-6'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cached, ("Done".into(), "done".into()));
}

#[tokio::test(flavor = "current_thread")]
async fn allowed_lists_the_accounts_jira_accepts() {
    let server = MockServer::start();
    mock_createmeta(&server, json!([{ "id": "42", "name": "Acme" }]));
    let got = ok(allowed_list(auth(&server), FIELD.into()).await);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].id.as_str(), got[0].name.as_str()), ("42", "Acme"));
}

#[tokio::test(flavor = "current_thread")]
async fn relearn_then_suggest_ranks_the_learned_account() {
    let state = state();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/search/jql");
        then.status(200).json_body(json!({ "issues": [
            { "fields": { "summary": "Acme - invoice export", FIELD: { "id": 42, "name": "Acme" } } }
        ]}));
    });
    mock_createmeta(
        &server,
        json!([{ "id": "42", "name": "Acme" }, { "id": "7", "name": "Other" }]),
    );
    let report = ok(relearn_accounts(state.clone(), auth(&server), FIELD.into()).await);
    assert_eq!(report.tickets_read, 1);
    assert_eq!(report.accounts, 1);
    let got =
        ok(suggest_accounts(state, auth(&server), FIELD.into(), "Acme dashboard".into()).await);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].account.id, "42");
}

#[tokio::test(flavor = "current_thread")]
async fn suggest_on_an_empty_clue_log_relearns_first() {
    let server = MockServer::start();
    let search = server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/search/jql");
        then.status(200).json_body(json!({ "issues": [
            { "fields": { "summary": "Acme - invoice export", FIELD: { "id": 42, "name": "Acme" } } }
        ]}));
    });
    mock_createmeta(&server, json!([{ "id": "42", "name": "Acme" }]));
    let got = ok(suggest_accounts(state(), auth(&server), FIELD.into(), "Acme dashboard".into()).await);
    search.assert_hits(1);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].account.id, "42");
}

#[tokio::test(flavor = "current_thread")]
async fn create_refuses_emoji_with_400_and_posts_nothing() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let body = AssistCreateBody {
        summary: "Ship it \u{1F680}".into(),
        description: "d".into(),
        account_id: "42".into(),
        guessed_account_id: None,
        clues: vec![],
        assignee_account_id: None,
        unassigned: false,
    };
    let err = create_assisted(state(), auth(&server), FIELD.into(), body)
        .await
        .expect_err("emoji must be refused");
    any.assert_hits(0);
    assert!(matches!(err, ApiError::BadRequest(_)));
}

#[tokio::test(flavor = "current_thread")]
async fn hints_route_lists_done_hints() {
    let state = state();
    let (status, body) = call(&state, Request::get("/hints").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}
