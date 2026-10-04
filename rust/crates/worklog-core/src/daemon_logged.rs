//! Daemon routes for the Logged section (spec 014). Child module of
//! `daemon.rs` so it reuses its private `with_conn`/`ApiError`. The Tempo
//! fetch runs on the blocking pool with the sqlite lock released.

use anyhow::{anyhow, Context};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{NaiveDate, SecondsFormat, Utc};
use serde::Deserialize;

use crate::collectors::tempo::{self, TempoAuth};
use crate::logged_contract::{
    DayBody, DismissBody, LoggedDay, LoggedRange, RangeBody, DISMISS_REASON_MAX_CHARS,
    LOGGED_MAX_RANGE_DAYS,
};
use crate::{logged, tempo_remote, tz};

use super::daemon_tasks::hub_error;
use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct RangeQuery {
    from: String,
    to: String,
}

fn bad_request(message: String) -> ApiError {
    ApiError::BadRequest(anyhow!(message))
}

fn parse_day(raw: &str) -> Result<NaiveDate, ApiError> {
    raw.parse()
        .map_err(|_| bad_request(format!("`{raw}` is not a date (YYYY-MM-DD)")))
}

fn parse_range(from: &str, to: &str) -> Result<(NaiveDate, NaiveDate), ApiError> {
    let (from, to) = (parse_day(from)?, parse_day(to)?);
    if from > to {
        return Err(bad_request("`from` is after `to`".to_string()));
    }
    if (to - from).num_days() + 1 > LOGGED_MAX_RANGE_DAYS {
        return Err(bad_request(format!(
            "range is longer than {LOGGED_MAX_RANGE_DAYS} days"
        )));
    }
    Ok((from, to))
}

fn today() -> NaiveDate {
    tz::local_date(Utc::now())
}

pub async fn get_range(
    State(state): State<Shared>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<LoggedRange>, ApiError> {
    let (from, to) = parse_range(&q.from, &q.to)?;
    let today = today();
    Ok(Json(
        with_conn(state, move |c| logged::logged_range(c, from, to, today)).await?,
    ))
}

pub async fn pull_range(
    State(state): State<Shared>,
    Json(body): Json<RangeBody>,
) -> Result<Json<LoggedRange>, ApiError> {
    let (from, to) = parse_range(&body.from, &body.to)?;
    let auth = TempoAuth::from_secrets()?;
    Ok(Json(pull_range_with(state, auth, from, to).await?))
}

pub(crate) async fn pull_range_with(
    state: Shared,
    auth: TempoAuth,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<LoggedRange, ApiError> {
    let (worklogs, schedule) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let client = crate::http::client()?;
        let account = tempo::resolve_account_id(&auth.author, &client)?;
        Ok((
            tempo::list_worklogs_with(&auth, &account, from, to, &client)?,
            tempo::user_schedule_with(&auth, &account, from, to, &client)?,
        ))
    })
    .await
    .context("spawn_blocking")?
    .map_err(hub_error)?;
    let pulled_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let today = today();
    Ok(with_conn(state, move |c| {
        tempo_remote::store_range(c, from, to, &worklogs, &schedule, &pulled_at)?;
        logged::logged_range(c, from, to, today)
    })
    .await?)
}

pub async fn dismiss_day(
    State(state): State<Shared>,
    Json(body): Json<DismissBody>,
) -> Result<Json<LoggedDay>, ApiError> {
    let day = parse_day(&body.day)?;
    let reason = body.reason.trim().to_string();
    if reason.is_empty() || reason.chars().count() > DISMISS_REASON_MAX_CHARS {
        return Err(bad_request(format!(
            "reason must be 1-{DISMISS_REASON_MAX_CHARS} characters"
        )));
    }
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    write_day(state, day, move |c| logged::dismiss(c, day, &reason, &now)).await
}

pub async fn undismiss_day(
    State(state): State<Shared>,
    Json(body): Json<DayBody>,
) -> Result<Json<LoggedDay>, ApiError> {
    let day = parse_day(&body.day)?;
    write_day(state, day, move |c| logged::undismiss(c, day)).await
}

async fn write_day(
    state: Shared,
    day: NaiveDate,
    write: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<()> + Send + 'static,
) -> Result<Json<LoggedDay>, ApiError> {
    let today = today();
    let mut range = with_conn(state, move |c| {
        write(c)?;
        logged::logged_range(c, day, day, today)
    })
    .await?;
    Ok(Json(range.days.remove(0)))
}
