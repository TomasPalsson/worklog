//! The change log — detects and records automatic changes to a block's
//! customer, deild, split or description by diffing against the block's
//! last-seen `deild_contract::ResolutionSnapshot`, and serves the live
//! pop-up + catch-up feed (spec 006). Types live in `deild_contract`; see
//! `deild_contract::BlockChange`. Populated by T010: `new_batch`,
//! `refresh_day`, `feed`, `unseen`, `mark_seen`, `purge_old`.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::billing::{resolve_block_slices, work_folder_for_block, BLANK};
use crate::billing_deildir::list_deildir;
use crate::billing_registry::Registry;
use crate::deild_contract::{
    BillingSlice, BlockChange, ChangeBatch, ChangeFeed, ChangeField, ChangeSource,
    ResolutionSnapshot, ShareRow, CHANGE_RETENTION_DAYS,
};
use crate::models::Block;
use crate::repo;

/// One run of one writer — the pop-up unit (D-07).
pub fn new_batch(source: ChangeSource) -> String {
    format!("{}-{}", to_col(source), Utc::now().timestamp_millis())
}

/// Re-resolves every non-personal block of `day` and logs what changed
/// since the last refresh, tagged with `source` and `batch`. A block with
/// no stored snapshot is snapshotted silently (first sighting); a stored
/// snapshot with no matching block is dropped silently — new/deleted
/// blocks and duration alone are never notified (FR-12). Returns the
/// number of change rows logged.
pub fn refresh_day(
    conn: &Connection,
    day: &str,
    source: ChangeSource,
    batch: &str,
) -> Result<usize> {
    let registry = Registry::load(conn)?;
    let deildir = list_deildir(conn)?;
    let blocks = repo::list_blocks_for_day(conn, day)?;

    let mut current: Vec<String> = Vec::new();
    let mut logged = 0usize;
    for block in blocks.iter().filter(|b| !b.is_personal) {
        current.push(block.started_at.clone());
        let folder = work_folder_for_block(conn, block.id)?.unwrap_or_else(|| BLANK.to_string());
        let slices = resolve_block_slices(conn, block, &folder, &registry, &deildir)?;
        let snapshot = build_snapshot(block, &slices);
        match load_snapshot(conn, day, &block.started_at)? {
            None => store_snapshot(conn, &snapshot)?,
            Some(old) => {
                logged += diff_and_log(conn, &old, &snapshot, source, batch)?;
                store_snapshot(conn, &snapshot)?;
            }
        }
    }
    for gone in stored_started_at(conn, day)? {
        if !current.contains(&gone) {
            delete_snapshot(conn, day, &gone)?;
        }
    }
    Ok(logged)
}

/// Changes with `id > after`, all sources, plus one summary per batch
/// (D-07) ordered by that batch's first change id.
pub fn feed(conn: &Connection, after: i64) -> Result<ChangeFeed> {
    query_feed(
        conn,
        "SELECT id, day, started_at, field, old_value, new_value, source, batch, created_at, seen_at
           FROM block_changes WHERE id > ?1 ORDER BY id",
        params![after],
    )
}

/// The catch-up: every change not yet marked seen (FR-11), all sources —
/// the Owner's own edits (FR-15) show up here, never in a live pop-up.
pub fn unseen(conn: &Connection) -> Result<ChangeFeed> {
    query_feed(
        conn,
        "SELECT id, day, started_at, field, old_value, new_value, source, batch, created_at, seen_at
           FROM block_changes WHERE seen_at IS NULL ORDER BY id",
        [],
    )
}

/// Marks every unseen change with `id <= up_to` seen (FR-11: opening the
/// catch-up marks it seen). Returns the number of rows marked.
pub fn mark_seen(conn: &Connection, up_to: i64) -> Result<usize> {
    conn.execute(
        "UPDATE block_changes SET seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE seen_at IS NULL AND id <= ?1",
        params![up_to],
    )
    .context("marking block_changes seen")
}

/// Drops changes older than [`CHANGE_RETENTION_DAYS`] (§5 retention).
/// Returns the number of rows purged.
pub fn purge_old(conn: &Connection) -> Result<usize> {
    let cutoff = (Utc::now() - Duration::days(CHANGE_RETENTION_DAYS))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    conn.execute(
        "DELETE FROM block_changes WHERE created_at < ?1",
        params![cutoff],
    )
    .context("purging old block_changes")
}

