//! A block's hand-set owner tables — `block_customer_shares` and
//! `block_resolution_snapshots` — are keyed by its exact `started_at`.
//! `infer::persist_blocks` deletes and re-inserts a whole day's blocks on
//! every re-infer, so a rebuilt block whose start shifted (an earlier
//! backfilled event moves it) or that got re-cut would otherwise leave
//! those rows orphaned — silently dropping the owner's customer/deild
//! choice from billing.
//!
//! Run once per `persist_blocks` call, inside its transaction, after the
//! new blocks are inserted: every new block with no row of its own
//! inherits a COPY of the OLD block's row with the largest time overlap
//! that had a row (never overwriting a row already at the new start);
//! rows left at a `started_at` no new block claims are then deleted as
//! orphans.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::collections::HashSet;

use crate::infer::parse_pair;

struct OwnerTable {
    name: &'static str,
    cols: &'static [&'static str],
}

const OWNER_TABLES: &[OwnerTable] = &[
    OwnerTable {
        name: "block_customer_shares",
        cols: &["shares", "rows_json"],
    },
    OwnerTable {
        name: "block_resolution_snapshots",
        cols: &["description", "parts_json"],
    },
];

/// Carries every owner table's rows from `old_spans` onto `new_spans` for
/// `day` — see module doc. Each span is `(started_at, ended_at)`.
pub(crate) fn carry_owner_tables(
    tx: &Connection,
    day: &str,
    old_spans: &[(String, String)],
    new_spans: &[(String, String)],
) -> Result<()> {
    for table in OWNER_TABLES {
        carry_table(tx, table, day, old_spans, new_spans)?;
    }
    Ok(())
}

fn carry_table(
    tx: &Connection,
    table: &OwnerTable,
    day: &str,
    old_spans: &[(String, String)],
    new_spans: &[(String, String)],
) -> Result<()> {
    let mut stmt = tx.prepare(&format!(
        "SELECT started_at FROM {} WHERE day = ?1",
        table.name
    ))?;
    let existing: HashSet<String> = stmt
        .query_map(params![day], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("listing {}", table.name))?;
    drop(stmt);
    if existing.is_empty() {
        return Ok(());
    }

    let col_list = table.cols.join(", ");
    for (new_start, new_end) in new_spans {
        // Never overwrite a row already at the new block's exact start.
        if existing.contains(new_start) {
            continue;
        }
        let best = old_spans
            .iter()
            .filter(|(old_start, _)| existing.contains(old_start))
            .filter_map(|(old_start, old_end)| {
                overlap_seconds(old_start, old_end, new_start, new_end)
                    .map(|secs| (secs, old_start))
            })
            .filter(|(secs, _)| *secs > 0)
            .max_by_key(|(secs, _)| *secs);
        let Some((_, old_start)) = best else {
            continue;
        };
        tx.execute(
            &format!(
                "INSERT OR IGNORE INTO {t} (day, started_at, {col_list})
                 SELECT day, ?2, {col_list} FROM {t} WHERE day = ?1 AND started_at = ?3",
                t = table.name
            ),
            params![day, new_start, old_start],
        )
        .with_context(|| format!("inheriting {} row", table.name))?;
    }

    let new_starts: HashSet<&String> = new_spans.iter().map(|(s, _)| s).collect();
    for old_start in &existing {
        if !new_starts.contains(old_start) {
            tx.execute(
                &format!(
                    "DELETE FROM {} WHERE day = ?1 AND started_at = ?2",
                    table.name
                ),
                params![day, old_start],
            )
            .with_context(|| format!("deleting orphaned {} row", table.name))?;
        }
    }
    Ok(())
}

/// Overlap in seconds between `[a_start, a_end)` and `[b_start, b_end)`;
/// `None` if either timestamp fails to parse.
fn overlap_seconds(a_start: &str, a_end: &str, b_start: &str, b_end: &str) -> Option<i64> {
    let (a_s, a_e) = parse_pair(a_start, a_end)?;
    let (b_s, b_e) = parse_pair(b_start, b_end)?;
    Some((a_e.min(b_e) - a_s.max(b_s)).num_seconds())
}

#[cfg(test)]
#[path = "infer_carry_shares_test.rs"]
mod tests;
