//! Daemon routes for a single billing line's text (spec 006, FR-26/FR-31/
//! FR-33/FR-35). Child module of `daemon.rs` (via `#[path]`) so it reuses
//! its private `with_conn`/`ApiError` — see design.md §4.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::clues_contract::BillingLineKey;
use crate::estimate;
use crate::line_text;
use crate::line_text_jobs::JobState;

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct SetTextBody {
    pub day: String,
    pub folder: String,
    pub customer: String,
    pub text: String,
}

/// Hand a billing line's text back to the owner's own words (FR-31), or,
/// with an empty `text`, reset it back to the next generated/fallback
/// value. Never validated — the owner's words are final.
pub async fn set_text(
    State(state): State<Shared>,
    Json(body): Json<SetTextBody>,
) -> Result<Json<Value>, ApiError> {
    let SetTextBody {
        day,
        folder,
        customer,
        text,
    } = body;
    with_conn(state, move |c| {
        let key = BillingLineKey {
            day,
            folder,
            customer,
        };
        line_text::set_manual(c, &key, &text)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct RegenerateBody {
    pub day: String,
    pub folder: String,
    pub customer: String,
}

/// Starts text generation for exactly one billing line in the
/// background and returns immediately (FR-33) — no model call ever
/// happens on the request path. A second regenerate for a key already
/// running is rejected with `started: false` rather than racing the
/// first. Poll `GET /billing/lines/status` for the outcome: a
/// hand-edited line (FR-31) or any other failure surfaces there as
/// `"failed"` with a `reason`, and the stored text is left untouched
/// either way (FR-35).
pub async fn regenerate(
    State(state): State<Shared>,
    Json(body): Json<RegenerateBody>,
) -> Result<Json<Value>, ApiError> {
    let RegenerateBody {
        day,
        folder,
        customer,
    } = body;
    let key = BillingLineKey {
        day,
        folder,
        customer,
    };
    if !state.line_text_jobs.try_start(key.clone()) {
        return Ok(Json(
            json!({ "started": false, "reason": "already running" }),
        ));
    }
    let job_state = state.clone();
    let job_key = key.clone();
    tokio::spawn(async move {
        let result = run_line_text_job(job_state.clone(), job_key.clone()).await;
        job_state.line_text_jobs.finish(job_key, result);
    });
    Ok(Json(json!({ "started": true })))
}

#[derive(Deserialize)]
pub struct StatusQuery {
    pub day: String,
    pub folder: String,
    pub customer: String,
}

/// Polled by the web UI while a regenerate is in flight (spec change
/// set: background regenerate). `"idle"` means nothing has ever been
/// requested for this key this daemon run — never a promise that a
/// stored text exists.
pub async fn status(State(state): State<Shared>, Query(q): Query<StatusQuery>) -> Json<Value> {
    let key = BillingLineKey {
        day: q.day,
        folder: q.folder,
        customer: q.customer,
    };
    Json(match state.line_text_jobs.state(&key) {
        None => json!({ "state": "idle" }),
        Some(JobState::Running) => json!({ "state": "running" }),
        Some(JobState::Done) => json!({ "state": "done" }),
        Some(JobState::Failed(reason)) => json!({ "state": "failed", "reason": reason }),
    })
}

/// Generates every distinct billing line's text for `day`, one
/// [`run_line_text_job`] at a time — used by `run_estimate`'s line-texts
/// pass so the sqlite mutex is never held across any of those calls'
/// `claude -p` round trips. Mirrors [`line_text::generate_for_day`]'s
/// skip-hand-edited/report-the-rest shape.
pub(crate) async fn generate_day(state: Shared, day: String) -> line_text::LineTextReport {
    let day_for_keys = day.clone();
    let keys = with_conn(state.clone(), move |c| {
        line_text::distinct_keys(c, &day_for_keys)
    })
    .await;
    let keys = match keys {
        Ok(keys) => keys,
        Err(_) => {
            return line_text::LineTextReport {
                generated: Vec::new(),
                not_generated: Vec::new(),
            }
        }
    };

    let mut report = line_text::LineTextReport {
        generated: Vec::new(),
        not_generated: Vec::new(),
    };
    for key in keys {
        match run_line_text_job(state.clone(), key.clone()).await {
            Ok(()) => report.generated.push(key),
            Err(reason) if reason == "hand-edited" => {}
            Err(reason) => report.not_generated.push((key, reason)),
        }
    }
    report
}

/// Runs [`line_text::prepare_line`] / [`line_text::invoke_line`] /
/// [`line_text::commit_line`] back to back, dropping the sqlite
/// connection lock for the whole `claude -p` round trip in between (the
/// daemon holds exactly one connection, behind a mutex — see
/// `with_conn`'s own doc comment). `Err("hand-edited")` when the line
/// was (or became, mid-flight) hand-edited; any other `Err` is the
/// invoker's own failure message.
async fn run_line_text_job(state: Shared, key: BillingLineKey) -> std::result::Result<(), String> {
    let prep_key = key.clone();
    let prep_result = with_conn(state.clone(), move |c| {
        Ok(line_text::prepare_line(c, &prep_key))
    })
    .await
    .map_err(|e| e.to_string())?;
    let prep = prep_result?;

    let (prep, reply) = tokio::task::spawn_blocking(move || {
        let reply = estimate::build_thinking_invoker(line_text::LINE_TEXT_THINKING_TOKENS)
            .map_err(|e| e.to_string())
            .and_then(|inv| {
                line_text::invoke_line(&prep, inv.as_ref(), line_text::LINE_TEXT_MODEL)
            });
        (prep, reply)
    })
    .await
    .map_err(|e| e.to_string())?;

    with_conn(state, move |c| Ok(line_text::commit_line(c, &prep, reply)))
        .await
        .map_err(|e| e.to_string())?
}
