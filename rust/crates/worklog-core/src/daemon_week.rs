//! Daemon routes for the Tempo week pull and the week close-out (spec 012).
//! Child module of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`. Tempo calls run on the blocking pool with the
//! sqlite lock released: call, then one write.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::NaiveDate;
use serde::Deserialize;

use crate::collectors::tempo::TempoAuth;
use crate::tempo_hub_contract::{PullReport, WeekCloseout};

use super::{ApiError, Shared};

#[derive(Deserialize)]
pub struct PullBody {
    monday: String,
}

pub async fn pull(
    State(_state): State<Shared>,
    Json(_body): Json<PullBody>,
) -> Result<Json<PullReport>, ApiError> {
    todo!()
}

pub async fn closeout(
    State(_state): State<Shared>,
    AxumPath(_monday): AxumPath<String>,
) -> Result<Json<WeekCloseout>, ApiError> {
    todo!()
}

pub(crate) async fn pull_week(
    _state: Shared,
    _auth: TempoAuth,
    _monday: NaiveDate,
) -> Result<PullReport, ApiError> {
    todo!()
}
