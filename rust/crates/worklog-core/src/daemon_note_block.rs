//! Daemon routes for note blocks (spec 019): save a note block, start the
//! background AI write of its description, report the job. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/`ApiError`.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use crate::estimate::{self, ModelInvoker};
use crate::line_text_jobs::JobState;
use crate::models::Block;
use crate::note_block_contract::*;
use crate::note_writer;
use crate::tz;

use super::daemon_tasks::{hub_error, validated_key};
use super::{with_conn, ApiError, Shared};

type MakeInvoker = Box<dyn FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> + Send>;

fn real_invoker() -> anyhow::Result<Box<dyn ModelInvoker>> {
    estimate::build_regenerate_invoker(NOTE_THINKING_TOKENS)
}

/// Saves the note block and returns it; the AI write runs in the background.
pub async fn log_note(
    State(state): State<Shared>,
    Json(body): Json<NoteBlockBody>,
) -> Result<Json<Block>, ApiError> {
    validated_key(&body.jira_issue)?;
    let today = tz::local_date(Utc::now());
    let block = with_conn(state.clone(), move |c| {
        note_writer::log_note_block(c, &body, today)
    })
    .await
    .map_err(hub_error)?;
    start_job(&state, block.id, false, real_invoker);
    Ok(Json(block))
}

pub async fn regenerate(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<RegenerateNoteBody>,
) -> Result<Json<Value>, ApiError> {
    // Check before try_start so a bad id never leaves a tracker entry.
    let is_note = with_conn(state.clone(), move |c| {
        Ok(crate::repo::get_block(c, id)?.is_some_and(|b| b.rough_note.is_some()))
    })
    .await?;
    if !is_note {
        return Ok(Json(
            json!({ "started": false, "reason": REASON_NOT_A_NOTE_BLOCK }),
        ));
    }
    Ok(if start_job(&state, id, body.force, real_invoker) {
        Json(json!({ "started": true }))
    } else {
        Json(json!({ "started": false, "reason": "already running" }))
    })
}

pub async fn status(State(state): State<Shared>, AxumPath(id): AxumPath<i64>) -> Json<Value> {
    Json(match state.note_jobs.state(&id) {
        None => json!({ "state": "idle" }),
        Some(JobState::Running) => json!({ "state": "running" }),
        Some(JobState::Done) => json!({ "state": "done" }),
        Some(JobState::Failed(reason)) => json!({ "state": "failed", "reason": reason }),
    })
}

/// `false` when a note job for `block_id` is already running.
pub(crate) fn start_job<F>(state: &Shared, block_id: i64, force: bool, make_invoker: F) -> bool
where
    F: FnOnce() -> anyhow::Result<Box<dyn ModelInvoker>> + Send + 'static,
{
    if !state.note_jobs.try_start(block_id) {
        return false;
    }
    let state = state.clone();
    let make_invoker: MakeInvoker = Box::new(make_invoker);
    tokio::spawn(async move {
        let result = run_note_job(state.clone(), block_id, force, make_invoker).await;
        state.note_jobs.finish(block_id, result);
    });
    true
}

/// prepare in `with_conn` → model call in `spawn_blocking` (no connection
/// held) → guarded commit in `with_conn`.
async fn run_note_job(
    state: Shared,
    block_id: i64,
    force: bool,
    make_invoker: MakeInvoker,
) -> std::result::Result<(), String> {
    let prep = with_conn(state.clone(), move |c| {
        Ok(note_writer::prepare_note(c, block_id))
    })
    .await
    .map_err(|e| e.to_string())??;

    let reply = tokio::task::spawn_blocking(move || {
        make_invoker()
            .map_err(|e| e.to_string())
            .and_then(|inv| note_writer::invoke_note(&prep, inv.as_ref(), NOTE_MODEL))
    })
    .await
    .map_err(|e| e.to_string())??;

    with_conn(state, move |c| {
        Ok(note_writer::commit_note(c, block_id, &reply, force))
    })
    .await
    .map_err(|e| e.to_string())?
}
