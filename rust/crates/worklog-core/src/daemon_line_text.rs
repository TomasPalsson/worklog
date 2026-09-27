//! Daemon routes for a single billing line's text (spec 006, FR-26/FR-31/
//! FR-33/FR-35). Child module of `daemon.rs` (via `#[path]`) so it reuses
//! its private `with_conn`/`ApiError` — see design.md §4.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::clues_contract::BillingLineKey;
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
    let result = with_conn(state, move |c| {
        let key = BillingLineKey {
            day,
            folder,
            customer,
        };
        Ok(line_text::generate_line_with_default_provider(
            c,
            &key,
            line_text::LINE_TEXT_MODEL,
        ))
    })
    .await?;
    Ok(Json(match result {
        Ok(()) => json!({ "generated": true }),
        Err(reason) => json!({ "generated": false, "reason": reason }),
    }))
}
