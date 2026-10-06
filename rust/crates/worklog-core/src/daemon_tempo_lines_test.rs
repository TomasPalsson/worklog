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
        serde_json::json!({"text": "Lagaði villu í uppsetningu. Prófaði breytinguna."}),
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
    assert_eq!(lines[0]["text"], "Lagaði villu í uppsetningu. Prófaði breytinguna.");
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
    assert_eq!(lines[0]["text"], "Lagaði villu í uppsetningu. Prófaði breytinguna.");
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

fn state_with_one_block_line() -> Shared {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES (?1, 'APRO-1', '2026-09-30T09:00:00+00:00', '2026-09-30T10:00:00+00:00', 3600, 'Alpha')",
        params![DAY],
    )
    .unwrap();
    state_from_conn(conn)
}

#[tokio::test(flavor = "current_thread")]
async fn forced_regenerate_asks_the_model_even_for_a_single_description() {
    let state = state_with_one_block_line();
    let forced = generate_tempo_lines(state.clone(), DAY.to_string(), Some(key()), fixed_invoker)
        .await
        .unwrap();
    assert_eq!(forced, vec![key()]);
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], "Lagaði villu í uppsetningu. Prófaði breytinguna.");
}

#[tokio::test(flavor = "current_thread")]
async fn forced_regenerate_fails_loudly_when_the_model_fails() {
    let state = state_with_two_block_line();
    let err = generate_tempo_lines(state.clone(), DAY.to_string(), Some(key()), failing_invoker)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("model unreachable"), "{err}");
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], serde_json::Value::Null);
}

type Seen = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

fn scripted_invoker(
    replies: &'static [&'static str],
    seen: Seen,
) -> impl FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> {
    struct Scripted(std::sync::Mutex<std::collections::VecDeque<&'static str>>, Seen);
    impl ModelInvoker for Scripted {
        fn invoke(
            &self,
            _system: &str,
            user: &str,
            _schema: &serde_json::Value,
            _model: &str,
        ) -> anyhow::Result<serde_json::Value> {
            self.1.lock().unwrap().push(user.to_string());
            let text = self.0.lock().unwrap().pop_front().unwrap_or("x 1");
            Ok(serde_json::json!({ "text": text }))
        }
    }
    move || Ok(Box::new(Scripted(std::sync::Mutex::new(replies.iter().copied().collect()), seen)) as Box<dyn ModelInvoker>)
}

#[tokio::test(flavor = "current_thread")]
async fn unforced_generation_never_copies_english_when_the_model_fails() {
    let state = state_with_one_block_line();
    let generated = generate_tempo_lines(state.clone(), DAY.to_string(), None, failing_invoker)
        .await
        .unwrap();
    assert!(generated.is_empty());
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], serde_json::Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn path_reply_is_retried_then_stored_and_the_prompt_carries_the_descriptions() {
    let state = state_with_two_block_line();
    let seen_log = Seen::default();
    let forced = generate_tempo_lines(
        state.clone(),
        DAY.to_string(),
        Some(key()),
        scripted_invoker(&["Lagaði src/main.rs. Prófaði það.", "Lagaði villu. Prófaði það."], seen_log.clone()),
    )
    .await
    .unwrap();
    assert_eq!(forced, vec![key()]);
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], "Lagaði villu. Prófaði það.");
    let seen = seen_log.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].contains("Alpha") && seen[0].contains("Beta"), "{}", seen[0]);
    assert!(seen[1].contains("Síðasta svar var hafnað"), "{}", seen[1]);
}

#[tokio::test(flavor = "current_thread")]
async fn three_bad_replies_fail_forced_loudly_and_store_nothing_unforced() {
    let state = state_with_two_block_line();
    let bad: &'static [&'static str] = &["a 1", "b 2", "c 3"];
    let forced = generate_tempo_lines(state.clone(), DAY.to_string(), Some(key()), scripted_invoker(bad, Seen::default())).await;
    let err = forced.unwrap_err();
    assert!(err.to_string().contains("(reynt 3 sinnum)"), "{err}");
    let unforced = generate_tempo_lines(state.clone(), DAY.to_string(), None, scripted_invoker(bad, Seen::default()))
        .await
        .unwrap();
    assert!(unforced.is_empty());
    let (_, lines) = call(&state, get_day()).await;
    assert_eq!(lines[0]["text"], serde_json::Value::Null);
}

// --- Verdict text check on generated lines (spec 017 FR-19..FR-21, B6) ------

const SUMMARY: &str = "Fix login redirect";
const GOOD: &str = "Lagaði villu í innskráningu. Prófaði breytinguna.";
const VAGUE: &str = "Vann í ýmsum verkefnum. Sinnti ýmsu.";

