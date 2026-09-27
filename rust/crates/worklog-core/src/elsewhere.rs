//! Per-day "done elsewhere" list — org commits/PRs whose sha is absent
//! from every local clone (`events.elsewhere = 1`, D-07) — and moving one
//! into a chosen block by hand (FR-05, FR-06). Populated by T006.
//!
//! `events.elsewhere` meaning (schema.sql documents the same):
//!   0 = normal event, eligible for inference.
//!   1 = org commit/PR whose sha is in no local clone; listed here,
//!       excluded from every block.
//!   2 = owner-moved into a block by hand (`move_into_block`). Never
//!       votes, extends or lists (FR-06) — a collector re-run or the
//!       upgrade_006 re-resolve must never touch it back to 0/1.

use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::personal;

/// One row of the per-day "done elsewhere" list (FR-05).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ElsewhereItem {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub title: String,
    pub repo: Option<String>,
}

/// Events flagged `elsewhere = 1` whose local-TZ day (`tz::local_date`,
/// the same bucketing blocks use) is `day`, time-ordered.
pub fn list_for_day(conn: &Connection, day: NaiveDate) -> Result<Vec<ElsewhereItem>> {
    // started_at is a fixed-width ISO-8601 string; lexicographic comparison
    // works once the `+00:00` suffix is trimmed the same way on both sides
    // (see infer::load_day_events's `iso_prefix`).
    let (start, end) = day_window(day);
    let mut stmt = conn.prepare(
        "SELECT id, source, started_at, title, repo
           FROM events
          WHERE elsewhere = 1 AND started_at >= ?1 AND started_at < ?2
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![start, end], |r| {
        Ok(ElsewhereItem {
            id: r.get(0)?,
            source: r.get(1)?,
            started_at: r.get(2)?,
            title: r.get(3)?,
            repo: r.get(4)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn day_window(day: NaiveDate) -> (String, String) {
    let (start_utc, end_utc) = crate::tz::utc_window_for_local_day(day);
    (
        start_utc.to_rfc3339().trim_end_matches("+00:00").to_owned(),
        end_utc.to_rfc3339().trim_end_matches("+00:00").to_owned(),
    )
}

/// Move an elsewhere-flagged event into a chosen block by hand (FR-06):
/// keys it to the block's dominant project and flags it owner-moved
/// (`elsewhere = 2`) so neither a collector re-run nor a re-infer ever
/// sends it back to "done elsewhere" or lets it vote/extend a block.
pub fn move_into_block(conn: &Connection, event_id: i64, block_id: i64) -> Result<()> {
    let elsewhere: Option<i64> = conn
        .query_row(
            "SELECT elsewhere FROM events WHERE id = ?1",
            params![event_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(elsewhere) = elsewhere else {
        bail!("event {event_id} not found");
    };
    if elsewhere == 0 {
        bail!("event {event_id} is not elsewhere");
    }
    let block_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM blocks WHERE id = ?1)",
        params![block_id],
        |r| r.get(0),
    )?;
    if !block_exists {
        bail!("block {block_id} not found");
    }

    let dominant = personal::dominant_project_path_for_block(conn, block_id)?;
    conn.execute(
        "UPDATE events SET project_path = ?1, elsewhere = 2 WHERE id = ?2",
        params![dominant, event_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )?;
    Ok(())
}

/// FR-06: after `infer::persist_blocks` rebuilds `day`'s blocks, re-link
/// every owner-moved event (`elsewhere = 2`) on that day into one of them
/// — the one whose linked events' dominant folder matches the moved
/// event's, nearest in time; falling back to the nearest non-personal
/// block; left unlinked if the day has no blocks at all. Called once,
/// inside `persist_blocks`'s own transaction, after its inserts.
pub(crate) fn relink_moved_events(conn: &Connection, day: NaiveDate) -> Result<()> {
    let (start, end) = day_window(day);
    let mut stmt = conn.prepare(
        "SELECT id, started_at, project_path FROM events
          WHERE elsewhere = 2 AND started_at >= ?1 AND started_at < ?2",
    )?;
    let moved: Vec<(i64, String, Option<String>)> = stmt
        .query_map(params![start, end], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    if moved.is_empty() {
        return Ok(());
    }

    let day_iso = day.to_string();
    let mut block_stmt =
        conn.prepare("SELECT id, started_at, is_personal FROM blocks WHERE day = ?1")?;
    let blocks: Vec<(i64, String, bool)> = block_stmt
        .query_map(params![day_iso], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0))
        })?
        .collect::<std::result::Result<_, _>>()?;
    drop(block_stmt);
    if blocks.is_empty() {
        return Ok(());
    }

    let mut folders = std::collections::HashMap::new();
    for (id, _, _) in &blocks {
        let dominant = personal::dominant_project_path_for_block(conn, *id)?;
        folders.insert(
            *id,
            dominant.as_deref().and_then(crate::billing::work_folder_for_path),
        );
    }

    for (event_id, started_at, project_path) in moved {
        let event_folder = project_path.as_deref().and_then(crate::billing::work_folder_for_path);
        let event_ts = parse_ts(&started_at);
        let target = event_folder
            .as_ref()
            .and_then(|f| {
                blocks
                    .iter()
                    .filter(|(id, _, _)| folders.get(id).and_then(|x| x.as_ref()) == Some(f))
                    .min_by_key(|(_, started, _)| time_distance(started, event_ts))
            })
            .or_else(|| {
                blocks
                    .iter()
                    .filter(|(_, _, is_personal)| !is_personal)
                    .min_by_key(|(_, started, _)| time_distance(started, event_ts))
            });
        if let Some((block_id, _, _)) = target {
            conn.execute(
                "INSERT OR IGNORE INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![block_id, event_id],
            )?;
        }
    }
    Ok(())
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn time_distance(block_started: &str, event_ts: Option<DateTime<Utc>>) -> i64 {
    match (parse_ts(block_started), event_ts) {
        (Some(b), Some(e)) => (b - e).num_seconds().abs(),
        _ => i64::MAX,
    }
}

// Tests live in elsewhere_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "elsewhere_test.rs"]
mod tests;
