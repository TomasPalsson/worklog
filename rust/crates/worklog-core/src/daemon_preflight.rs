//! Daemon route handlers for the pre-send checklist and read-back (spec 018,
//! FR-11..14). Read-only. Routes are registered in `daemon.rs`.

use anyhow::Context;
use axum::extract::{Query, State};
use axum::Json;
use chrono::NaiveDate;
use serde::Deserialize;

use crate::collectors::tempo::TempoAuth;
use crate::daemon::daemon_logged::pull_range_with;
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
    let auth = tokio::task::spawn_blocking(TempoAuth::from_secrets)
        .await
        .context("spawn_blocking")??;
    read_back_with(state, auth, q.day).await
}

async fn read_back_with(
    state: Shared,
    auth: TempoAuth,
    day: NaiveDate,
) -> Result<Json<PreflightRow>, ApiError> {
    pull_range_with(state.clone(), auth, day, day).await?;
    let row = tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        preflight::read_back(&conn, day)
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
    use httpmock::prelude::*;

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

    const ACCOUNT: &str = "557058:abc";

    fn auth(server: &MockServer) -> TempoAuth {
        TempoAuth {
            token: "tempo-token".to_string(),
            author: ACCOUNT.to_string(),
            base_url: server.base_url(),
        }
    }

    fn stored_900s() -> Shared {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO tempo_remote_worklogs (tempo_worklog_id, day, issue_id, seconds, owner, pulled_at)
             VALUES ('w1', '2026-10-01', 1, 900, 'worklog', 'x')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description, tempo_worklog_id)
             VALUES ('2026-10-01', 'AB-1', '2026-10-01T09:00:00+00:00', '2026-10-01T09:00:00+00:00', 600, 'x', '11')",
            [],
        )
        .unwrap();
        state_from_conn(conn)
    }

    fn mock_tempo(server: &MockServer, seconds: i64) {
        server.mock(|when, then| {
            when.method(GET).path(format!("/worklogs/user/{ACCOUNT}"));
            then.status(200).json_body(serde_json::json!({"results": [
                {"tempoWorklogId": 11, "startDate": "2026-10-01", "issue": {"id": 100},
                 "timeSpentSeconds": seconds, "description": "a"}
            ]}));
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("/user-schedule/{ACCOUNT}"));
            then.status(200)
                .json_body(serde_json::json!({"results": []}));
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_back_compares_against_the_fresh_pull_not_stored_rows() {
        let server = MockServer::start();
        mock_tempo(&server, 600);
        let Json(row) = read_back_with(stored_900s(), auth(&server), date("2026-10-01"))
            .await
            .unwrap_or_else(|_| panic!("mismatch is data"));
        // catches: comparing stale stored rows (900s) without pulling
        assert_eq!(row.check, PreflightCheck::ReadBack);
        assert!(!row.ok);
        assert!(row.detail.contains("Tempo has 600s"), "{}", row.detail);
        assert_eq!(row.target.as_deref(), Some("2026-10-01"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_back_is_green_when_the_fresh_pull_matches_despite_stale_rows() {
        let server = MockServer::start();
        mock_tempo(&server, 0);
        let Json(row) = read_back_with(stored_900s(), auth(&server), date("2026-10-01"))
            .await
            .unwrap_or_else(|_| panic!("pull should succeed"));
        // catches: pull made but result ignored, or compare run before the store
        assert!(row.ok, "{}", row.detail);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_pull_is_an_error_never_a_stale_green() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path(format!("/worklogs/user/{ACCOUNT}"));
            then.status(500).body("boom");
        });
        // stored rows already match (nothing sent, nothing stored)
        let state = state_from_conn(open_memory().unwrap());
        let got = read_back_with(state, auth(&server), date("2026-10-01")).await;
        // catches: swallowing the pull error and comparing stored rows
        assert!(matches!(got, Err(ApiError::BadGateway(_))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_back_pulls_only_the_asked_day() {
        let server = MockServer::start();
        let worklogs = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/worklogs/user/{ACCOUNT}"))
                .query_param("from", "2026-10-01")
                .query_param("to", "2026-10-01");
            then.status(200)
                .json_body(serde_json::json!({"results": []}));
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("/user-schedule/{ACCOUNT}"));
            then.status(200)
                .json_body(serde_json::json!({"results": []}));
        });
        let _ = read_back_with(stored_900s(), auth(&server), date("2026-10-01")).await;
        // catches: pulling a wider or shifted range
        worklogs.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_back_reports_the_mismatch_for_the_asked_day() {
        let server = MockServer::start();
        mock_tempo(&server, 900);
        let Json(row) = read_back_with(stored_900s(), auth(&server), date("2026-10-01"))
            .await
            .unwrap_or_else(|_| panic!("mismatch is data"));
        // catches: always-green read-back, or a row for the wrong check kind
        assert_eq!(row.check, PreflightCheck::ReadBack);
        assert!(!row.ok);
        assert_eq!(row.target.as_deref(), Some("2026-10-01"));
    }
}
