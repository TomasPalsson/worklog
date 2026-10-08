// Tests for T003 — daemon note routes and note jobs (spec 019).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::block_service::set_description;
use crate::daemon::daemon_note_block::start_job;
use crate::daemon::{router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::estimate::{FixedInvoker, ModelInvoker};
use crate::line_text_jobs::JobState;
use crate::note_block_contract::*;
use crate::note_writer::log_note_block;

fn fixed(reply: Value) -> impl FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> + Send + 'static {
    move || Ok(Box::new(FixedInvoker(reply)) as Box<dyn ModelInvoker>)
}

fn failing(
    msg: &'static str,
) -> impl FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> + Send + 'static {
    move || Err(anyhow::anyhow!(msg))
}

fn note_body(start: &str, minutes: i64) -> NoteBlockBody {
    NoteBlockBody {
        jira_issue: "APRO-1".into(),
        day: "2026-04-18".into(),
        start: start.into(),
        minutes,
        note: "fixed login bug".into(),
    }
}

async fn state_with_note_block() -> (Shared, i64) {
    let state = state_from_conn(open_memory().unwrap());
    let id = {
        let conn = state.conn.lock().await;
        let _g = crate::tz::test_env_lock();
        std::env::remove_var("WORKLOG_TZ");
        let today = chrono::NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        log_note_block(&conn, &note_body("10:00", 30), today)
            .unwrap()
            .id
    };
    (state, id)
}

async fn block(state: &Shared, id: i64) -> crate::models::Block {
    let conn = state.conn.lock().await;
    crate::repo::get_block(&conn, id).unwrap().unwrap()
}

async fn settle(state: &Shared, id: i64) -> JobState {
    for _ in 0..200_000 {
        match state.note_jobs.state(&id) {
            Some(JobState::Running) | None => tokio::task::yield_now().await,
            Some(done) => return done,
        }
    }
    panic!("note job never settled");
}

