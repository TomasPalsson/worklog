//! Daemon route handler for the footer nudges (spec 018). Routes are
//! registered in `daemon.rs`. The handler reads only the local DB; the review
//! cache is filled by `daemon::spawn_nudge_refresh_loop`.

use anyhow::Context;
use axum::extract::State;
use axum::Json;
use chrono::Utc;

use crate::daemon::{ApiError, Shared};
use crate::daily_helpers_contract::Nudge;
use crate::nudges;

pub async fn get_nudges(State(state): State<Shared>) -> Result<Json<Vec<Nudge>>, ApiError> {
    let list = tokio::task::spawn_blocking(move || {
        nudges::current(&state.conn.blocking_lock(), Utc::now())
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(list))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::state_from_conn;
    use crate::db::open_memory;

    #[tokio::test]
    async fn stale_cache_still_answers_from_local_state_without_fetching() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at)
             VALUES ('GENAI-9', 's', 'In Progress', 'indeterminate', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES ('2026-01-01', 'GENAI-9', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0)",
            [],
        )
        .unwrap();
        let Ok(Json(list)) = get_nudges(State(state_from_conn(conn))).await else {
            panic!("get_nudges failed on a stale cache");
        };
        // catches: a request-path refetch (needs secrets) or an error on a missing cache
        assert_eq!(list.len(), 1);
        assert!(list[0].text.contains("GENAI-9"));
    }
}
