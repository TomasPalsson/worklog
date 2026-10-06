//! Daemon route handlers for the pre-send checklist and read-back (spec 018,
//! FR-11..14). Read-only. Routes are registered in `daemon.rs`.

use anyhow::Context;
use axum::extract::{Query, State};
use axum::Json;
use chrono::NaiveDate;
use serde::Deserialize;

use crate::daemon::{ApiError, Shared};
use crate::daily_helpers_contract::PreflightRow;
use crate::preflight;

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Deserialize)]
pub struct DayQuery {
    pub day: NaiveDate,
}

pub async fn get_preflight(
    State(state): State<Shared>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<PreflightRow>>, ApiError> {
    if q.from > q.to {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "from {} is after to {}",
            q.from,
            q.to
        )));
    }
    let rows = tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        preflight::check(&conn, q.from, q.to)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(rows))
}

pub async fn get_read_back(
    State(state): State<Shared>,
    Query(q): Query<DayQuery>,
) -> Result<Json<PreflightRow>, ApiError> {
    let row = tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        preflight::read_back(&conn, q.day)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::state_from_conn;
    use crate::daily_helpers_contract::PreflightCheck;
    use crate::db::open_memory;

    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn state_with_unticketed_block() -> Shared {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
             VALUES ('2026-10-01', NULL, '2026-10-01T09:00:00+00:00', '2026-10-01T09:00:00+00:00', 600, 'x')",
            [],
        )
        .unwrap();
        state_from_conn(conn)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn preflight_returns_red_rows_as_data_naming_the_fault() {
        let range = RangeQuery {
            from: date("2026-10-01"),
            to: date("2026-10-01"),
        };
        let Json(rows) = get_preflight(State(state_with_unticketed_block()), Query(range))
            .await
            .unwrap_or_else(|_| panic!("red rows are data, not an error"));
        // catches: handler returns only green rows / swallows the check result
        let red = rows
            .iter()
            .find(|r| r.check == PreflightCheck::Ticketed)
            .unwrap();
        assert!(!red.ok);
        assert!(red.target.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn preflight_range_is_inclusive_of_the_last_day() {
        // catches: `from..to` exclusive range skipping the end day
        let range = RangeQuery {
            from: date("2026-09-30"),
            to: date("2026-10-01"),
        };
        let Json(rows) = get_preflight(State(state_with_unticketed_block()), Query(range))
            .await
            .unwrap_or_else(|_| panic!("range should succeed"));
        assert!(rows.iter().any(|r| !r.ok));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reversed_range_is_a_bad_request_not_a_false_all_green() {
        let range = RangeQuery {
            from: date("2026-10-02"),
            to: date("2026-10-01"),
        };
        // catches: empty loop returns four green rows for a range that checked nothing
        let got = get_preflight(State(state_with_unticketed_block()), Query(range)).await;
        assert!(matches!(got, Err(ApiError::BadRequest(_))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_day_range_is_accepted() {
        // catches: `from >= to` rejecting a one-day check
        let range = RangeQuery {
            from: date("2026-10-01"),
            to: date("2026-10-01"),
        };
        assert!(
            get_preflight(State(state_with_unticketed_block()), Query(range))
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_back_reports_the_mismatch_for_the_asked_day() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO tempo_remote_worklogs (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
             VALUES ('w1', '2026-10-01', 1, 900, 'worklog', 'x')",
            [],
        )
        .unwrap();
        let q = DayQuery {
            day: date("2026-10-01"),
        };
        let Json(row) = get_read_back(State(state_from_conn(conn)), Query(q))
            .await
            .unwrap_or_else(|_| panic!("mismatch is data"));
        // catches: always-green read-back, or a row for the wrong check kind
        assert_eq!(row.check, PreflightCheck::ReadBack);
        assert!(!row.ok);
        assert_eq!(row.target.as_deref(), Some("2026-10-01"));
    }
}