/// `parts[i].fraction` is that slice's share of the block's duration,
/// rounded to 0.01 — the unit `structural_diff` compares split changes at.
fn build_snapshot(block: &Block, slices: &[BillingSlice]) -> ResolutionSnapshot {
    let total = block.duration_seconds.max(1) as f64;
    let parts = slices
        .iter()
        .map(|s| {
            let seconds: i64 = s.intervals.iter().map(|(from, to)| to - from).sum();
            let fraction = ((seconds as f64 / total) * 100.0).round() / 100.0;
            ShareRow {
                customer: s.customer.clone().unwrap_or_default(),
                deild: s.deild.clone(),
                fraction,
            }
        })
        .collect();
    ResolutionSnapshot {
        day: block.day.clone(),
        started_at: block.started_at.clone(),
        description: block.description.clone(),
        parts,
    }
}

fn load_snapshot(
    conn: &Connection,
    day: &str,
    started_at: &str,
) -> Result<Option<ResolutionSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT description, parts_json FROM block_resolution_snapshots
          WHERE day = ?1 AND started_at = ?2",
    )?;
    let mut rows = stmt.query(params![day, started_at])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let description: Option<String> = row.get(0)?;
    let parts_json: String = row.get(1)?;
    // Trust boundary (design.md §2): a malformed parts_json is treated as
    // no snapshot at all, so it re-seeds silently instead of erroring.
    let Ok(parts) = serde_json::from_str::<Vec<ShareRow>>(&parts_json) else {
        return Ok(None);
    };
    Ok(Some(ResolutionSnapshot {
        day: day.to_string(),
        started_at: started_at.to_string(),
        description,
        parts,
    }))
}

fn store_snapshot(conn: &Connection, snapshot: &ResolutionSnapshot) -> Result<()> {
    let parts_json =
        serde_json::to_string(&snapshot.parts).context("serializing resolution snapshot")?;
    conn.execute(
        "INSERT INTO block_resolution_snapshots (day, started_at, description, parts_json)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(day, started_at) DO UPDATE SET
             description = excluded.description, parts_json = excluded.parts_json",
        params![
            snapshot.day,
            snapshot.started_at,
            snapshot.description,
            parts_json
        ],
    )
    .context("upserting block_resolution_snapshots")?;
    Ok(())
}

fn delete_snapshot(conn: &Connection, day: &str, started_at: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM block_resolution_snapshots WHERE day = ?1 AND started_at = ?2",
        params![day, started_at],
    )
    .context("deleting block_resolution_snapshots row")?;
    Ok(())
}

