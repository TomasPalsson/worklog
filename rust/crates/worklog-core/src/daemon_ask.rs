//! Daemon route handlers for ask (spec 018, FR-30..33). Read-only apart from
//! filling the search index. Routes are registered in `daemon.rs`.

use anyhow::Context;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::ask::{self, Hit, Stopped};
use crate::daemon::{ApiError, Shared};

#[derive(Deserialize)]
pub struct AskQuery {
    pub q: String,
}

#[derive(Deserialize)]
pub struct StoppedQuery {
    pub repo: String,
}

pub async fn get_ask(
    State(state): State<Shared>,
    Query(q): Query<AskQuery>,
) -> Result<Json<Vec<Hit>>, ApiError> {
    let hits = tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        ask::sync(&conn)?;
        ask::search(&conn, &q.q)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(hits))
}

pub async fn get_stopped(
    State(state): State<Shared>,
    Query(q): Query<StoppedQuery>,
) -> Result<Json<Stopped>, ApiError> {
    let stopped = tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        ask::where_stopped(&conn, &q.repo)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(stopped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::state_from_conn;
    use crate::db::open_memory;

    fn block(conn: &rusqlite::Connection, start: &str, desc: &str) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue)
             VALUES ('2026-04-18', ?1, ?1, 1800, ?2, 'AAA-1')",
            rusqlite::params![start, desc],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    async fn ask_for(state: &Shared, q: &str) -> Vec<Hit> {
        let query = Query(AskQuery { q: q.to_string() });
        get_ask(State(state.clone()), query).await.ok().unwrap().0
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn finds_blocks_never_indexed_newest_first() {
        let conn = open_memory().unwrap();
        let old = block(&conn, "2026-04-18T09:00:00+00:00", "migrate kafka topics");
        let new = block(&conn, "2026-04-18T11:00:00+00:00", "kafka consumer lag");
        block(&conn, "2026-04-18T13:00:00+00:00", "unrelated");
        let state = state_from_conn(conn);
        // catches: handler that skips ask::sync (nothing indexed yet) or loses ordering
        let hits = ask_for(&state, "kafka").await;
        let ids: Vec<i64> = hits.iter().map(|h| h.block_id).collect();
        assert_eq!(ids, vec![new, old]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn caps_at_five_hits() {
        let conn = open_memory().unwrap();
        for i in 0..6 {
            block(&conn, &format!("2026-04-18T0{i}:00:00+00:00"), "kafka");
        }
        let state = state_from_conn(conn);
        // catches: dropping the limit (6 returned)
        assert_eq!(ask_for(&state, "kafka").await.len(), 5);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn blank_and_operator_queries_are_data_not_errors() {
        let conn = open_memory().unwrap();
        block(&conn, "2026-04-18T09:00:00+00:00", "kafka");
        let state = state_from_conn(conn);
        // catches: blank query reaching FTS5 as a syntax error
        assert!(ask_for(&state, "   ").await.is_empty());
        // catches: unescaped quote/operator breaking the MATCH
        assert!(ask_for(&state, "\"kafka OR (").await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ask_leaves_blocks_untouched() {
        let conn = open_memory().unwrap();
        let id = block(&conn, "2026-04-18T09:00:00+00:00", "kafka");
        let state = state_from_conn(conn);
        ask_for(&state, "kafka").await;
        let conn = state.conn.try_lock().unwrap();
        let b = crate::repo::get_block(&conn, id).unwrap().unwrap();
        // catches: a search that rewrites block rows
        assert_eq!(b.description.as_deref(), Some("kafka"));
        assert_eq!(b.duration_seconds, 1800);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stopped_for_unknown_repo_is_empty() {
        let state = state_from_conn(open_memory().unwrap());
        let q = Query(StoppedQuery {
            repo: "nope".into(),
        });
        // catches: unknown repo mapped to an error
        let Json(out) = get_stopped(State(state), q).await.ok().unwrap();
        assert_eq!(out, Stopped::default());
    }
}
