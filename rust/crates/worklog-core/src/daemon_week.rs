//! Daemon routes for the Tempo week pull and the week close-out (spec 012).
//! Child module of `daemon.rs` (via `#[path]`) so it reuses its private
//! `with_conn`/`ApiError`. Tempo calls run on the blocking pool with the
//! sqlite lock released: call, then one write.

use anyhow::Context;
use axum::extract::{Path as AxumPath, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, SecondsFormat, Utc, Weekday};
use serde::Deserialize;

use crate::collectors::tempo::{self, TempoAuth};
use crate::tempo_hub_contract::{HubError, PullReport, WeekCloseout};
use crate::{tempo_remote, week_closeout};

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct PullBody {
    monday: String,
}

fn parse_monday(raw: &str) -> Result<NaiveDate, ApiError> {
    match raw.parse::<NaiveDate>() {
        Ok(monday) if monday.weekday() == Weekday::Mon => Ok(monday),
        _ => Err(ApiError::BadRequest(
            HubError::InvalidInput(format!("`{raw}` is not a Monday (YYYY-MM-DD)")).into(),
        )),
    }
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
    .map_err(upstream_as_bad_gateway)?;
    let pulled_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    Ok(with_conn(state, move |c| {
        tempo_remote::store_week(c, monday, &worklogs, &schedule, &pulled_at)
    })
    .await?)
}

fn upstream_as_bad_gateway(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<HubError>() {
        Some(HubError::Upstream { .. }) => ApiError::BadGateway(error),
        _ => ApiError::from(error),
    }
}
