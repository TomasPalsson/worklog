//! Daemon routes for a single billing line's text (spec 006, FR-26/FR-31/
//! FR-33/FR-35). Child module of `daemon.rs` (via `#[path]`) so it reuses
//! its private `with_conn`/`ApiError` — see design.md §4.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::clues_contract::BillingLineKey;
use crate::estimate;
use crate::line_text;

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

/// Re-run text generation for exactly one billing line (FR-33). A hand-
/// edited line comes back `generated: false` with `reason: "hand-edited"`
/// and is left untouched; any other failure also leaves the stored text
/// untouched (FR-35). Always 200 — the caller distinguishes success from
/// "not generated" via the `generated` field, not the HTTP status.
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
    let result = run_line_text_job(state, key).await;
    Ok(Json(match result {
        Ok(()) => json!({ "generated": true }),
        Err(reason) => json!({ "generated": false, "reason": reason }),
    }))
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