fn with_summary(state: &Shared) -> &Shared {
    state
        .conn
        .try_lock()
        .unwrap()
        .execute(
            "INSERT INTO jira_tickets (key, summary) VALUES ('APRO-1', ?1)",
            [SUMMARY],
        )
        .unwrap();
    state
}

/// Verdict stand-in: only `GOOD` is about the ticket and specific.
fn verdict(seen: Seen) -> impl Fn(&str, &[String]) -> anyhow::Result<Vec<bool>> + Send + 'static {
    move |query, texts| {
        seen.lock().unwrap().push(query.to_string());
        Ok(texts.iter().map(|t| t == GOOD).collect())
    }
}

async fn checked(state: &Shared, replies: &'static [&'static str]) -> (Vec<String>, Vec<String>) {
    let (model, asked) = (Seen::default(), Seen::default());
    crate::daemon::daemon_tempo_lines::generate_tempo_lines_with(
        state.clone(),
        DAY.to_string(),
        None,
        scripted_invoker(replies, model.clone()),
        verdict(asked.clone()),
    )
    .await
    .unwrap();
    let model = model.lock().unwrap().clone();
    let asked = asked.lock().unwrap().clone();
    (model, asked)
}

async fn stored(state: &Shared) -> (Option<String>, Option<String>) {
    state
        .conn
        .lock()
        .await
        .query_row(
            "SELECT text, check_status FROM tempo_line_texts WHERE jira_issue = 'APRO-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn generated_vague_line_is_regenerated_once_then_flagged() {
    // catches: generation never running the check or never storing it (B6)
    let state = state_with_two_block_line();
    let (model, _) = checked(with_summary(&state), &[VAGUE, VAGUE]).await;
    assert_eq!(model.len(), 2);
    assert_eq!(stored(&state).await, (Some(VAGUE.to_string()), Some("needs_look".to_string())));
}

#[tokio::test(flavor = "current_thread")]
async fn generated_good_line_is_stored_passed_without_a_regenerate() {
    // catches: flagging unconditionally, regenerating a passing line, or asking about the wrong ticket
    let state = state_with_two_block_line();
    let (model, asked) = checked(with_summary(&state), &[GOOD]).await;
    assert_eq!(model.len(), 1);
    assert_eq!(asked[0], SUMMARY);
    assert_eq!(stored(&state).await, (Some(GOOD.to_string()), Some("passed".to_string())));
}

#[tokio::test(flavor = "current_thread")]
async fn generated_vague_line_is_replaced_by_a_passing_regenerate() {
    // catches: storing the first text after the regenerate passed
    let state = state_with_two_block_line();
    checked(with_summary(&state), &[VAGUE, GOOD]).await;
    assert_eq!(stored(&state).await, (Some(GOOD.to_string()), Some("passed".to_string())));
}

#[tokio::test(flavor = "current_thread")]
async fn generated_line_without_a_cached_ticket_summary_is_not_checked() {
    // catches: checking against an empty summary and flagging every uncached ticket
    let state = state_with_two_block_line();
    let (model, asked) = checked(&state, &[VAGUE]).await;
    assert_eq!(model.len(), 1);
    assert!(asked.is_empty());
    assert_eq!(stored(&state).await, (Some(VAGUE.to_string()), None));
}

async fn set_check_status(state: &Shared, value: &str) {
    state
        .conn
        .lock()
        .await
        .execute(
            "INSERT INTO tempo_line_texts (day, jira_issue, updated_at, check_status)
             VALUES (?1, 'APRO-1', '2026-09-30T12:00:00+00:00', ?2)",
            params![DAY, value],
        )
        .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn get_carries_check_status_needs_look_for_a_flagged_line() {
    let state = state_with_two_block_line();
    set_check_status(&state, "needs_look").await;
    let (_, lines) = call(&state, get_day()).await;
    // Catches never selecting the column (field stays null).
    assert_eq!(lines[0]["check_status"], "needs_look");
}

#[tokio::test(flavor = "current_thread")]
async fn get_carries_check_status_passed_for_a_passed_line() {
    let state = state_with_two_block_line();
    set_check_status(&state, "passed").await;
    let (_, lines) = call(&state, get_day()).await;
    // Catches mapping every non-null value to needs_look.
    assert_eq!(lines[0]["check_status"], "passed");
}

#[tokio::test(flavor = "current_thread")]
async fn get_carries_null_check_status_for_an_unchecked_line() {
    let state = state_with_two_block_line();
    let (_, lines) = call(&state, get_day()).await;
    // Catches omitting the key (web reads null) or defaulting to passed.
    assert!(lines[0].as_object().unwrap().contains_key("check_status"));
    assert_eq!(lines[0]["check_status"], serde_json::Value::Null);
}