fn stored_started_at(conn: &Connection, day: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT started_at FROM block_resolution_snapshots WHERE day = ?1")?;
    let rows = stmt
        .query_map(params![day], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("listing block_resolution_snapshots")?;
    Ok(rows)
}

/// Logs at most one structural change (Customer, cascading to Deild,
/// cascading to Split) plus, independently, a Description change — a
/// description rewrite over an unchanged split logs only Description
/// (spec Journey 4, edge path). Returns the number of rows logged.
fn diff_and_log(
    conn: &Connection,
    old: &ResolutionSnapshot,
    new: &ResolutionSnapshot,
    source: ChangeSource,
    batch: &str,
) -> Result<usize> {
    let mut logged = 0;
    if let Some(field) = structural_diff(&old.parts, &new.parts) {
        let (o, n) = (
            Some(format_parts(&old.parts)),
            Some(format_parts(&new.parts)),
        );
        insert_change(conn, new, field, o, n, source, batch)?;
        logged += 1;
    }
    if old.description != new.description {
        let (o, n) = (old.description.clone(), new.description.clone());
        insert_change(conn, new, ChangeField::Description, o, n, source, batch)?;
        logged += 1;
    }
    Ok(logged)
}

/// The FR-09 cascade: the customer set first, then (customer, deild)
/// pairs for the same customers, then fractions for the same pairs.
fn structural_diff(old: &[ShareRow], new: &[ShareRow]) -> Option<ChangeField> {
    if customer_set(old) != customer_set(new) {
        return Some(ChangeField::Customer);
    }
    if pair_set(old) != pair_set(new) {
        return Some(ChangeField::Deild);
    }
    (fraction_list(old) != fraction_list(new)).then_some(ChangeField::Split)
}

fn customer_set(rows: &[ShareRow]) -> BTreeSet<&str> {
    rows.iter().map(|p| p.customer.as_str()).collect()
}

fn pair_set(rows: &[ShareRow]) -> BTreeSet<(&str, Option<&str>)> {
    rows.iter()
        .map(|p| (p.customer.as_str(), p.deild.as_deref()))
        .collect()
}

/// Fractions are already rounded to 0.01; comparing the formatted value
/// sidesteps float-equality pitfalls without a tolerance const.
fn fraction_list(rows: &[ShareRow]) -> Vec<(&str, Option<&str>, String)> {
    let mut v: Vec<_> = rows
        .iter()
        .map(|p| {
            (
                p.customer.as_str(),
                p.deild.as_deref(),
                format!("{:.2}", p.fraction),
            )
        })
        .collect();
    v.sort();
    v
}

/// `"Sjúkra·Rekstur 50% · APRÓ·AI hraðall 50%"` — the shared old/new
/// display for Customer, Deild and Split changes (design.md §6: built
/// separately from the TS formatter, no shared code).
fn format_parts(parts: &[ShareRow]) -> String {
    parts
        .iter()
        .map(|p| {
            let label = match &p.deild {
                Some(d) if !d.trim().is_empty() => format!("{}·{}", p.customer, d),
                _ => p.customer.clone(),
            };
            format!("{label} {}%", (p.fraction * 100.0).round() as i64)
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn insert_change(
    conn: &Connection,
    snapshot: &ResolutionSnapshot,
    field: ChangeField,
    old: Option<String>,
    new: Option<String>,
    source: ChangeSource,
    batch: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO block_changes (day, started_at, field, old_value, new_value, source, batch)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            snapshot.day,
            snapshot.started_at,
            to_col(field),
            old,
            new,
            to_col(source),
            batch
        ],
    )
    .context("inserting block_changes row")?;
    Ok(())
}

/// A malformed `field`/`source` column fails the whole row the same way a
/// SQLite type mismatch would — both mean the row can't be trusted.
fn column_error(e: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
}

fn parse_row(r: &rusqlite::Row) -> rusqlite::Result<BlockChange> {
    let field: String = r.get(3)?;
    let source: String = r.get(6)?;
    let seen_at: Option<String> = r.get(9)?;
    Ok(BlockChange {
        id: r.get(0)?,
        day: r.get(1)?,
        started_at: r.get(2)?,
        field: from_col(&field).map_err(column_error)?,
        old: r.get(4)?,
        new: r.get(5)?,
        source: from_col(&source).map_err(column_error)?,
        batch: r.get(7)?,
        created_at: r.get(8)?,
        seen: seen_at.is_some(),
    })
}

fn query_feed(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<ChangeFeed> {
    let mut stmt = conn.prepare(sql)?;
    let changes = stmt
        .query_map(params, parse_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(changes_to_feed(changes))
}

/// One summary per batch (D-07), ordered by that batch's first change id
/// — the order `changes` (id-ascending) already visits batches in.
fn changes_to_feed(changes: Vec<BlockChange>) -> ChangeFeed {
    let cursor = changes.iter().map(|c| c.id).max().unwrap_or(0);
    let mut order: Vec<String> = Vec::new();
    let mut acc: HashMap<String, (ChangeSource, BTreeSet<(String, String)>)> = HashMap::new();
    for c in &changes {
        acc.entry(c.batch.clone())
            .or_insert_with(|| {
                order.push(c.batch.clone());
                (c.source, BTreeSet::new())
            })
            .1
            .insert((c.day.clone(), c.started_at.clone()));
    }
    let batches = order
        .into_iter()
        .map(|batch| {
            let (source, blocks) = acc.remove(&batch).expect("batch just recorded");
            ChangeBatch {
                batch,
                source,
                count: blocks.len() as i64,
            }
        })
        .collect();
    ChangeFeed {
        changes,
        batches,
        cursor,
    }
}

/// Round-trips an enum through its own `serde(rename_all = "snake_case")`
/// so the `TEXT` column and any JSON wire form can never drift apart.
fn to_col<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value).expect("contract enums always serialize") {
        serde_json::Value::String(s) => s,
        _ => unreachable!("contract enums serialize to a JSON string"),
    }
}

fn from_col<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .with_context(|| format!("unknown change-log column value '{raw}'"))
}

#[cfg(test)]
#[path = "change_log_test.rs"]
mod tests;
