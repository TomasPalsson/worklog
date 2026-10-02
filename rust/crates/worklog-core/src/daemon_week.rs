//! Daemon routes for the Tempo week pull and the week close-out (spec 012).
//! Child module of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`. Tempo calls run on the blocking pool with the
//! sqlite lock released: call, then one write.

use anyhow::Context;
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::{Duration, NaiveDate, SecondsFormat, Utc};
use serde::Deserialize;

use crate::collectors::tempo::{self, TempoAuth};
use crate::tempo_hub_contract::{PullReport, WeekCloseout};
use crate::{tempo_remote, week_closeout};

use super::daemon_tasks::{hub_error, parse_monday};
use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct PullBody {
    monday: String,
}

pub async fn pull(
    State(state): State<Shared>,
    Json(body): Json<PullBody>,
) -> Result<Json<PullReport>, ApiError> {
    let monday = parse_monday(&body.monday)?;
    let auth = TempoAuth::from_secrets()?;
    Ok(Json(pull_week(state, auth, monday).await?))
}

pub async fn closeout(
    State(state): State<Shared>,
    AxumPath(monday): AxumPath<String>,
) -> Result<Json<WeekCloseout>, ApiError> {
    let monday = parse_monday(&monday)?;
    Ok(Json(
        with_conn(state, move |c| week_closeout::week_closeout(c, monday)).await?,
    ))
}

pub(crate) async fn pull_week(
    state: Shared,
    auth: TempoAuth,
    monday: NaiveDate,
) -> Result<PullReport, ApiError> {
    let sunday = monday + Duration::days(6);
    let (worklogs, schedule) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let client = crate::http::client()?;
        let account = tempo::resolve_account_id(&auth.author, &client)?;
        Ok((
            tempo::list_worklogs_with(&auth, &account, monday, sunday, &client)?,
            tempo::user_schedule_with(&auth, &account, monday, sunday, &client)?,
        ))
    })
    .await
    .context("spawn_blocking")?
    .map_err(hub_error)?;
    let pulled_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    Ok(with_conn(state, move |c| {
        tempo_remote::store_week(c, monday, &worklogs, &schedule, &pulled_at)
    })
    .await?)
}
