//! Daemon routes for multi-tenant infra folders (spec 005): tenant
//! discovery/links and a block's customer split. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/
//! `ApiError` — see design.md §4.

use std::collections::BTreeMap;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

use crate::billing;
use crate::billing_registry::Registry;
use crate::tenant_contract::{CustomerShares, CustomerSlice, Tenant, TenantLink};
use crate::tenant_shares;
use crate::tenant_split::tenant_slices_for_block;
use crate::tenants;

use super::{with_conn, ApiError, Shared};

/// The four exact `anyhow::bail!` messages design.md §3 maps to 400;
/// anything else (a real db/io failure) stays 500.
fn tenant_bad_request(e: anyhow::Error) -> ApiError {
    const BAD_REQUESTS: [&str; 4] = [
        "Customer no longer exists",
        "Folder is not multi-tenant",
        "Shares must add up to 100%",
        "Share must be above 0",
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
) -> Result<Json<Vec<CustomerSlice>>, ApiError> {
    let slices = with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        let Some(folder) = billing::work_folder_for_block(c, id)? else {
            return Ok(Vec::new());
        };
        let registry = Registry::load(c)?;
        Ok(tenant_slices_for_block(c, &block, &folder, &registry)?.unwrap_or_default())
    })
    .await?;
    Ok(Json(slices))
}

#[derive(Deserialize)]
pub struct CustomerSharesBody {
    pub shares: BTreeMap<String, f64>,
}

pub async fn save_customer_shares(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<CustomerSharesBody>,
) -> Result<Json<Value>, ApiError> {
    with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        let registry = Registry::load(c)?;
        let folder = billing::work_folder_for_block(c, id)?;
        let multi_tenant = folder
            .as_deref()
            .map(|f| {
                registry
                    .folders
                    .iter()
                    .any(|m| m.folder == f && m.multi_tenant)
            })
            .unwrap_or(false);
        if !multi_tenant {
            anyhow::bail!("Folder is not multi-tenant");
        }
        let shares = CustomerShares {
            day: block.day,
            started_at: block.started_at,
            shares: body.shares,
        };
        tenant_shares::save_shares(c, &shares, &registry)
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
