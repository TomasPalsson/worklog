//! Daemon routes for My Tasks, Jira transitions/comments and the AI ticket
//! draft (spec 012). Child module of `daemon.rs` (via `#[path]`) so it
//! reuses its private `with_conn`/`ApiError`.

use anyhow::Result;
use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::collectors::jira::JiraAuth;
use crate::estimate::ModelInvoker;
use crate::tempo_hub_contract::{
    CommentBody, TasksResponse, TicketDraft, TicketStatus, Transition, TransitionBody,
};

use super::{ApiError, Shared};

#[derive(Deserialize)]
pub struct TasksQuery {
    monday: Option<String>,
}

pub async fn list_tasks(
    State(_state): State<Shared>,
    Query(_query): Query<TasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    todo!("list_tasks")
}

pub async fn list_transitions(
    State(_state): State<Shared>,
    AxumPath(_key): AxumPath<String>,
) -> Result<Json<Vec<Transition>>, ApiError> {
    todo!("list_transitions")
}

pub async fn transition(
    State(_state): State<Shared>,
    AxumPath(_key): AxumPath<String>,
    Json(_body): Json<TransitionBody>,
) -> Result<Json<TicketStatus>, ApiError> {
    todo!("transition")
}

pub async fn comment(
    State(_state): State<Shared>,
    AxumPath(_key): AxumPath<String>,
    Json(_body): Json<CommentBody>,
) -> Result<Json<Value>, ApiError> {
    todo!("comment")
}

pub async fn draft(
    State(_state): State<Shared>,
    AxumPath(_key): AxumPath<String>,
) -> Result<Json<TicketDraft>, ApiError> {
    todo!("draft")
}

pub(crate) async fn apply_transition(
    _state: Shared,
    _auth: JiraAuth,
    _key: String,
    _transition_id: String,
) -> Result<TicketStatus, ApiError> {
    todo!("apply_transition")
}

pub(crate) async fn draft_ticket<F>(
    _state: Shared,
    _auth: JiraAuth,
    _key: String,
    _make_invoker: F,
) -> Result<TicketDraft, ApiError>
where
    F: FnOnce() -> Result<Box<dyn ModelInvoker>> + Send + 'static,
{
    todo!("draft_ticket")
}
