//! Deild resolution — the FR-03 ladder — for [`super::rows_for_day`]
//! (spec 006, T006).
//!
//! `resolve_block_slices` turns one block into its billing slices: a
//! saved split wins outright; otherwise the same customer slices the
//! export already computes for a block (tenant clues for a multi-tenant
//! folder, else a single fallback slice) each get a deild by the ladder:
//! exactly one keyword match among the slice customer's deildir, else the
//! folder pin's Verkefni when the slice customer is the pin's own
//! customer, else blank.

use anyhow::Result;
use rusqlite::Connection;

use crate::billing::{block_haystack, block_interval, resolve_block};
use crate::billing_deildir::deild_in_text;
use crate::billing_registry::{Registry, Resolved};
use crate::deild_contract::{BillingSlice, Deild, DeildOrigin};
use crate::models::Block;
use crate::tenant_contract::SplitOrigin;
use crate::tenant_shares::{load_rows, slices_from_rows};
use crate::tenant_split::tenant_slices_for_block;

/// A block's billing slices (FR-03, FR-04): a saved split row set applies
/// regardless of the folder's multi-tenant flag — the split editor is
/// available on every block — and wins outright over any guess.
pub fn resolve_block_slices(
    conn: &Connection,
    block: &Block,
    folder: &str,
    registry: &Registry,
    deildir: &[Deild],
) -> Result<Vec<BillingSlice>> {
    let (start, end) = block_interval(block);

    if let Some(shares) = load_rows(conn, &block.day, &block.started_at)? {
        if !shares.rows.is_empty() {
            return Ok(slices_from_rows(start, end, &shares.rows));
        }
    }

    let resolved = resolve_block(conn, block, folder, registry)?;
    let haystack = block_haystack(conn, block)?;

    let to_slice = |customer: Option<String>, intervals: Vec<(i64, i64)>, origin: SplitOrigin| {
        let (deild, deild_origin) =
            resolve_deild(customer.as_deref(), &haystack, deildir, &resolved);
        BillingSlice {
            customer,
            deild,
            intervals,
            origin,
            deild_origin,
        }
    };

    // A multi-tenant folder's block splits into one slice per customer
    // clue; a non-multi-tenant folder (`None`) keeps today's single
    // whole-block slice, billed to the folder's own resolution.
    Ok(
        match tenant_slices_for_block(conn, block, folder, registry)? {
            Some(slices) => slices
                .into_iter()
                .map(|slice| {
                    // A Fallback slice with nothing resolved goes through
                    // today's registry resolution, same as an unsplit block.
                    let customer = slice.customer.or_else(|| resolved.customer.clone());
                    to_slice(customer, slice.intervals, slice.origin)
                })
                .collect(),
            None => vec![to_slice(
                resolved.customer.clone(),
                vec![(start, end)],
                SplitOrigin::Fallback,
            )],
        },
    )
}

/// The FR-03 ladder for one slice, manual rows already handled by the
/// caller: exactly one keyword match among the slice customer's deildir,
/// else the folder pin's Verkefni when the slice customer is the pin's own
/// customer — a slice billed to a different customer must not inherit it,
/// that would put an invented Verkefni on another customer's invoice —
/// else blank.
fn resolve_deild(
    customer: Option<&str>,
    haystack: &str,
    deildir: &[Deild],
    resolved: &Resolved,
) -> (Option<String>, DeildOrigin) {
    let Some(customer) = customer else {
        return (None, DeildOrigin::Blank);
    };
    if let Some(name) = deild_in_text(deildir, customer, haystack) {
        return (Some(name), DeildOrigin::Keyword);
    }
    if Some(customer) == resolved.customer.as_deref() {
        if let Some(verkefni) = &resolved.verkefni {
            return (Some(verkefni.clone()), DeildOrigin::FolderDefault);
        }
    }
    (None, DeildOrigin::Blank)
}
