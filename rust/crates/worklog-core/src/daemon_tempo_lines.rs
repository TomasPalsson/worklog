//! Daemon routes for stored Tempo ticket lines (spec 011, B9). Child module
//! of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`.

use anyhow::Result;
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use rusqlite::OptionalExtension;

use crate::estimate::{self, ModelInvoker};
use crate::line_check;
use crate::line_text;
use crate::mirres;
use crate::tempo_line_contract::{
    MirresDay, SetTempoLineHours, SetTempoLineText, TempoLine, TempoLineKey,
};
use crate::tempo_line_writer;
use crate::tempo_lines;
use crate::verdict;

use super::{with_conn, ApiError, Shared};

pub async fn list_lines(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Vec<TempoLine>>, ApiError> {
    let lines = with_conn(state, move |c| tempo_lines::lines_for_day(c, &day)).await?;
    Ok(Json(lines))
}

/// Pulls Mirres facts for the day's tickets (network on a blocking thread,
/// outside the sqlite lock), stores them, returns the day's lines.
pub async fn refresh_mirres(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Vec<TempoLine>>, ApiError> {
    let day_for_read = day.clone();
    let issues = with_conn(state.clone(), move |c| {
        Ok(tempo_lines::lines_for_day(c, &day_for_read)?
            .into_iter()
            .map(|l| l.jira_issue)
            .collect::<Vec<_>>())
    })
    .await?;
    let day_for_fetch = day.clone();
    let rows =
        tokio::task::spawn_blocking(move || mirres::fetch_day_billing(&day_for_fetch, &issues))
            .await
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?
            .map_err(ApiError::bad_request)?;
    let lines = with_conn(state, move |c| {
        mirres::store_day(c, &day, &rows)?;
        tempo_lines::lines_for_day(c, &day)
    })
    .await?;
    Ok(Json(lines))
}

/// Every day with stored Mirres facts, newest first.
pub async fn mirres_overview(
    State(state): State<Shared>,
) -> Result<Json<Vec<MirresDay>>, ApiError> {
    Ok(Json(with_conn(state, mirres::overview).await?))
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
    generate_tempo_lines_with(state, day, force, make_invoker, verdict::match_texts).await
}

/// [`generate_tempo_lines`] with the Verdict `matcher` injected: each new
/// text gets the line check and at most one regenerate (spec 017 FR-19/20).
pub(crate) async fn generate_tempo_lines_with<F, M>(
    state: Shared,
    day: String,
    force: Option<TempoLineKey>,
    make_invoker: F,
    matcher: M,
) -> Result<Vec<TempoLineKey>>
where
    F: FnOnce() -> Result<Box<dyn ModelInvoker>> + Send + 'static,
    M: Fn(&str, &[String]) -> Result<Vec<bool>> + Send + 'static,
{
    let forced = force.is_some();
    let pending = with_conn(state.clone(), move |c| {
        tempo_lines::pending_generation(c, &day, force.as_ref())
    })
    .await?;
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    // Prepare under the lock, then drop it for every model call.
    let prepared = with_conn(state.clone(), move |c| {
        pending
            .into_iter()
            .map(|(key, _, hash)| {
                let meeting = crate::meeting_description::line_text(c, &key)?;
                let msg = tempo_line_writer::prepare(c, &key);
                let summary: Option<String> = c
                    .query_row(
                        "SELECT summary FROM jira_tickets WHERE key = ?1",
                        [&key.jira_issue],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok((key, meeting, msg, summary.unwrap_or_default(), hash))
            })
            .collect::<Result<Vec<_>>>()
    })
    .await?;
    let texts = tokio::task::spawn_blocking(move || -> Result<Vec<_>> {
        let invoker = make_invoker()?;
        let mut out = Vec::new();
        for (key, meeting, msg, summary, hash) in prepared {
            if let Some(text) = meeting {
                out.push((key, text, None, hash));
                continue;
            }
            let written = msg.and_then(|m| {
                let write = || {
                    tempo_line_writer::write(&m, &key, invoker.as_ref(), line_text::LINE_TEXT_MODEL)
                };
                let text = write()?;
                line_check::check_with_regenerate(&matcher, text, &summary, || {
                    write().map_err(anyhow::Error::msg)
                })
                .map_err(|e| e.to_string())
            });
            match written {
                Ok((text, check)) => out.push((key, text, check, hash)),
                // An explicit Generate/Regenerate fails loudly; the automatic
                // pass leaves the line without text (never English fallback).
                Err(reason) if forced => anyhow::bail!(reason),
                Err(_) => {}
            }
        }
        Ok(out)
    })
    .await??;
    with_conn(state, move |c| {
        let mut committed = Vec::new();
        for (key, text, check, hash) in texts {
            tempo_lines::commit_generated(c, &key, &text, &hash, forced)?;
            tempo_lines::set_check(c, &key, check)?;
            committed.push(key);
        }
        Ok(committed)
    })
    .await
}
