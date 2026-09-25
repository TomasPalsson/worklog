//! The per-block customer split rules for multi-tenant infra folders (spec 005).

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;

use crate::billing_registry::Registry;
use crate::models::Block;
use crate::tenant_clues::clues_for_block;
use crate::tenant_contract::{Clue, CustomerSlice, SplitOrigin, HOUSE_CUSTOMER};
use crate::tenant_shares::{load_shares, slices_from_shares};
use crate::tenants::tenant_customer_map;

/// Split `[start, end)` between customers by the rules in order:
/// drop House (`APRÓ`) clues when any other customer has a clue (FR-07);
/// within each minute keep only the strongest remaining clue (FR-08); give
/// every second to its nearest clue, an exact tie going to the earlier one
/// (FR-06). `None` when `clues` has no timestamped clue left to split on.
pub fn split_block(start: i64, end: i64, clues: &[Clue]) -> Option<Vec<CustomerSlice>> {
    let has_other = clues.iter().any(|c| c.customer != HOUSE_CUSTOMER);
    let survivors: Vec<&Clue> = clues
        .iter()
        .filter(|c| !has_other || c.customer != HOUSE_CUSTOMER)
        .collect();
    if survivors.is_empty() {
        return None;
    }

    let mut by_minute: BTreeMap<i64, Vec<&Clue>> = BTreeMap::new();
    for clue in survivors {
        by_minute
            .entry(clue.at.timestamp().div_euclid(60))
            .or_default()
            .push(clue);
    }
    let mut winners: Vec<&Clue> = Vec::new();
    for group in by_minute.into_values() {
        let strongest = group
            .iter()
            .map(|c| c.strength)
            .max()
            .expect("group is never empty");
        winners.extend(group.into_iter().filter(|c| c.strength == strongest));
    }
    winners.sort_by_key(|c| c.at);

    // Each winner owns the seconds nearer to it than to its neighbours; the
    // boundary between two consecutive winners is their midpoint second,
    // rounded so a tie belongs to the earlier one.
    let mut by_customer: BTreeMap<String, Vec<(i64, i64)>> = BTreeMap::new();
    let mut from = start;
    for (i, clue) in winners.iter().enumerate() {
        let to = match winners.get(i + 1) {
            Some(next) => (clue.at.timestamp() + next.at.timestamp()) / 2 + 1,
            None => end,
        };
        by_customer
            .entry(clue.customer.clone())
            .or_default()
            .push((from, to));
        from = to;
    }

    Some(
        by_customer
            .into_iter()
            .filter_map(|(customer, intervals)| {
                let intervals = merge_adjacent(intervals);
                (!intervals.is_empty()).then_some(CustomerSlice {
                    customer: Some(customer),
                    intervals,
                    origin: SplitOrigin::Clues,
                })
            })
            .collect(),
    )
}

/// Collapse `[a, b), [b, c), ...` into `[a, c)`; drops zero-length ranges
/// (two clues landing on the very same second).
fn merge_adjacent(intervals: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    let mut merged: Vec<(i64, i64)> = Vec::with_capacity(intervals.len());
    for (from, to) in intervals {
        if from == to {
            continue;
        }
        match merged.last_mut() {
            Some(last) if last.1 == from => last.1 = to,
            _ => merged.push((from, to)),
        }
    }
    merged
}

/// `block`'s wall-clock interval as epoch seconds. Mirrors (privately)
/// `billing::block_interval` — duration, not `ended_at`, is the canonical
/// logged time.
fn block_interval(block: &Block) -> (i64, i64) {
    let start = chrono::DateTime::parse_from_rfc3339(&block.started_at)
        .map(|d| d.timestamp())
        .unwrap_or(0);
    (start, start + block.duration_seconds.max(0))
}

/// A non-House customer named in `text` — the summary clue of FR-09, where a
/// customer named alongside `APRÓ` beats it. `None` when nothing non-House
/// is unambiguously named; the caller then falls back to the folder's
/// normal (pin/alias) resolution.
fn summary_customer(text: &str, registry: &Registry) -> Option<String> {
    let without_house = Registry {
        customers: registry
            .customers
            .iter()
            .filter(|c| c.name != HOUSE_CUSTOMER)
            .cloned()
            .collect(),
        folders: Vec::new(),
    };
    without_house.customer_in_text(text)
}

/// `block`'s customer slices: the owner's hand-set shares first, else the
/// clue split, else (no timestamped clue) a single `Fallback` slice —
/// `customer` from the block's own summary when it unambiguously names one,
/// else `None` for the folder's normal resolution to fill in. `None`
/// overall when `folder` isn't multi-tenant.
pub fn tenant_slices_for_block(
    conn: &Connection,
    block: &Block,
    folder: &str,
    registry: &Registry,
) -> Result<Option<Vec<CustomerSlice>>> {
    let multi_tenant = registry
        .folders
        .iter()
        .any(|f| f.folder == folder && f.multi_tenant);
    if !multi_tenant {
        return Ok(None);
    }

    let (start, end) = block_interval(block);

    if let Some(shares) = load_shares(conn, &block.day, &block.started_at)? {
        return Ok(Some(slices_from_shares(start, end, &shares)));
    }

    let tenants = tenant_customer_map(conn)?;
    let clues = clues_for_block(conn, block.id, folder, &tenants, registry)?;
    if let Some(slices) = split_block(start, end, &clues) {
        return Ok(Some(slices));
    }

    let customer = block
        .description
        .as_deref()
        .and_then(|text| summary_customer(text, registry));
    Ok(Some(vec![CustomerSlice {
        customer,
        intervals: vec![(start, end)],
        origin: SplitOrigin::Fallback,
    }]))
}

#[cfg(test)]
#[path = "tenant_split_test.rs"]
mod tests;
