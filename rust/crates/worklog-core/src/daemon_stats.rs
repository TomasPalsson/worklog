//! GET /stats?from&to — the stats page's one payload. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/`ApiError`.

use anyhow::anyhow;
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, NaiveDate, Utc};
use serde::Deserialize;

const STATS_MAX_RANGE_DAYS: i64 = 366 * 30;

use crate::stats_contract::StatsReport;
use crate::{stats, tz};

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    to: Option<String>,
}

fn parse_day(raw: &str) -> Result<NaiveDate, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::bad_request(anyhow!("`{raw}` is not a date (YYYY-MM-DD)")))
}

pub async fn get_stats(
    State(state): State<Shared>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<StatsReport>, ApiError> {
    let from = q.from.as_deref().map(parse_day).transpose()?;
    let today = tz::local_date(Utc::now());
    let to = q.to.as_deref().map(parse_day).transpose()?.unwrap_or(today);
    if from.is_some_and(|f| f > to) {
        return Err(ApiError::bad_request(anyhow!("`from` is after `to`")));
    }
    // Daily rows are allocated up front while the shared connection is
    // locked, so bound the range (and the years, for tz window arithmetic).
    let in_years = |d: NaiveDate| (1970..=9999).contains(&d.year());
    if !in_years(to) || from.is_some_and(|f| !in_years(f)) {
        return Err(ApiError::bad_request(anyhow!(
            "dates must fall in 1970..=9999"
        )));
    }
    if from.is_some_and(|f| (to - f).num_days() + 1 > STATS_MAX_RANGE_DAYS) {
        return Err(ApiError::bad_request(anyhow!(
            "range is longer than {STATS_MAX_RANGE_DAYS} days"
        )));
    }
    Ok(Json(
        with_conn(state, move |c| {
            // Default start = earliest event day, clamped so an explicit
            // past `to` never yields an inverted range.
            let from = match from {
                Some(f) => f,
                None => stats::first_day(c)?.unwrap_or(today).min(to),
            };
            stats::stats_report(c, from, to, today)
        })
        .await?,
    ))
}
