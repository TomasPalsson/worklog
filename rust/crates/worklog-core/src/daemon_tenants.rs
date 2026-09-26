//! Daemon routes for multi-tenant infra folders (spec 005): tenant
//! discovery/links and a block's customer split. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/
//! `ApiError` — see design.md §4.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

use crate::billing;
use crate::billing_deildir;
use crate::billing_registry::Registry;
use crate::deild_contract::{BillingSlice, BlockShares, ShareRow};
use crate::tenant_contract::{Tenant, TenantLink};
use crate::tenant_shares;
use crate::tenants;

use super::{with_conn, ApiError, Shared};

/// The five exact `anyhow::bail!` messages design.md §3 maps to 400;
/// anything else (a real db/io failure) stays 500.
fn tenant_bad_request(e: anyhow::Error) -> ApiError {
    const BAD_REQUESTS: [&str; 5] = [
        "Customer no longer exists",
        "Folder is not multi-tenant",
        "Shares must add up to 100%",
        "Share must be above 0",
        "Block is personal",
    ];
    if BAD_REQUESTS.contains(&e.to_string().as_str()) {
        ApiError::bad_request(e)
    } else {
        ApiError::from(e)
    }
}

pub async fn list_tenants(State(state): State<Shared>) -> Result<Json<Vec<Tenant>>, ApiError> {
    let tenants = with_conn(state, tenants::list_tenants).await?;
    Ok(Json(tenants))
}

pub async fn link_tenant(
    State(state): State<Shared>,
    Json(body): Json<TenantLink>,
) -> Result<Json<Value>, ApiError> {
    let folder = body.folder.clone();
    let tenant = body.tenant.clone();
    with_conn(state, move |c| tenants::link_tenant(c, &body))
        .await
        .map_err(tenant_bad_request)?;
    info!(folder, tenant, "linked tenant");
    Ok(Json(json!({ "ok": true })))
}

pub async fn customer_slices(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Vec<BillingSlice>>, ApiError> {
    let slices = with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        if block.is_personal {
            anyhow::bail!("Block is personal");
        }
        // A folder-less block (e.g. pure calendar/Jira, no events) still
        // gets its saved split — `resolve_block_slices` checks that before
        // ever looking at the folder — so it must run for every block, not
        // just ones with a resolvable folder. `BLANK` mirrors the folder
        // `rows_for_day` uses for the same case.
        let folder =
            billing::work_folder_for_block(c, id)?.unwrap_or_else(|| billing::BLANK.to_string());
        let registry = Registry::load(c)?;
        let deildir = billing_deildir::list_deildir(c)?;
        billing::resolve_block_slices(c, &block, &folder, &registry, &deildir)
    })
    .await
    .map_err(tenant_bad_request)?;
    Ok(Json(slices))
}

#[derive(Deserialize)]
pub struct CustomerSharesBody {
    pub rows: Vec<ShareRow>,
}

pub async fn save_customer_shares(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<CustomerSharesBody>,
) -> Result<Json<Value>, ApiError> {
    with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        if block.is_personal {
            anyhow::bail!("Block is personal");
        }
        let registry = Registry::load(c)?;
        // A blank-string deild is no deild — store it as `None`, same as
        // how a blank deild reads back everywhere else.
        let rows = body
            .rows
            .into_iter()
            .map(|mut row| {
                row.deild = row.deild.filter(|d| !d.is_empty());
                row
            })
            .collect();
        let shares = BlockShares {
            day: block.day,
            started_at: block.started_at,
            rows,
        };
        tenant_shares::save_rows(c, &shares, &registry)
    })
    .await
    .map_err(tenant_bad_request)?;
    info!(block_id = id, "saved customer shares");
    Ok(Json(json!({ "ok": true })))
}

pub async fn clear_customer_shares(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        tenant_shares::clear_shares(c, &block.day, &block.started_at)
    })
    .await?;
    info!(block_id = id, "cleared customer shares");
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
#[path = "daemon_tenants_test.rs"]
mod tests;
