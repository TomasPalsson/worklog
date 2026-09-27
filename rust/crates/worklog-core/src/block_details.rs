//! The Details view's backing query: every event of a block, including
//! helper/message activity in its span, in time order (spec 006, FR-20).
//! Populated by T015: `details_for_block`.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use std::collections::HashSet;

use crate::clues_contract::{
    RawRecord, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_MESSAGE, SOURCE_CLAUDE_TOOL,
};
use crate::models::Event;
use crate::repo;

/// One row of the Details view timeline (design §2, §4).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetailRow {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub title: String,
    pub details: Option<String>,
    pub repo: Option<String>,
    pub project_path: Option<String>,
    pub session_id: Option<String>,
    pub jira_issue: Option<String>,
    pub raw: Option<RawRecord>,
}

/// Every event linked to `block_id` via `block_events`, plus every
/// claude_tool/claude_helper/claude_message event whose session_id
/// matches a linked event's session and whose timestamp falls inside
/// the block's `[started_at, ended_at]` window — time-ordered, no
/// duplicates (FR-20).
pub fn details_for_block(conn: &Connection, block_id: i64) -> Result<Vec<DetailRow>> {
    let block =
        repo::get_block(conn, block_id)?.ok_or_else(|| anyhow!("block {block_id} not found"))?;

    let linked = repo::list_events_for_block(conn, block_id)?;
    let linked_ids: HashSet<i64> = linked.iter().filter_map(|e| e.id).collect();
    let session_ids: Vec<String> = linked
        .iter()
        .filter_map(|e| e.session_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut events = linked;
    if !session_ids.is_empty() {
        let span_start = parse_ts(&block.started_at);
        let span_end = parse_ts(&block.ended_at);
        let day_bounds = span_start
            .zip(span_end)
            .map(|(s, en)| widen_day_bounds(s, en));
        let candidates = helper_activity_for_sessions(conn, &session_ids, day_bounds.as_ref())?;
        events.extend(candidates.into_iter().filter(|e| {
            if e.id.is_some_and(|id| linked_ids.contains(&id)) {
                return false;
            }
            match (parse_ts(&e.started_at), span_start, span_end) {
                (Some(ts), Some(s), Some(en)) => ts >= s && ts <= en,
                _ => false,
            }
        }));
    }

    let mut rows: Vec<(DateTime<Utc>, i64, DetailRow)> = events
        .into_iter()
        .map(|e| {
            let ts = parse_ts(&e.started_at).unwrap_or(DateTime::<Utc>::MIN_UTC);
            let id = e.id.unwrap_or_default();
            (ts, id, to_detail_row(e))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(rows.into_iter().map(|(_, _, row)| row).collect())
}

fn to_detail_row(e: Event) -> DetailRow {
    let raw = e
        .raw_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<RawRecord>(s).ok());
    DetailRow {
        id: e.id.unwrap_or_default(),
        source: e.source,
        started_at: e.started_at,
        title: e.title,
        details: e.details,
        repo: e.repo,
        project_path: e.project_path,
        session_id: e.session_id,
        jira_issue: e.jira_issue,
        raw,
    }
}

/// Calendar-day bounds (`YYYY-MM-DD`) for a coarse SQL pre-filter on `events.started_at`: a naive timestamp's
/// leading date is already its UTC date, and an RFC3339 offset shifts only the instant, so the date written is at
/// most one UTC day off. Widening the span's UTC days by one day each side is thus a superset for every
/// `parse_ts`-accepted format, so SQL can only ever remove rows the Rust filter below would remove too.
fn widen_day_bounds(span_start: DateTime<Utc>, span_end: DateTime<Utc>) -> (String, String) {
    let lo = span_start.date_naive() - chrono::Duration::days(1);
    let hi = span_end.date_naive() + chrono::Duration::days(1);
    (
        lo.format("%Y-%m-%d").to_string(),
        hi.format("%Y-%m-%d").to_string(),
    )
}

/// `claude_tool`/`claude_helper`/`claude_message` events for any of `session_ids` — one query regardless of how
/// many sessions the block touches. `day_bounds` narrows the SQL pre-filter (see `widen_day_bounds`); the exact
/// filter still runs in Rust (`parse_ts`) since blocks/events use slightly different ISO formats.
fn helper_activity_for_sessions(
    conn: &Connection,
    session_ids: &[String],
    day_bounds: Option<&(String, String)>,
) -> Result<Vec<Event>> {
    let placeholders = vec!["?"; session_ids.len()].join(",");
    let day_filter = day_bounds.map_or("", |_| " AND substr(started_at, 1, 10) BETWEEN ? AND ?");
    let sql = format!(
        "SELECT id, source, source_id, started_at, ended_at,
                duration_seconds, title, details, repo,
                project_path, jira_issue, session_id,
                tempo_worklog_id, raw_json
           FROM events
          WHERE source IN (?, ?, ?)
            AND session_id IN ({placeholders}){day_filter}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params = [
        SOURCE_CLAUDE_TOOL,
        SOURCE_CLAUDE_HELPER,
        SOURCE_CLAUDE_MESSAGE,
    ]
    .into_iter()
    .chain(session_ids.iter().map(String::as_str))
    .chain(
        day_bounds
            .into_iter()
            .flat_map(|(lo, hi)| [lo.as_str(), hi.as_str()]),
    );
    let rows = stmt.query_map(params_from_iter(params), |r| {
        Ok(Event {
            id: Some(r.get(0)?),
            source: r.get(1)?,
            source_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            title: r.get(6)?,
            details: r.get(7)?,
            repo: r.get(8)?,
            project_path: r.get(9)?,
            jira_issue: r.get(10)?,
            session_id: r.get(11)?,
            tempo_worklog_id: r.get(12)?,
            raw_json: crate::raw_json::decode_raw_json(r, 13)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

/// Accepts the ISO-8601 variants this codebase writes (`+00:00` offset,
/// `Z`, or a naive string treated as UTC) — mirrors `infer::parse_maybe_utc`.
fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(naive.and_utc());
    }
    None
}

// Tests live in block_details_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "block_details_test.rs"]
mod tests;
