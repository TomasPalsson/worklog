//! Daemon routes for stored Tempo ticket lines (spec 011, B9). Child module
//! of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`.

use anyhow::Result;
use axum::extract::{Path as AxumPath, State};
use axum::Json;

use crate::estimate::{self, ModelInvoker};
use crate::line_text;
use crate::tempo_line_contract::{SetTempoLineHours, SetTempoLineText, TempoLine, TempoLineKey};
use crate::tempo_lines;

use super::{with_conn, ApiError, Shared};

pub async fn list_lines(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Vec<TempoLine>>, ApiError> {
    let lines = with_conn(state, move |c| tempo_lines::lines_for_day(c, &day)).await?;
    Ok(Json(lines))
}

fn line_or_not_found(line: Option<TempoLine>) -> Result<Json<TempoLine>, ApiError> {
    line.map(Json)
        .ok_or_else(|| ApiError::NotFound(anyhow::anyhow!("no such ticket line")))
}

pub async fn set_text(
    State(state): State<Shared>,
    Json(body): Json<SetTempoLineText>,
) -> Result<Json<TempoLine>, ApiError> {
    let line = with_conn(state, move |c| tempo_lines::set_text(c, &body)).await?;
    line_or_not_found(line)
}

pub async fn set_hours(
    State(state): State<Shared>,
    Json(body): Json<SetTempoLineHours>,
) -> Result<Json<TempoLine>, ApiError> {
    let line = with_conn(state, move |c| tempo_lines::set_hours(c, &body))
        .await
        .map_err(|e| {
            if e.is::<tempo_lines::InvalidHours>() {
                ApiError::bad_request(e)
            } else {
                ApiError::from(e)
            }
        })?;
    line_or_not_found(line)
}

pub async fn regenerate(
    State(state): State<Shared>,
    Json(key): Json<TempoLineKey>,
) -> Result<Json<TempoLine>, ApiError> {
    generate_tempo_lines(state.clone(), key.day.clone(), Some(key.clone()), || {
        estimate::build_regenerate_invoker(line_text::LINE_TEXT_THINKING_TOKENS)
    })
    .await?;
    let line = with_conn(state, move |c| tempo_lines::line_for(c, &key)).await?;
    line_or_not_found(line)
}

/// Generates text for the day's pending ticket lines (or just `force`'s line,
/// overwriting any text) and returns the keys committed. The model calls run
/// on a blocking thread outside the sqlite lock; the invoker is built there
/// because providers are not `Send`.
pub(crate) async fn generate_tempo_lines<F>(
    state: Shared,
    day: String,
    force: Option<TempoLineKey>,
    make_invoker: F,
) -> Result<Vec<TempoLineKey>>
where
    F: FnOnce() -> Result<Box<dyn ModelInvoker>> + Send + 'static,
{
    let forced = force.is_some();
    let (pending, previous) = with_conn(state.clone(), move |c| {
        let pending = tempo_lines::pending_generation(c, &day, force.as_ref())?;
        // The text a forced Regenerate replaces, so the model rewords it.
        let previous = match &force {
            Some(key) => tempo_lines::line_for(c, key)?.and_then(|line| line.text),
            None => None,
        };
        Ok((pending, previous))
    })
    .await?;
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let texts = tokio::task::spawn_blocking(move || -> Result<Vec<_>> {
        let invoker = make_invoker()?;
        if forced {
            // An explicit Generate/Regenerate always asks the model and fails loudly.
            return pending
                .into_iter()
                .map(|(key, descriptions, hash)| {
                    let text = tempo_lines::rewrite_text(
                        invoker.as_ref(),
                        &key,
                        &descriptions,
                        previous.as_deref(),
                        line_text::LINE_TEXT_MODEL,
                    )?;
                    Ok((key, text, hash))
                })
                .collect();
        }
        Ok(pending
            .into_iter()
            .filter_map(|(key, descriptions, hash)| {
                let text = tempo_lines::generate_text(
                    Some(invoker.as_ref()),
                    &key,
                    &descriptions,
                    line_text::LINE_TEXT_MODEL,
                );
                text.map(|text| (key, text, hash))
            })
            .collect())
    })
    .await??;
    with_conn(state, move |c| {
        let mut committed = Vec::new();
        for (key, text, hash) in texts {
            tempo_lines::commit_generated(c, &key, &text, &hash, forced)?;
            committed.push(key);
        }
        Ok(committed)
    })
    .await
}
