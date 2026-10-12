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

/// Window the background pull keeps fresh: the current pay cycle, but never
/// fewer than the last 7 days so the previous cycle's tail stays current.
pub(crate) fn pull_range_for(today: NaiveDate, cycle_start_day: u32) -> (NaiveDate, NaiveDate) {
    let cycle = crate::purge::cycle_start_on_or_before(today, cycle_start_day);
    (cycle.min(today - chrono::Duration::days(7)), today)
}

/// Keeps `tempo_remote_worklogs` fresh without the Owner pressing pull: first
/// tick after 60 s, then every 3 h. Read-only against Tempo; the outcome lands
/// in source health. Missing Tempo credentials skip the tick quietly.
pub fn spawn_tempo_pull_loop(state: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut logged_no_auth = false;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        loop {
            match tokio::task::spawn_blocking(TempoAuth::from_secrets).await {
                Ok(Ok(auth)) => {
                    logged_no_auth = false;
                    let (from, to) =
                        pull_range_for(today(), crate::purge::configured_cycle_start_day());
                    let res = pull_range_with(state.clone(), auth, from, to).await;
                    let msg = res.err().map(|e| match e {
                        ApiError::BadRequest(e)
                        | ApiError::NotFound(e)
                        | ApiError::Forbidden(e)
                        | ApiError::Conflict(e)
                        | ApiError::BadGateway(e)
                        | ApiError::Internal(e) => format!("{e:#}"),
                    });
                    if let Some(m) = &msg {
                        tracing::warn!("tempo auto-pull failed: {m}");
                    }
                    let _ = with_conn(state.clone(), move |c| {
                        crate::collect_health::record(
                            c,
                            "tempo",
                            msg.as_deref().map_or(Ok(()), Err),
                        )
                    })
                    .await;
                }
                Ok(Err(e)) => {
                    if !logged_no_auth {
                        tracing::info!("tempo auto-pull skipped: {e:#}");
                        logged_no_auth = true;
                    }
                }
                Err(e) => tracing::warn!("tempo auto-pull credential task failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(3 * 60 * 60)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn pull_range_mid_cycle() {
        assert_eq!(
            pull_range_for(d("2026-10-12"), 20),
            (d("2026-09-20"), d("2026-10-12"))
        );
    }

    #[test]
    fn pull_range_cycle_just_started_uses_seven_day_floor() {
        assert_eq!(
            pull_range_for(d("2026-10-21"), 20),
            (d("2026-10-14"), d("2026-10-21"))
        );
    }

    #[test]
    fn pull_range_on_start_day() {
        assert_eq!(
            pull_range_for(d("2026-10-20"), 20),
            (d("2026-10-13"), d("2026-10-20"))
        );
    }
}
