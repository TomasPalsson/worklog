//! Daemon routes for the deildir registry (spec 006): CRUD for the
//! per-customer deild list. Child module of `daemon.rs` (via `#[path]`)
//! so it reuses its private `with_conn`/`ApiError` — see design.md §4.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::{json, Value};
use tracing::info;

use crate::billing_deildir;
use crate::deild_contract::{ChangeSource, Deild};

use super::{refresh_recent_days, with_conn, ApiError, Shared};

/// `billing_deildir::upsert_deild`'s two validation bails (empty name,
/// duplicate (customer, name)) map to 400, design.md §2; anything else
/// (a real db failure) stays 500.
fn deild_bad_request(e: anyhow::Error) -> ApiError {
    let msg = e.to_string();
    if msg.contains("name must not be empty") || msg.contains("already exists for customer") {
        ApiError::bad_request(e)
    } else {
        ApiError::from(e)
    }
}

pub async fn upsert_deild(
    State(state): State<Shared>,
    Json(body): Json<Deild>,
) -> Result<Json<Value>, ApiError> {
    let customer = body.customer.clone();
    let name = body.name.clone();
    let id = with_conn(state, move |c| {
        let id = billing_deildir::upsert_deild(c, &body)?;
        refresh_recent_days(c, ChangeSource::Keyword);
        Ok(id)
    })
    .await
    .map_err(deild_bad_request)?;
    info!(customer = %customer, name = %name, id, "upserted deild");
    Ok(Json(json!({ "id": id })))
}

pub async fn delete_deild(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    let removed = with_conn(state, move |c| {
        let removed = billing_deildir::delete_deild(c, id)?;
        refresh_recent_days(c, ChangeSource::Keyword);
        Ok(removed)
    })
    .await?;
    info!(id, removed, "deleted deild");
    Ok(Json(json!({ "removed": removed })))
}
