//! Daemon route for "Regenerate with prompt" (spec 026). Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/`ApiError`.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::Value;

use crate::estimate::{self, ModelInvoker};
use crate::note_block_contract::{NOTE_MODEL, NOTE_THINKING_TOKENS};
use crate::redescribe;
use crate::redescribe_contract::*;

use super::{with_conn, ApiError, Shared};

/// Lets route-level tests answer the model call without a provider.
#[cfg(test)]
pub(crate) static TEST_REPLY: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

fn real_invoker() -> anyhow::Result<Box<dyn ModelInvoker>> {
    #[cfg(test)]
    if let Some(reply) = TEST_REPLY.lock().unwrap().clone() {
        return Ok(Box::new(estimate::FixedInvoker(reply)));
    }
    estimate::build_regenerate_invoker(NOTE_THINKING_TOKENS)
}

/// `redescribe::invoke` wants a sized invoker; the factory hands back a box.
struct BoxedInvoker(Box<dyn ModelInvoker>);

impl ModelInvoker for BoxedInvoker {
    fn invoke(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        model: &str,
    ) -> anyhow::Result<Value> {
        self.0.invoke(system, user, schema, model)
    }
}

pub async fn redescribe(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<RedescribeBody>,
) -> Result<Json<RedescribeResult>, ApiError> {
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

    let prep = with_conn(state.clone(), move |c| {
        redescribe::prepare(c, id, &instruction)
    })
    .await
    .map_err(prepare_error)?;

    let (prep, description) = tokio::task::spawn_blocking(move || {
        let invoker = BoxedInvoker(real_invoker()?);
        let description = redescribe::invoke(&prep, &invoker, NOTE_MODEL)?;
        anyhow::Ok((prep, description))
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(ApiError::BadGateway)?;

    let result = with_conn(state, move |c| redescribe::commit(c, &prep, &description)).await?;
    Ok(Json(result))
}

fn prepare_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if [REASON_PERSONAL, REASON_NOT_DESCRIBED, REASON_NOTE_BLOCK].contains(&message.as_str()) {
        ApiError::BadRequest(error)
    } else if message.ends_with("not found") {
        ApiError::NotFound(error)
    } else {
        ApiError::from(error)
    }
}
