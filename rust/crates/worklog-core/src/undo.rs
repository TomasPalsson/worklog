//! Before-image journal for the Owner's block changes (spec 018, FR-25..28).
//! `block_service` calls [`record`] inside each change's transaction;
//! [`undo_last`] puts the newest entry back.

use anyhow::{Context, Result};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as Json};

use crate::daily_helpers_contract::{BlockChange, UndoOutcome, UNDO_DEPTH};

type Row = Map<String, Json>;

/// Columns undo never writes back: the id is the key, and the sent-marker
/// must survive an undo untouched (FR-28). A live block also keeps its
/// `exported_at`; only a re-inserted block gets its own back.
const SKIP_COLUMNS: [&str; 2] = ["id", "tempo_worklog_id"];

#[derive(Serialize, Deserialize)]
struct Snapshot {
    row: Row,
    events: Vec<i64>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    // Highest block id before the change; anything above it was created by it.
    max_id: i64,
    blocks: Vec<Snapshot>,
}

fn marker_set(v: Option<&str>) -> bool {
    v.is_some_and(|s| !s.trim().is_empty())
}

fn snapshot(tx: &Transaction, id: i64) -> Result<Option<Snapshot>> {
    let mut st = tx.prepare("SELECT * FROM blocks WHERE id = ?1")?;
    let names: Vec<String> = st.column_names().iter().map(|n| n.to_string()).collect();
    let row = st
        .query_row(params![id], |r| {
            let mut m = Row::new();
            for (i, name) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Integer(n) => Json::from(n),
                    ValueRef::Real(f) => Number::from_f64(f).map_or(Json::Null, Json::Number),
                    ValueRef::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Null | ValueRef::Blob(_) => Json::Null,
                };
                m.insert(name.clone(), v);
            }
            Ok(m)
        })
        .optional()
        .context("undo: reading block")?;
    let Some(row) = row else { return Ok(None) };
    let mut ev = tx.prepare("SELECT event_id FROM block_events WHERE block_id = ?1")?;
    let events = ev
        .query_map(params![id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(Some(Snapshot { row, events }))
}

/// Journals the current state of `before` (call it before the change is
/// written, in the change's transaction). A change that touches no
/// existing block leaves no entry.
pub fn record(tx: &Transaction, change: BlockChange, before: &[i64]) -> Result<()> {
    let mut blocks = Vec::new();
    for &id in before {
        blocks.extend(snapshot(tx, id)?);
    }
    if blocks.is_empty() {
        return Ok(());
    }
    let max_id: i64 = tx.query_row("SELECT COALESCE(MAX(id), 0) FROM blocks", [], |r| r.get(0))?;
    let payload = serde_json::to_string(&Entry { max_id, blocks })?;
    tx.execute(
        "INSERT INTO block_undo (change, payload_json) VALUES (?1, ?2)",
        params![serde_json::to_value(change)?.as_str(), payload],
    )
    .context("undo: journaling change")?;
    tx.execute(
        "DELETE FROM block_undo WHERE id <=
            (SELECT id FROM block_undo ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        params![UNDO_DEPTH as i64],
    )
    .context("undo: trimming journal")?;
    Ok(())
}

fn sql_value(v: &Json) -> Value {
    match v {
        Json::Number(n) => n.as_i64().map_or_else(
            || Value::Real(n.as_f64().unwrap_or_default()),
            Value::Integer,
        ),
        Json::String(s) => Value::Text(s.clone()),
        _ => Value::Null,
    }
}

fn str_of<'a>(row: &'a Row, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Json::as_str)
}

fn id_of(s: &Snapshot) -> i64 {
    s.row.get("id").and_then(Json::as_i64).unwrap_or_default()
}

