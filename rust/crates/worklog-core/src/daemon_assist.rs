//! Daemon routes for the Jira assistant (spec 013). Child module of
//! `daemon.rs` (via `#[path]`); Jira calls run on the blocking pool.

use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, State};
use axum::Json;

use crate::collectors::jira::{self, JiraAuth};
use crate::jira_assist_contract::{
    AccountSuggestion, AllowedAccount, AssistCreateBody, AssistCreated, MoveBody, RelearnReport,
    StartResult, StatusHint, SuggestBody, TicketView, CREATE_ISSUE_TYPE, RELEARN_LIMIT,
    WRITE_PROJECT,
};
use crate::tempo_hub_contract::{HubError, TicketStatus};
use crate::{account_clues, repo, secrets, status_hints, ticket_assist};

use super::daemon_tasks::hub_error;
use super::{with_conn, ApiError, Shared};

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
        Err(hub_error(
            HubError::InvalidInput(format!("`{key}` is not a Jira ticket key")).into(),
        ))
    }
}

fn optional_field_id() -> Option<String> {
    secrets::get("jira_account_field_id")
        .ok()
        .flatten()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn account_field_id() -> Result<String, ApiError> {
    optional_field_id().ok_or_else(|| {
        hub_error(
            HubError::InvalidInput(
                "no Jira account field is configured; set `jira_account_field_id`".into(),
            )
            .into(),
        )
    })
}

async fn jira_call<T, F>(auth: JiraAuth, call: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&JiraAuth, &reqwest::blocking::Client) -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || call(&auth, &crate::http::client()?))
        .await
        .context("spawn_blocking")?
        .map_err(hub_error)
}

pub(super) async fn view_ticket(
    auth: JiraAuth,
    field_id: Option<String>,
    key: String,
) -> Result<TicketView, ApiError> {
    jira_call(auth, move |auth, client| {
        let detail = jira::fetch_detail_with(auth, &key, client)?;
        let account = match field_id {
            Some(field_id) => jira::fetch_account_with(auth, &key, &field_id, client)?,
            None => None,
        };
        Ok(TicketView {
            detail,
            account_id: account.as_ref().map(|a| a.id.clone()),
            account_name: account.map(|a| a.name),
        })
    })
    .await
}

pub(super) async fn start(
    auth: JiraAuth,
    field_id: Option<String>,
    key: String,
) -> Result<StartResult, ApiError> {
    jira_call(auth, move |auth, client| {
        ticket_assist::start_ticket_with(auth, field_id.as_deref(), &key, client)
    })
    .await
}

pub(super) async fn move_status(
    state: Shared,
    auth: JiraAuth,
    key: String,
    to_status: String,
) -> Result<TicketStatus, ApiError> {
    let moved = jira_call(auth, move |auth, client| {
        ticket_assist::move_ticket_with(auth, &key, &to_status, client)
    })
    .await?;
    let stored = moved.clone();
    with_conn(state, move |c| {
        repo::set_ticket_status(c, &stored.key, &stored.status, stored.status_category)
    })
    .await?;
    Ok(moved)
}

pub(super) async fn allowed_list(
    auth: JiraAuth,
    field_id: String,
) -> Result<Vec<AllowedAccount>, ApiError> {
    jira_call(auth, move |auth, client| {
        jira::allowed_accounts_with(auth, WRITE_PROJECT, CREATE_ISSUE_TYPE, &field_id, client)
    })
    .await
}

pub(super) async fn suggest_accounts(
    state: Shared,
    auth: JiraAuth,
    field_id: String,
    text: String,
) -> Result<Vec<AccountSuggestion>, ApiError> {
    let allowed = allowed_list(auth.clone(), field_id.clone()).await?;
    let empty = with_conn(state.clone(), |c| {
        c.query_row("SELECT NOT EXISTS (SELECT 1 FROM account_clues)", [], |r| {
            r.get::<_, bool>(0)
        })
        .map_err(Into::into)
    })
    .await?;
    if empty {
        relearn_accounts(state.clone(), auth, field_id).await?;
    }
    Ok(with_conn(state, move |c| account_clues::suggest(c, &text, &allowed)).await?)
}

pub(super) async fn relearn_accounts(
    state: Shared,
    auth: JiraAuth,
    field_id: String,
) -> Result<RelearnReport, ApiError> {
    let tickets = jira_call(auth, move |auth, client| {
        jira::search_accounted_with(auth, &field_id, RELEARN_LIMIT, client)
    })
    .await?;
    with_conn(state, move |c| account_clues::relearn(c, &tickets))
        .await
        .map_err(hub_error)
}

pub(super) async fn create_assisted(
    state: Shared,
    auth: JiraAuth,
    field_id: String,
    body: AssistCreateBody,
) -> Result<AssistCreated, ApiError> {
    // ponytail: holds the sqlite lock across the Jira calls; split into prepare/record if it ever contends.
    tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        ticket_assist::assist_create_with(&conn, &auth, &field_id, &body, &crate::http::client()?)
    })
    .await
    .context("spawn_blocking")?
    .map_err(hub_error)
}

pub async fn view(AxumPath(key): AxumPath<String>) -> Result<Json<TicketView>, ApiError> {
    validated_key(&key)?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(view_ticket(auth, optional_field_id(), key).await?))
}

pub async fn start_route(AxumPath(key): AxumPath<String>) -> Result<Json<StartResult>, ApiError> {
    validated_key(&key)?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(start(auth, optional_field_id(), key).await?))
}

pub async fn move_route(
    State(state): State<Shared>,
    AxumPath(key): AxumPath<String>,
    Json(body): Json<MoveBody>,
) -> Result<Json<TicketStatus>, ApiError> {
    validated_key(&key)?;
    if !crate::jira_assist_contract::is_writable_key(&key) {
        return Err(hub_error(
            HubError::InvalidInput(format!("{key} is not a {WRITE_PROJECT} ticket")).into(),
        ));
    }
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(move_status(state, auth, key, body.to_status).await?))
}

pub async fn assist_create(
    State(state): State<Shared>,
    Json(body): Json<AssistCreateBody>,
) -> Result<Json<AssistCreated>, ApiError> {
    let field_id = account_field_id()?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(create_assisted(state, auth, field_id, body).await?))
}

pub async fn allowed() -> Result<Json<Vec<AllowedAccount>>, ApiError> {
    let field_id = account_field_id()?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(allowed_list(auth, field_id).await?))
}

pub async fn suggest(
    State(state): State<Shared>,
    Json(body): Json<SuggestBody>,
) -> Result<Json<Vec<AccountSuggestion>>, ApiError> {
    let field_id = account_field_id()?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(
        suggest_accounts(state, auth, field_id, body.text).await?,
    ))
}

pub async fn relearn(State(state): State<Shared>) -> Result<Json<RelearnReport>, ApiError> {
    let field_id = account_field_id()?;
    let auth = JiraAuth::from_secrets()?;
    Ok(Json(relearn_accounts(state, auth, field_id).await?))
}

pub async fn hints(State(state): State<Shared>) -> Result<Json<Vec<StatusHint>>, ApiError> {
    Ok(Json(with_conn(state, status_hints::done_hints).await?))
}
