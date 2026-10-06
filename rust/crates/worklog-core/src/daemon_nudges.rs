//! Daemon route handler for the footer nudges (spec 018). Routes are
//! registered in `daemon.rs`; the network fetch runs with the sqlite lock released.

use anyhow::Context;
use axum::extract::State;
use axum::Json;
use chrono::Utc;

use crate::collectors::github::GitHubAuth;
use crate::daemon::{ApiError, Shared};
use crate::daily_helpers_contract::Nudge;
use crate::{http, nudges};

pub async fn get_nudges(State(state): State<Shared>) -> Result<Json<Vec<Nudge>>, ApiError> {
    let list = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Nudge>> {
        let now = Utc::now();
        if nudges::reviews_stale(&state.conn.blocking_lock(), now)? {
            // no GitHub credentials = no review nudges, the rest still show
            if let (Ok(auth), Ok(client)) = (GitHubAuth::from_secrets(), http::client()) {
                let fetched = crate::collectors::github::review_requests(&client, &auth);
                let conn = state.conn.blocking_lock();
                match fetched {
                    Ok(body) => {
                        nudges::store_reviews(&conn, now, &nudges::parse_review_search(&body))?
                    }
                    Err(e) => eprintln!("worklog: review nudges not refreshed: {e:#}"),
                }
            }
        }
        nudges::current(&state.conn.blocking_lock(), now)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(list))
}
