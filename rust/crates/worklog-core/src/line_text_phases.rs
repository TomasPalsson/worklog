//! Prepare/invoke/commit phases for one billing line's text — split so
//! a caller (the daemon) can drop its sqlite connection lock for the
//! whole `claude -p` round trip, mirroring `estimate.rs`'s
//! `prepare_block_estimate` / `invoke_block_estimate` /
//! `commit_block_estimate`. A child module of `line_text.rs` (via
//! `#[path]`) so it can reuse its private helpers.

use crate::clues_contract::{BillingLineKey, LineTextOrigin};
use crate::clues_send;
use crate::estimate::ModelInvoker;
use rusqlite::Connection;
use serde_json::Value;

use super::{
    line_text_schema, stored_origin, unwrap_nested_text, upsert_generated, validate,
    SYSTEM_PROMPT_IS,
};

/// Everything [`invoke`] needs, read from sqlite up front. Fields are
/// private: callers only ferry this between phases. `Clone` so a batched
/// caller (`daemon_line_text::generate_day`, SLICE T11 round 2) can keep
/// its own copy across the `invoke_many` call and still commit each one
/// afterward.
#[derive(Clone)]
pub struct Prep {
    key: BillingLineKey,
    user_msg: String,
}

/// Phase 1: read the stored origin — skipping a hand-edited line
/// (FR-31) before ever paying for a model call — and build the line's
/// `DescriptionInput`.
pub fn prepare(conn: &Connection, key: &BillingLineKey) -> std::result::Result<Prep, String> {
    if stored_origin(conn, key).map_err(|e| e.to_string())? == Some(LineTextOrigin::Manual) {
        return Err("hand-edited".to_string());
    }
    let input = clues_send::build_line_input(conn, key).map_err(|e| e.to_string())?;
    let user_msg = serde_json::to_string(&input).map_err(|e| e.to_string())?;
    Ok(Prep {
        key: key.clone(),
        user_msg,
    })
}

/// Phase 2: the LLM round trip. Deliberately takes no [`Connection`] —
/// this is the call that can block for the invoker's whole timeout.
pub fn invoke(
    prep: &Prep,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> std::result::Result<String, String> {
    reply_to_text(invoker.invoke(SYSTEM_PROMPT_IS, &prep.user_msg, &line_text_schema(), model))
}

/// Phase 2, batched: the same round trip as [`invoke`] for every `prep`,
/// through one `invoke_many` call so an invoker that overrides it
/// (`ClaudeSubprocess`, `LiteLLMInvoker`) can run several concurrently —
/// `preps`' order is preserved in the returned `Vec` regardless of which
/// call finishes first (SLICE T11 round 2; `daemon_line_text::generate_day`
/// is the only caller).
pub fn invoke_many(
    preps: &[Prep],
    invoker: &dyn ModelInvoker,
    model: &str,
) -> Vec<std::result::Result<String, String>> {
    let users: Vec<String> = preps.iter().map(|p| p.user_msg.clone()).collect();
    invoker
        .invoke_many(SYSTEM_PROMPT_IS, &users, &line_text_schema(), model)
        .into_iter()
        .map(reply_to_text)
        .collect()
}

fn reply_to_text(reply: anyhow::Result<Value>) -> std::result::Result<String, String> {
    let reply = reply.map_err(|e| e.to_string())?;
    let text = reply
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "reply missing `text` field".to_string())?;
    Ok(unwrap_nested_text(text))
}

/// Phase 3: re-checks the row is still not manual — an edit may have
/// landed while the model call was in flight, and that must never be
/// overwritten (FR-31) — then validates and persists `reply`.
pub fn commit(
    conn: &Connection,
    prep: &Prep,
    reply: std::result::Result<String, String>,
) -> std::result::Result<(), String> {
    if stored_origin(conn, &prep.key).map_err(|e| e.to_string())? == Some(LineTextOrigin::Manual) {
        return Err("hand-edited".to_string());
    }
    let validated = validate(&reply?)?;
    upsert_generated(conn, &prep.key, &validated).map_err(|e| e.to_string())
}

#[path = "line_text_phases_test.rs"]
#[cfg(test)]
mod tests;
