// Tests for T004 — POST /blocks/:id/redescribe (spec 026).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::daemon::daemon_redescribe::TEST_REPLY;
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::redescribe_contract::*;

// TEST_REPLY is process-wide; serialise the tests that touch it.
static REPLY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn state_with_block() -> (Shared, i64) {
    let state = state_from_conn(open_memory().unwrap());
    let id = {
        let conn = state.conn.lock().await;
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, \
             jira_issue, tempo_worklog_id, exported_at, estimated_by) VALUES ('2026-04-18', \
             '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 'Old text', \
             'APRO-1', 'tempo-7', '2026-04-19T00:00:00+00:00', 'manual')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    };
    (state, id)
}

async fn call(state: &Shared, id: i64, body: Value) -> (StatusCode, Value) {
    let request = Request::post(format!("/blocks/{id}/redescribe"))
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

async fn row(state: &Shared, id: i64) -> Vec<String> {
    let conn = state.conn.lock().await;
    conn.query_row(
        "SELECT description, jira_issue, tempo_worklog_id, exported_at, estimated_by, \
         CAST(duration_seconds AS TEXT) FROM blocks WHERE id = ?1",
        [id],
        |r| (0..6).map(|i| r.get::<_, String>(i)).collect(),
    )
    .unwrap()
}

fn set_reply(reply: Option<Value>) {
    *TEST_REPLY.lock().unwrap() = reply;
}

// catches: handler not wired, or writing columns beyond description
#[tokio::test(flavor = "current_thread")]
async fn returns_fake_description_and_changes_only_that_column() {
    let _guard = REPLY_LOCK.lock().await;
    let (state, id) = state_with_block().await;
    let before = row(&state, id).await;
    set_reply(Some(json!({"description": "Fixed the login bug"})));
    let (status, body) = call(&state, id, json!({"instruction": "login bug fix"})).await;
    set_reply(None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"block_id": id, "description": "Fixed the login bug"})
    );
    let after = row(&state, id).await;
    assert_eq!(after[0], "Fixed the login bug");
    assert_eq!(after[1..], before[1..]);
}

// catches: no trim before the empty check
#[tokio::test(flavor = "current_thread")]
async fn blank_instruction_is_400() {
    let (state, id) = state_with_block().await;
    let (status, _) = call(&state, id, json!({"instruction": "  \n "})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(row(&state, id).await[0], "Old text");
}

// catches: > vs >=, counting bytes instead of chars, validating before trim
#[tokio::test(flavor = "current_thread")]
async fn instruction_limit_is_500_chars_after_trim() {
    let _guard = REPLY_LOCK.lock().await;
    let (state, id) = state_with_block().await;
    set_reply(Some(json!({"description": "ok"})));
    for accepted in [
        "a".repeat(500),
        "þ".repeat(500),
        format!("  {}  ", "æ".repeat(500)),
    ] {
        let (status, _) = call(&state, id, json!({"instruction": accepted})).await;
        assert_eq!(status, StatusCode::OK);
    }
    set_reply(None);
    for rejected in ["a".repeat(501), "ð".repeat(501)] {
        let (status, _) = call(&state, id, json!({"instruction": rejected})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

// catches: swallowing the error / contract reason text not reaching the body
#[tokio::test(flavor = "current_thread")]
async fn rejection_bodies_carry_the_contract_reason() {
    let (state, id) = state_with_block().await;
    let (_, empty) = call(&state, id, json!({"instruction": ""})).await;
    assert!(empty.to_string().contains(REASON_EMPTY_INSTRUCTION));
    let (_, long) = call(&state, id, json!({"instruction": "a".repeat(501)})).await;
    assert!(long.to_string().contains(REASON_INSTRUCTION_TOO_LONG));
}

// catches: missing block surfacing as 200/500
#[tokio::test(flavor = "current_thread")]
async fn unknown_block_is_404() {
    let (state, _) = state_with_block().await;
    let (status, _) = call(&state, 9999, json!({"instruction": "x"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// catches: personal block reaching the model
#[tokio::test(flavor = "current_thread")]
async fn personal_block_is_400() {
    let (state, id) = state_with_block().await;
    state
        .conn
        .lock()
        .await
        .execute("UPDATE blocks SET is_personal = 1", [])
        .unwrap();
    let (status, body) = call(&state, id, json!({"instruction": "x"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.to_string().contains(REASON_PERSONAL));
}

// catches: a failed model reply being committed
#[tokio::test(flavor = "current_thread")]
async fn bad_model_reply_leaves_description() {
    let _guard = REPLY_LOCK.lock().await;
    let (state, id) = state_with_block().await;
    set_reply(Some(json!({"description": "   "})));
    let (status, _) = call(&state, id, json!({"instruction": "x"})).await;
    set_reply(None);
    assert!(!status.is_success());
    assert_eq!(row(&state, id).await[0], "Old text");
}
