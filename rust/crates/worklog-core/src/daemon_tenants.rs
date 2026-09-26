//! Daemon routes for multi-tenant infra folders (spec 005): tenant
//! discovery/links and a block's customer split. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/
//! `ApiError` — see design.md §4.

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::billing;
use crate::billing_deildir;
use crate::billing_registry::Registry;
use crate::change_log;
use crate::deild_contract::{BillingSlice, BlockShares, ChangeSource, ShareRow};
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
        tenant_shares::save_rows(c, &shares, &registry)?;
        // One batch per save (D-07); a refresh failure must not fail it.
        let batch = change_log::new_batch(ChangeSource::User);
        if let Err(e) = change_log::refresh_day(c, &shares.day, ChangeSource::User, &batch) {
            warn!(error = %e, block_id = id, "change log refresh failed");
        }
        Ok(())
    })
    .await
    .map_err(tenant_bad_request)?;
    info!(block_id = id, "saved customer shares");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct MoveLineDeildBody {
    pub day: String,
    pub block_ids: Vec<i64>,
    pub customer: String,
    pub from_deild: Option<String>,
    pub to_deild: Option<String>,
}

/// FR-13: move a whole super block's deild from its line header. For each
/// block on the line, every slice naming `customer`+`from_deild` is
/// repointed to `to_deild`; every other slice on that block keeps its own
/// (customer, deild) — a shared block split across customers only moves
/// the part that named this line's customer. The result is saved as a
/// manual row set, same as the split editor.
pub async fn move_line_deild(
    State(state): State<Shared>,
    Json(body): Json<MoveLineDeildBody>,
) -> Result<Json<Value>, ApiError> {
    let day = body.day.clone();
    let customer = body.customer.clone();
    with_conn(state, move |c| {
        let registry = Registry::load(c)?;
        let deildir = billing_deildir::list_deildir(c)?;
        for &block_id in &body.block_ids {
            let block = crate::repo::get_block(c, block_id)?
                .ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))?;
            let folder = billing::work_folder_for_block(c, block_id)?
                .unwrap_or_else(|| billing::BLANK.to_string());
            let slices =
                billing::resolve_block_slices(c, &block, &folder, &registry, &deildir)?;
            let (start, end) = billing::block_interval(&block);
            let total = (end - start).max(1) as f64;

            let mut rows: Vec<ShareRow> = Vec::new();
            for slice in &slices {
                let Some(customer) = slice.customer.clone() else {
                    anyhow::bail!("block {block_id} has a slice with no customer, fill it in before moving this line");
                };
                let deild = if customer == body.customer && slice.deild == body.from_deild {
                    body.to_deild.clone()
                } else {
                    slice.deild.clone()
                };
                let seconds: i64 = slice.intervals.iter().map(|(s, e)| e - s).sum();
                let fraction = seconds as f64 / total;
                match rows
                    .iter_mut()
                    .find(|r| r.customer == customer && r.deild == deild)
                {
                    Some(existing) => existing.fraction += fraction,
                    None => rows.push(ShareRow {
                        customer,
                        deild,
                        fraction,
                    }),
                }
            }
            // The last row absorbs whatever second the earlier ones'
            // rounding leaves over, so `validate_rows` always accepts it.
            let n = rows.len();
            if n > 0 {
                let sum_rest: f64 = rows[..n - 1].iter().map(|r| r.fraction).sum();
                rows[n - 1].fraction = (1.0 - sum_rest).max(0.0);
            }

            let shares = BlockShares {
                day: block.day.clone(),
                started_at: block.started_at.clone(),
                rows,
            };
            tenant_shares::save_rows(c, &shares, &registry)?;
        }
        // One batch for the whole move (D-07); a refresh failure must not
        // fail the write — the rows above already committed.
        let batch = change_log::new_batch(ChangeSource::User);
        if let Err(e) = change_log::refresh_day(c, &body.day, ChangeSource::User, &batch) {
            warn!(error = %e, day = %body.day, "change log refresh failed");
        }
        Ok(())
    })
    .await
    .map_err(|e| {
        if e.to_string().contains("has a slice with no customer") {
            ApiError::bad_request(e)
        } else {
            tenant_bad_request(e)
        }
    })?;
    info!(day, customer, "moved line deild");
    Ok(Json(json!({ "ok": true })))
}

pub async fn clear_customer_shares(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    with_conn(state, move |c| {
        let block = crate::repo::get_block(c, id)?
            .ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        tenant_shares::clear_shares(c, &block.day, &block.started_at)?;
        // One batch per edit (D-07); a refresh failure must not fail it.
        let batch = change_log::new_batch(ChangeSource::User);
        if let Err(e) = change_log::refresh_day(c, &block.day, ChangeSource::User, &batch) {
            warn!(error = %e, block_id = id, "change log refresh failed");
        }
        Ok(())
    })
    .await?;
    info!(block_id = id, "cleared customer shares");
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
#[path = "daemon_tenants_test.rs"]
mod tests;
