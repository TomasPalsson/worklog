// Tests for T004 — POST /tempo/lines/redescribe (spec 027).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::daemon::daemon_tempo_line_redescribe::{commit_error, TEST_REPLY};
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::redescribe_contract::*;
use crate::tempo_line_contract::REASON_TEXT_CHANGED;

// TEST_REPLY is process-wide; serialise the tests that touch it.
static REPLY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const DAY: &str = "2026-10-05";
const NEW_TEXT: &str = "Lagaði villu í innskráningu. Prófaði breytinguna.";

async fn state_with_block() -> Shared {
    let state = state_from_conn(open_memory().unwrap());
    state
        .conn
        .lock()
        .await
        .execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, \
             description, tempo_worklog_id, estimated_by) VALUES ('2026-10-05', 'APRO-1', \
             '2026-10-05T09:00:00+00:00', '2026-10-05T09:30:00+00:00', 1800, 'Worked on login', \
             'tempo-7', 'manual')",
            [],
        )
        .unwrap();
    state
}

fn body_for(instruction: &str) -> Value {
    json!({"day": DAY, "jira_issue": "APRO-1", "instruction": instruction})
}

async fn call(state: &Shared, body: Value) -> (StatusCode, Value) {
    let request = Request::post("/tempo/lines/redescribe")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn block_row(state: &Shared) -> Vec<String> {
    state
        .conn
        .lock()
        .await
        .query_row(
            "SELECT description, jira_issue, tempo_worklog_id, estimated_by, \
             CAST(duration_seconds AS TEXT) FROM blocks",
            [],
            |r| (0..5).map(|i| r.get::<_, String>(i)).collect(),
        )
        .unwrap()
}

async fn text_rows(state: &Shared) -> i64 {
    state
        .conn
        .lock()
        .await
        .query_row("SELECT COUNT(*) FROM tempo_line_texts", [], |r| r.get(0))
        .unwrap()
}

fn set_reply(reply: Option<Value>) {
    *TEST_REPLY.lock().unwrap() = reply;
}

// catches: handler not wired, a reply not stored as manual text, or writes beyond the line row
#[tokio::test(flavor = "current_thread")]
async fn returns_fake_text_and_changes_only_the_line_row() {
    let _guard = REPLY_LOCK.lock().await;
    let state = state_with_block().await;
    let before = block_row(&state).await;
    set_reply(Some(json!({"text": NEW_TEXT})));
    let (status, body) = call(&state, body_for("nefndu prófanir")).await;
    set_reply(None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["text"], NEW_TEXT);
    assert_eq!(body["text_origin"], "manual");
    assert_eq!(body["jira_issue"], "APRO-1");
    assert_eq!(body["union_seconds"], 1800);
    assert_eq!(block_row(&state).await, before);
    assert_eq!(text_rows(&state).await, 1);
}

// catches: no trim before the empty check
#[tokio::test(flavor = "current_thread")]
async fn blank_instruction_is_400() {
    let state = state_with_block().await;
    let (status, _) = call(&state, body_for("  \n ")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text_rows(&state).await, 0);
}

// catches: > vs >=, counting bytes instead of chars, validating before trim
#[tokio::test(flavor = "current_thread")]
async fn instruction_limit_is_500_chars_after_trim() {
    let _guard = REPLY_LOCK.lock().await;
    let state = state_with_block().await;
    set_reply(Some(json!({"text": NEW_TEXT})));
    for accepted in [
        "a".repeat(500),
        "þ".repeat(500),
        format!("  {}  ", "æ".repeat(500)),
    ] {
        let (status, _) = call(&state, body_for(&accepted)).await;
        assert_eq!(status, StatusCode::OK);
    }
    set_reply(None);
    for rejected in ["a".repeat(501), "ð".repeat(501)] {
        let (status, _) = call(&state, body_for(&rejected)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

// catches: validating after the model call, or a rejected instruction still creating the row
#[tokio::test(flavor = "current_thread")]
async fn over_long_instruction_leaves_no_text_row() {
    let _guard = REPLY_LOCK.lock().await;
    let state = state_with_block().await;
    set_reply(Some(json!({"text": NEW_TEXT})));
    let (status, _) = call(&state, body_for(&"a".repeat(501))).await;
    set_reply(None);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text_rows(&state).await, 0);
}

// catches: swallowing the error / contract reason text not reaching the body
#[tokio::test(flavor = "current_thread")]
async fn rejection_bodies_carry_the_contract_reason() {
    let state = state_with_block().await;
    let (_, empty) = call(&state, body_for("")).await;
    assert!(empty.to_string().contains(REASON_EMPTY_INSTRUCTION));
    let (_, long) = call(&state, body_for(&"a".repeat(501))).await;
    assert!(long.to_string().contains(REASON_INSTRUCTION_TOO_LONG));
}

// catches: a line with no blocks surfacing as 200/500, or a prefix/other ticket matching
#[tokio::test(flavor = "current_thread")]
async fn unknown_line_is_404() {
    let state = state_with_block().await;
    for (day, issue) in [(DAY, "APRO-2"), (DAY, "APRO"), ("2026-10-06", "APRO-1")] {
        let body = json!({"day": day, "jira_issue": issue, "instruction": "x"});
        let (status, _) = call(&state, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{day} {issue}");
    }
}

// catches: personal blocks counting as a line
#[tokio::test(flavor = "current_thread")]
async fn line_with_only_personal_blocks_is_404() {
    let state = state_with_block().await;
    state
        .conn
        .lock()
        .await
        .execute("UPDATE blocks SET is_personal = 1", [])
        .unwrap();
    let (status, _) = call(&state, body_for("x")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// catches: a failed model reply being committed, or mapped to 500/200
#[tokio::test(flavor = "current_thread")]
async fn bad_model_reply_is_502_and_stores_nothing() {
    let _guard = REPLY_LOCK.lock().await;
    let state = state_with_block().await;
    set_reply(Some(json!({"text": "   "})));
    let (status, _) = call(&state, body_for("x")).await;
    set_reply(None);
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(text_rows(&state).await, 0);
}

// catches: the changed-while-writing error mapped to 500 instead of 409
#[test]
fn text_changed_maps_to_409_and_other_errors_to_500() {
    let conflict = commit_error(anyhow::anyhow!(REASON_TEXT_CHANGED)).into_response();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let other = commit_error(anyhow::anyhow!("disk full")).into_response();
    assert_eq!(other.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
