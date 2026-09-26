//! Daemon routes for the change log (spec 006): the live pop-up poll and
//! the catch-up feed. Child module of `daemon.rs` (via `#[path]`) so it
//! reuses its private `with_conn`/`ApiError` — see design.md §4.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::change_log;
use crate::deild_contract::ChangeFeed;

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct AfterQuery {
    #[serde(default)]
    pub after: i64,
}

/// `GET /changes?after=` — design.md §2: a non-integer `after` fails
/// axum's own `Query` extraction (400) before this handler runs.
pub async fn feed(
    State(state): State<Shared>,
    Query(q): Query<AfterQuery>,
) -> Result<Json<ChangeFeed>, ApiError> {
    let feed = with_conn(state, move |c| change_log::feed(c, q.after)).await?;
    Ok(Json(feed))
}

/// `GET /changes/unseen` — the catch-up (FR-11). Purges changes past
/// retention first so the Owner never catches up on stale rows.
pub async fn unseen(State(state): State<Shared>) -> Result<Json<ChangeFeed>, ApiError> {
    let feed = with_conn(state, |c| {
        change_log::purge_old(c)?;
        change_log::unseen(c)
    })
    .await?;
    Ok(Json(feed))
}

#[derive(Deserialize)]
pub struct SeenBody {
    pub up_to: i64,
}

/// `POST /changes/seen` — opening the catch-up marks it seen (FR-11).
pub async fn mark_seen(
    State(state): State<Shared>,
    Json(body): Json<SeenBody>,
) -> Result<Json<Value>, ApiError> {
    let marked = with_conn(state, move |c| change_log::mark_seen(c, body.up_to)).await?;
    Ok(Json(json!({ "marked": marked })))
}
