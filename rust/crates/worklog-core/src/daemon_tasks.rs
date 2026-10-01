//! Daemon routes for My Tasks, Jira transitions/comments and the AI ticket
//! draft (spec 012). Child module of `daemon.rs` (via `#[path]`) so it
//! reuses its private `with_conn`/`ApiError`. Jira and model calls run on
//! the blocking pool with the sqlite lock released: read, call, write.

use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::collectors::jira::{self, JiraAuth};
use crate::estimate::{self, ModelInvoker};
use crate::tempo_hub_contract::{
    CommentBody, HubError, TasksResponse, TicketDraft, TicketStatus, Transition, TransitionBody,
    COMMENT_MAX_CHARS,
};
use crate::{line_text, repo, secrets, task_board, task_draft, tz};

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct TasksQuery {
    monday: Option<String>,
}

fn hub_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<HubError>() {
        Some(HubError::InvalidInput(_)) => ApiError::BadRequest(error),
        Some(HubError::NotFound(_)) => ApiError::NotFound(error),
        Some(HubError::Upstream { .. }) => ApiError::BadGateway(error),
        None => ApiError::from(error),
    }
}

fn invalid_input(message: impl Into<String>) -> ApiError {
    hub_error(HubError::InvalidInput(message.into()).into())
}

fn validated_key(key: &str) -> Result<(), ApiError> {
    let valid = key.split_once('-').is_some_and(|(project, number)| {
        project.starts_with(|c: char| c.is_ascii_uppercase())
            && project
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
    });
    if valid {
        Ok(())
    } else {
        Err(invalid_input(format!("`{key}` is not a Jira ticket key")))
    }
}

fn validated_comment(text: &str) -> Result<&str, ApiError> {
    let text = text.trim();
    match text.chars().count() {
        0 => Err(invalid_input("comment is empty")),
        length if length > COMMENT_MAX_CHARS => Err(invalid_input(format!(
            "comment is {length} characters; the limit is {COMMENT_MAX_CHARS}"
        ))),
        _ => Ok(text),
    }
}

fn requested_monday(raw: Option<&str>, today: NaiveDate) -> Result<NaiveDate, ApiError> {
    let Some(raw) = raw else {
        return Ok(today - Duration::days(today.weekday().num_days_from_monday().into()));
    };
    match raw.parse::<NaiveDate>() {
        Ok(monday) if monday.weekday() == Weekday::Mon => Ok(monday),
        _ => Err(invalid_input(format!(
            "`{raw}` is not a Monday (YYYY-MM-DD)"
        ))),
    }
}

async fn jira_call<T, F>(auth: JiraAuth, call: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&JiraAuth, &Client) -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || call(&auth, &crate::http::client()?))
        .await
        .context("spawn_blocking")?
        .map_err(hub_error)
}

pub async fn list_tasks(
    State(state): State<Shared>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let today = tz::local_date(Utc::now());
    let monday = requested_monday(query.monday.as_deref(), today)?;
    let base_url = secrets::get("jira_base_url").ok().flatten();
    let response = with_conn(state, move |c| {
        task_board::tasks(c, monday, today, base_url.as_deref())
    })
    .await?;
    Ok(Json(response))
}

pub async fn list_transitions(
    State(_state): State<Shared>,
    AxumPath(key): AxumPath<String>,
) -> Result<Json<Vec<Transition>>, ApiError> {
    validated_key(&key)?;
    let auth = JiraAuth::from_secrets()?;
    let transitions = jira_call(auth, move |auth, client| {
        jira::list_transitions_with(auth, &key, client)
    })
    .await?;
    Ok(Json(transitions))
}

pub async fn transition(
    State(state): State<Shared>,
    AxumPath(key): AxumPath<String>,
    Json(body): Json<TransitionBody>,
) -> Result<Json<TicketStatus>, ApiError> {
    validated_key(&key)?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(
        apply_transition(state, auth, key, body.transition_id).await?,
    ))
}

pub async fn comment(
    State(_state): State<Shared>,
    AxumPath(key): AxumPath<String>,
    Json(body): Json<CommentBody>,
) -> Result<Json<Value>, ApiError> {
    validated_key(&key)?;
    let text = validated_comment(&body.text)?.to_owned();
    let auth = JiraAuth::from_secrets()?;
    jira_call(auth, move |auth, client| {
        jira::add_comment_with(auth, &key, &text, client)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn draft(
    State(state): State<Shared>,
    AxumPath(key): AxumPath<String>,
) -> Result<Json<TicketDraft>, ApiError> {
    validated_key(&key)?;
    let auth = JiraAuth::from_secrets()?;
    let draft = draft_ticket(state, auth, key, || {
        estimate::build_regenerate_invoker(line_text::LINE_TEXT_THINKING_TOKENS)
    })
    .await?;
    Ok(Json(draft))
}

pub(crate) async fn apply_transition(
    state: Shared,
    auth: JiraAuth,
    key: String,
    transition_id: String,
) -> Result<TicketStatus, ApiError> {
    let moved = key.clone();
    let (status, status_category) = jira_call(auth, move |auth, client| {
        jira::transition_with(auth, &moved, &transition_id, client)?;
        jira::fetch_status_with(auth, &moved, client)
    })
    .await?;
    let stored = (key.clone(), status.clone());
    with_conn(state, move |c| {
        repo::set_ticket_status(c, &stored.0, &stored.1, status_category)
    })
    .await?;
    Ok(TicketStatus {
        key,
        status,
        status_category,
    })
}

pub(crate) async fn draft_ticket<F>(
    state: Shared,
    auth: JiraAuth,
    key: String,
    make_invoker: F,
) -> Result<TicketDraft, ApiError>
where
    F: FnOnce() -> Result<Box<dyn ModelInvoker>> + Send + 'static,
{
    let today = tz::local_date(Utc::now());
    let prepared = key.clone();
    let prep = with_conn(state, move |c| {
        task_draft::prepare_draft(c, &prepared, today)
    })
    .await?;
    let listed = key;
    let transitions = jira_call(auth, move |auth, client| {
        jira::list_transitions_with(auth, &listed, client)
    })
    .await?;
    let draft = tokio::task::spawn_blocking(move || {
        task_draft::draft_with(
            make_invoker()?.as_ref(),
            &prep,
            &transitions,
            line_text::LINE_TEXT_MODEL,
        )
    })
    .await
    .context("spawn_blocking")??;
    Ok(draft)
}
