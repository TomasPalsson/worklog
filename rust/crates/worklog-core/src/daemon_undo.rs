//! Daemon route for undo (spec 018). Outcomes are data (HTTP 200);
//! only internal failures are `ApiError`.

use anyhow::Context;
use axum::extract::State;
use axum::Json;

use crate::daily_helpers_contract::UndoOutcome;
use crate::undo;

use crate::daemon::{ApiError, Shared};

pub async fn post_undo(State(state): State<Shared>) -> Result<Json<UndoOutcome>, ApiError> {
    let outcome = tokio::task::spawn_blocking(move || {
        let mut conn = state.conn.blocking_lock();
        undo::undo_last(&mut conn)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_service as bs;
    use crate::daemon::state_from_conn;
    use crate::daily_helpers_contract::BlockChange;
    use crate::db::open_memory;
    use rusqlite::params;

    fn state_with_block() -> (Shared, i64) {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 'orig', 'AAA-1')",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        (state_from_conn(conn), id)
    }

    fn description(state: &Shared, id: i64) -> Option<String> {
        let conn = state.conn.try_lock().unwrap();
        crate::repo::get_block(&conn, id)
            .unwrap()
            .unwrap()
            .description
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn restores_newest_change() {
        let (state, id) = state_with_block();
        bs::set_description(&state.conn.try_lock().unwrap(), id, "edited").unwrap();
        let Json(out) = post_undo(State(state.clone())).await.ok().unwrap();
        // catches: handler that returns an outcome without running undo
        assert_eq!(
            out,
            UndoOutcome::Restored {
                change: BlockChange::Text,
                block_ids: vec![id]
            }
        );
        assert_eq!(description(&state, id).as_deref(), Some("orig"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_journal_is_data_not_an_error() {
        let (state, _) = state_with_block();
        // catches: mapping NothingToUndo to an ApiError
        let Json(out) = post_undo(State(state)).await.ok().unwrap();
        assert_eq!(out, UndoOutcome::NothingToUndo);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn synced_block_is_refused_as_data_and_unchanged() {
        let (state, id) = state_with_block();
        {
            let conn = state.conn.try_lock().unwrap();
            conn.execute(
                "UPDATE blocks SET tempo_worklog_id = 'TW-1' WHERE id = ?1",
                params![id],
            )
            .unwrap();
            bs::set_description(&conn, id, "after sync").unwrap();
        }
        // catches: mapping RefusedSynced to an ApiError
        let Json(out) = post_undo(State(state.clone())).await.ok().unwrap();
        assert_eq!(out, UndoOutcome::RefusedSynced { block_id: id });
        assert_eq!(description(&state, id).as_deref(), Some("after sync"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_entry_is_an_internal_error() {
        let (state, id) = state_with_block();
        {
            let conn = state.conn.try_lock().unwrap();
            bs::set_description(&conn, id, "edited").unwrap();
            conn.execute(
                "UPDATE blocks SET description = 'estimator' WHERE id = ?1",
                params![id],
            )
            .unwrap();
        }
        // catches: swallowing the error into an Ok outcome
        let err = post_undo(State(state.clone())).await.err().unwrap();
        assert!(matches!(err, ApiError::Internal(_)));
        assert_eq!(description(&state, id).as_deref(), Some("estimator"));
    }
}