fn write_block(tx: &Transaction, s: &Snapshot) -> Result<()> {
    let id = id_of(s);
    let exists = tx
        .query_row(
            "SELECT 1 FROM blocks WHERE id = ?1",
            params![id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let cols: Vec<(&String, Value)> = s
        .row
        .iter()
        .filter(|(k, _)| {
            let kept_live = exists && k.as_str() == "exported_at";
            !(SKIP_COLUMNS.contains(&k.as_str()) || kept_live)
        })
        .map(|(k, v)| (k, sql_value(v)))
        .collect();
    let mut args: Vec<Value> = cols.iter().map(|(_, v)| v.clone()).collect();
    args.push(Value::Integer(id));
    let n = args.len();
    if exists {
        let set: Vec<String> = cols
            .iter()
            .enumerate()
            .map(|(i, (k, _))| format!("\"{k}\" = ?{}", i + 1))
            .collect();
        tx.execute(
            &format!("UPDATE blocks SET {} WHERE id = ?{n}", set.join(", ")),
            rusqlite::params_from_iter(args),
        )
    } else {
        let names: Vec<String> = cols.iter().map(|(k, _)| format!("\"{k}\"")).collect();
        let marks: Vec<String> = (1..n).map(|i| format!("?{i}")).collect();
        tx.execute(
            &format!(
                "INSERT INTO blocks (id, {}) VALUES (?{n}, {})",
                names.join(", "),
                marks.join(", ")
            ),
            rusqlite::params_from_iter(args),
        )
    }
    .with_context(|| format!("undo: restoring block {id}"))?;
    Ok(())
}

/// Blocks a split created: new since the journal entry and inside the
/// original block's span.
fn split_tails(tx: &Transaction, e: &Entry) -> Result<Vec<i64>> {
    let Some(first) = e.blocks.first() else {
        return Ok(vec![]);
    };
    let (day, start, end) = (
        str_of(&first.row, "day"),
        str_of(&first.row, "started_at"),
        str_of(&first.row, "ended_at"),
    );
    let mut st = tx.prepare(
        "SELECT id FROM blocks
          WHERE id > ?1 AND day = ?2 AND started_at >= ?3 AND ended_at <= ?4",
    )?;
    let ids = st
        .query_map(params![e.max_id, day, start, end], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

fn synced_now(tx: &Transaction, id: i64) -> Result<bool> {
    let marker: Option<Option<String>> = tx
        .query_row(
            "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(marker_set(marker.flatten().as_deref()))
}

/// Reverses the newest journaled change. Refuses, changing nothing, when
/// it would touch a block sent to Tempo.
pub fn undo_last(conn: &mut Connection) -> Result<UndoOutcome> {
    let tx = conn.transaction()?;
    let newest: Option<(i64, String, String)> = tx
        .query_row(
            "SELECT id, change, payload_json FROM block_undo ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((entry_id, change, payload)) = newest else {
        return Ok(UndoOutcome::NothingToUndo);
    };
    let change: BlockChange = serde_json::from_value(Json::String(change))?;
    let entry: Entry = serde_json::from_str(&payload).context("undo: reading journal entry")?;
    let tails = if change == BlockChange::Split {
        split_tails(&tx, &entry)?
    } else {
        vec![]
    };
    let block_ids: Vec<i64> = entry.blocks.iter().map(id_of).collect();

    for s in &entry.blocks {
        if marker_set(str_of(&s.row, "tempo_worklog_id")) || synced_now(&tx, id_of(s))? {
            return Ok(UndoOutcome::RefusedSynced { block_id: id_of(s) });
        }
    }
    for &t in &tails {
        if synced_now(&tx, t)? {
            return Ok(UndoOutcome::RefusedSynced { block_id: t });
        }
    }

    for &t in &tails {
        tx.execute("DELETE FROM blocks WHERE id = ?1", params![t])?;
    }
    for s in &entry.blocks {
        write_block(&tx, s)?;
    }
    for s in &entry.blocks {
        tx.execute(
            "DELETE FROM block_events WHERE block_id = ?1",
            params![id_of(s)],
        )?;
        for e in &s.events {
            tx.execute(
                "INSERT OR IGNORE INTO block_events (block_id, event_id)
                 SELECT ?1, id FROM events WHERE id = ?2",
                params![id_of(s), e],
            )?;
        }
    }
    tx.execute("DELETE FROM block_undo WHERE id = ?1", params![entry_id])?;
    tx.commit()?;
    Ok(UndoOutcome::Restored { change, block_ids })
}

#[cfg(test)]
#[path = "undo_test.rs"]
mod tests;
