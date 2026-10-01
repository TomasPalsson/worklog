// Tests for T007 — daemon /tempo/lines routes and line-text generation
// (spec 011, B9).

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use tower::ServiceExt;

use crate::daemon::daemon_tempo_lines::generate_tempo_lines;
use crate::daemon::{count_generated_tempo_lines, router, state_from_conn, Shared};
use crate::db::open_memory;
use crate::estimate::{FixedInvoker, ModelInvoker};
use crate::tempo_line_contract::TempoLineKey;

const DAY: &str = "2026-09-30";

fn state_with_two_block_line() -> Shared {
    let conn = open_memory().unwrap();
    for (start, end, description) in [
        (
            "2026-09-30T09:00:00+00:00",
            "2026-09-30T10:00:00+00:00",
            "Alpha",
        ),
        (
            "2026-09-30T10:00:00+00:00",
            "2026-09-30T11:00:00+00:00",
            "Beta",
        ),
    ] {
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
             VALUES (?1, 'APRO-1', ?2, ?3, 3600, ?4)",
            params![DAY, start, end, description],
        )
        .unwrap();
    }
    state_from_conn(conn)
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

fn get_day() -> Request<Body> {
    Request::get(format!("/tempo/lines/{DAY}"))
        .body(Body::empty())
        .unwrap()
}

fn key() -> TempoLineKey {
    TempoLineKey {
        day: DAY.to_string(),
        jira_issue: "APRO-1".to_string(),
    }
}

fn fixed_invoker() -> anyhow::Result<Box<dyn ModelInvoker>> {
    Ok(Box::new(FixedInvoker(
        serde_json::json!({"description": "Implement alpha and beta"}),
    )))
}

#[tokio::test(flavor = "current_thread")]
async fn get_lists_the_day_line_with_union_hours_and_no_stored_text() {
    let state = state_with_two_block_line();
    let (status, lines) = call(&state, get_day()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lines.as_array().unwrap().len(), 1);
    assert_eq!(lines[0]["jira_issue"], "APRO-1");
    assert_eq!(lines[0]["union_seconds"], 7200);
    assert_eq!(lines[0]["effective_seconds"], 7200);
    assert_eq!(lines[0]["text"], serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn text_route_stores_manual_text_and_404s_for_unknown_line() {
    let state = state_with_two_block_line();
    let (status, line) = call(
        &state,
        post(
            "/tempo/lines/text",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-1","text":" Mine "}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(line["text"], "Mine");
    assert_eq!(line["text_origin"], "manual");

    let (status, _) = call(
        &state,
        post(
            "/tempo/lines/text",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-9","text":"x"}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "current_thread")]
async fn hours_route_sets_override_400s_invalid_and_404s_unknown_line() {
    let state = state_with_two_block_line();
    let (status, line) = call(
        &state,
        post(
            "/tempo/lines/hours",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-1","seconds":5400}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(line["hours_override_seconds"], 5400);
    assert_eq!(line["effective_seconds"], 5400);
    assert_eq!(line["union_seconds"], 7200);

    let (status, _) = call(
        &state,
        post(
            "/tempo/lines/hours",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-1","seconds":1000}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &state,
        post(
            "/tempo/lines/hours",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-9","seconds":1800}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "current_thread")]
async fn regenerate_route_404s_for_unknown_line() {
    let state = state_with_two_block_line();
    let (status, _) = call(
        &state,
        post(
            "/tempo/lines/regenerate",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-9"}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "current_thread")]
async fn generation_stores_text_once_and_skips_fresh_and_manual_lines_unless_forced() {
    let state = state_with_two_block_line();
    let generated = generate_tempo_lines(state.clone(), DAY.to_string(), None, fixed_invoker)
        .await
        .unwrap();
    assert_eq!(generated, vec![key()]);

    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], "Implement alpha and beta");
    assert_eq!(lines[0]["text_origin"], "generated");

    let again = generate_tempo_lines(state.clone(), DAY.to_string(), None, fixed_invoker)
        .await
        .unwrap();
    assert!(again.is_empty());

    call(
        &state,
        post(
            "/tempo/lines/text",
            &format!(r#"{{"day":"{DAY}","jira_issue":"APRO-1","text":"Mine"}}"#),
        ),
    )
    .await;
    let after_manual = generate_tempo_lines(state.clone(), DAY.to_string(), None, fixed_invoker)
        .await
        .unwrap();
    assert!(after_manual.is_empty());

    let forced = generate_tempo_lines(state.clone(), DAY.to_string(), Some(key()), fixed_invoker)
        .await
        .unwrap();
    assert_eq!(forced, vec![key()]);
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], "Implement alpha and beta");
}

#[tokio::test(flavor = "current_thread")]
async fn estimate_pass_counts_only_newly_generated_lines() {
    let state = state_with_two_block_line();
    let first = count_generated_tempo_lines(state.clone(), DAY.to_string(), fixed_invoker).await;
    assert_eq!(first, 1);
    let second = count_generated_tempo_lines(state, DAY.to_string(), fixed_invoker).await;
    assert_eq!(second, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn estimate_route_reports_tempo_lines_count() {
    let state = state_from_conn(open_memory().unwrap());
    let (status, report) = call(&state, post("/estimate", &format!(r#"{{"day":"{DAY}"}}"#))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["tempo_lines"], 0);
}

fn failing_invoker() -> anyhow::Result<Box<dyn ModelInvoker>> {
    struct Failing;
    impl ModelInvoker for Failing {
        fn invoke(
            &self,
            _system: &str,
            _user: &str,
            _schema: &serde_json::Value,
            _model: &str,
        ) -> anyhow::Result<serde_json::Value> {
            anyhow::bail!("model unreachable")
        }
    }
    Ok(Box::new(Failing))
}

#[tokio::test(flavor = "current_thread")]
async fn failed_generation_stores_no_text_and_is_not_counted() {
    let state = state_with_two_block_line();
    let generated = generate_tempo_lines(state.clone(), DAY.to_string(), None, failing_invoker)
        .await
        .unwrap();
    assert!(generated.is_empty());
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], serde_json::Value::Null);

    let count = count_generated_tempo_lines(state.clone(), DAY.to_string(), failing_invoker).await;
    assert_eq!(count, 0);
    let retried = generate_tempo_lines(state, DAY.to_string(), None, fixed_invoker)
        .await
        .unwrap();
    assert_eq!(retried, vec![key()]);
}
