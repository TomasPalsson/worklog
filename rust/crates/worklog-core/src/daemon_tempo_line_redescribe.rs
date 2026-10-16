//! Daemon route for "Regenerate with prompt" on a super block (spec 027).
//! Child module of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`.

use axum::extract::State;
use axum::Json;

use crate::estimate::{self, ModelInvoker};
use crate::line_text::{LINE_TEXT_MODEL, LINE_TEXT_THINKING_TOKENS};
use crate::redescribe_contract::*;
use crate::tempo_line_contract::{
    RedescribeTempoLineBody, TempoLine, TempoLineKey, REASON_TEXT_CHANGED,
};
use crate::tempo_line_redescribe;

use super::{with_conn, ApiError, Shared};

/// Lets route-level tests answer the model call without a provider.
#[cfg(test)]
pub(crate) static TEST_REPLY: std::sync::Mutex<Option<serde_json::Value>> =
    std::sync::Mutex::new(None);

fn real_invoker() -> anyhow::Result<Box<dyn ModelInvoker>> {
    #[cfg(test)]
    if let Some(reply) = TEST_REPLY.lock().unwrap().clone() {
        return Ok(Box::new(estimate::FixedInvoker(reply)));
    }
    estimate::build_regenerate_invoker(LINE_TEXT_THINKING_TOKENS)
}

pub async fn redescribe(
    State(state): State<Shared>,
    Json(body): Json<RedescribeTempoLineBody>,
) -> Result<Json<TempoLine>, ApiError> {
    let instruction = body.instruction.trim().to_string();
    if instruction.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            REASON_EMPTY_INSTRUCTION
        )));
    }
    if instruction.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            REASON_INSTRUCTION_TOO_LONG
        )));
    }

    let key = TempoLineKey {
        day: body.day,
        jira_issue: body.jira_issue,
    };
    let not_found = || {
        ApiError::NotFound(anyhow::anyhow!(
            "no Tempo line for {} on {}",
            key.jira_issue,
            key.day
        ))
    };

    let prep = {
        let key = key.clone();
        with_conn(state.clone(), move |c| {
            tempo_line_redescribe::prepare(c, &key, &instruction)
        })
        .await?
    }
    .ok_or_else(not_found)?;

    let (prep, text) = tokio::task::spawn_blocking(move || {
        let text = tempo_line_redescribe::write(&prep, &*real_invoker()?, LINE_TEXT_MODEL)
            .map_err(anyhow::Error::msg)?;
        anyhow::Ok((prep, text))
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(ApiError::BadGateway)?;

    let line = with_conn(state, move |c| {
        tempo_line_redescribe::commit(c, &prep, &text)
    })
    .await
    .map_err(commit_error)?;
    line.map(Json).ok_or_else(not_found)
}

pub(crate) fn commit_error(error: anyhow::Error) -> ApiError {
    if error.to_string() == REASON_TEXT_CHANGED {
        ApiError::Conflict(error)
    } else {
        ApiError::from(error)
    }
}