async fn call(state: &Shared, request: Request<Body>) -> (StatusCode, Value) {
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

fn post(uri: &str, body: Value) -> Request<Body> {
    Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn status_req(id: i64) -> Request<Body> {
    Request::get(format!("/blocks/{id}/note/status"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn job_writes_the_ai_description() {
    let (state, id) = state_with_note_block().await;
    let reply = json!({"description": "Fixed the login bug."});
    assert!(start_job(&state, id, false, fixed(reply)));
    assert_eq!(settle(&state, id).await, JobState::Done);
    let b = block(&state, id).await;
    assert_eq!(b.description.as_deref(), Some("Fixed the login bug."));
}

// catches: ignoring description_origin (overwriting a hand edit)
#[tokio::test(flavor = "current_thread")]
async fn job_fails_hand_edited_and_keeps_the_edit() {
    let (state, id) = state_with_note_block().await;
    {
        let conn = state.conn.lock().await;
        set_description(&conn, id, "my own words").unwrap();
    }
    let reply = json!({"description": "AI text."});
    assert!(start_job(&state, id, false, fixed(reply)));
    assert_eq!(
        settle(&state, id).await,
        JobState::Failed(REASON_HAND_EDITED.into())
    );
    let b = block(&state, id).await;
    assert_eq!(b.description.as_deref(), Some("my own words"));
}

// catches: force flag dropped between route and commit
#[tokio::test(flavor = "current_thread")]
async fn forced_job_replaces_a_hand_edit() {
    let (state, id) = state_with_note_block().await;
    {
        let conn = state.conn.lock().await;
        set_description(&conn, id, "my own words").unwrap();
    }
    let reply = json!({"description": "AI text."});
    assert!(start_job(&state, id, true, fixed(reply)));
    assert_eq!(settle(&state, id).await, JobState::Done);
    let b = block(&state, id).await;
    assert_eq!(b.description.as_deref(), Some("AI text."));
}

// catches: swallowing the invoker error / writing anyway
#[tokio::test(flavor = "current_thread")]
async fn invoker_failure_surfaces_and_leaves_description() {
    let (state, id) = state_with_note_block().await;
    let before = block(&state, id).await.description;
    assert!(start_job(&state, id, false, failing("claude is down")));
    assert_eq!(
        settle(&state, id).await,
        JobState::Failed("claude is down".into())
    );
    assert_eq!(block(&state, id).await.description, before);
}

// catches: an empty model reply being committed
#[tokio::test(flavor = "current_thread")]
async fn bad_model_reply_fails_the_job() {
    let (state, id) = state_with_note_block().await;
    let reply = json!({"description": "   "});
    assert!(start_job(&state, id, false, fixed(reply)));
    assert!(matches!(settle(&state, id).await, JobState::Failed(_)));
}

// catches: a second start while running racing the first
#[tokio::test(flavor = "current_thread")]
async fn second_start_while_running_is_rejected() {
    let (state, id) = state_with_note_block().await;
    assert!(state.note_jobs.try_start(id));
    let reply = json!({"description": "x"});
    assert!(!start_job(&state, id, false, fixed(reply)));
}

// catches: tracker keyed on a shared constant instead of the block id
#[tokio::test(flavor = "current_thread")]
async fn status_is_per_block_id() {
    let state = state_from_conn(open_memory().unwrap());
    let (s, v) = call(&state, status_req(1)).await;
    assert_eq!((s, v), (StatusCode::OK, json!({"state": "idle"})));
    state.note_jobs.try_start(2);
    assert_eq!(
        call(&state, status_req(2)).await.1,
        json!({"state": "running"})
    );
    assert_eq!(
        call(&state, status_req(1)).await.1,
        json!({"state": "idle"})
    );
    state.note_jobs.finish(2, Ok(()));
    assert_eq!(
        call(&state, status_req(2)).await.1,
        json!({"state": "done"})
    );
    state.note_jobs.finish(3, Err("boom".into()));
    assert_eq!(
        call(&state, status_req(3)).await.1,
        json!({"state": "failed", "reason": "boom"})
    );
}

// catches: regenerate starting when a job already runs; wrong reason text
#[tokio::test(flavor = "current_thread")]
async fn regenerate_route_rejects_while_running() {
    let (state, id) = state_with_note_block().await;
    state.note_jobs.try_start(id);
    let req = post(&format!("/blocks/{id}/note/regenerate"), json!({}));
    let (s, v) = call(&state, req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v, json!({"started": false, "reason": "already running"}));
}

// catches: route not registered, non-note block reaching the model
#[tokio::test(flavor = "current_thread")]
async fn regenerate_route_on_plain_block_fails_not_a_note_block() {
    let state = state_from_conn(open_memory().unwrap());
    let id = {
        let conn = state.conn.lock().await;
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    };
    // The route now rejects it up front (R1); the job-level guard in
    // prepare_note is still exercised by starting the job directly.
    let req = post(&format!("/blocks/{id}/note/regenerate"), json!({}));
    let (s, v) = call(&state, req).await;
    assert_eq!(
        (s, v),
        (
            StatusCode::OK,
            json!({"started": false, "reason": REASON_NOT_A_NOTE_BLOCK})
        )
    );
    assert!(start_job(&state, id, false, failing("unused")));
    assert_eq!(
        settle(&state, id).await,
        JobState::Failed(REASON_NOT_A_NOTE_BLOCK.into())
    );
}

// catches: no key validation before log_time (lowercase, missing number)
#[tokio::test(flavor = "current_thread")]
async fn log_note_rejects_a_bad_ticket_key() {
    let state = state_from_conn(open_memory().unwrap());
    let mut body = json!({"jira_issue": "apro-1", "day": "2026-04-18", "start": "10:00", "minutes": 30, "note": "x"});
    let (s, _) = call(&state, post("/blocks/note", body.clone())).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    body["jira_issue"] = json!("APRO-");
    let (s, _) = call(&state, post("/blocks/note", body)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

struct LockProbe {
    state: Shared,
    was_free: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ModelInvoker for LockProbe {
    fn invoke(&self, _s: &str, _u: &str, _sc: &Value, _m: &str) -> anyhow::Result<Value> {
        self.was_free.store(
            self.state.conn.try_lock().is_ok(),
            std::sync::atomic::Ordering::SeqCst,
        );
        Ok(json!({"description": "Probed."}))
    }
}

// catches: holding the sqlite connection across the model call
#[tokio::test(flavor = "current_thread")]
async fn connection_is_free_during_the_model_call() {
    let (state, id) = state_with_note_block().await;
    let was_free = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = LockProbe {
        state: state.clone(),
        was_free: was_free.clone(),
    };
    let make = move || Ok(Box::new(probe) as Box<dyn ModelInvoker>);
    assert!(start_job(&state, id, false, make));
    assert_eq!(settle(&state, id).await, JobState::Done);
    assert!(was_free.load(std::sync::atomic::Ordering::SeqCst));
}

// catches: regenerate on a missing / plain block leaking a tracker entry
#[tokio::test(flavor = "current_thread")]
async fn regenerate_rejects_missing_and_plain_blocks_without_tracking() {
    let (state, _) = state_with_note_block().await;
    let plain = {
        let conn = state.conn.lock().await;
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T12:00:00+00:00', '2026-04-18T12:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    };
    for id in [9999, plain] {
        let uri = format!("/blocks/{id}/note/regenerate");
        let (s, v) = call(&state, post(&uri, json!({}))).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            v,
            json!({"started": false, "reason": REASON_NOT_A_NOTE_BLOCK})
        );
        let (_, st) = call(&state, status_req(id)).await;
        assert_eq!(st, json!({"state": "idle"}));
    }
}

// catches: log_time errors mapped to 500 instead of 400 (no hub_error)
#[tokio::test(flavor = "current_thread")]
async fn log_note_maps_invalid_input_to_400() {
    let state = state_from_conn(open_memory().unwrap());
    let body = json!({"jira_issue": "APRO-1", "day": "not-a-day", "start": "10:00", "minutes": 30, "note": "x"});
    let (s, _) = call(&state, post("/blocks/note", body)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
