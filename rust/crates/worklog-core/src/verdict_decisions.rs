//! The permanent decision log (spec 017): every Verdict guess and every
//! Owner correction, read back for examples, replay and the unchecked count.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::tz::utc_window_for_local_day;
use crate::verdict_contract::{DecisionKind, DecisionRow, DecisionSource};

const COLUMNS: &str =
    "kind, source, subject, state_json, options, ranking, chosen, previous, decided_at";

fn kind_str(k: DecisionKind) -> &'static str {
    match k {
        DecisionKind::Project => "project",
        DecisionKind::Ticket => "ticket",
        DecisionKind::LineText => "line_text",
    }
}

fn source_str(s: DecisionSource) -> &'static str {
    match s {
        DecisionSource::Verdict => "verdict",
        DecisionSource::Owner => "owner",
    }
}

fn parse_kind(s: &str) -> Option<DecisionKind> {
    [
        DecisionKind::Project,
        DecisionKind::Ticket,
        DecisionKind::LineText,
    ]
    .into_iter()
    .find(|k| kind_str(*k) == s)
}

fn parse_source(s: &str) -> Option<DecisionSource> {
    [DecisionSource::Verdict, DecisionSource::Owner]
        .into_iter()
        .find(|v| source_str(*v) == s)
}

fn bad_column(i: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(i, "unknown enum value".into(), rusqlite::types::Type::Text)
}

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<DecisionRow> {
    let kind: String = r.get(0)?;
    let source: String = r.get(1)?;
    let options: String = r.get(4)?;
    let ranking: Option<String> = r.get(5)?;
    Ok(DecisionRow {
        kind: parse_kind(&kind).ok_or_else(|| bad_column(0))?,
        source: parse_source(&source).ok_or_else(|| bad_column(1))?,
        subject: r.get(2)?,
        state_json: r.get(3)?,
        options: serde_json::from_str(&options).unwrap_or_default(),
        ranking: ranking.and_then(|j| serde_json::from_str(&j).ok()),
        chosen: r.get(6)?,
        previous: r.get(7)?,
        decided_at: r.get(8)?,
    })
}

pub fn record(conn: &Connection, row: &DecisionRow) -> Result<i64> {
    let ranking = row
        .ranking
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    conn.execute(
        &format!(
            "INSERT INTO verdict_decisions ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        ),
        params![
            kind_str(row.kind),
            source_str(row.source),
            row.subject,
            row.state_json,
            serde_json::to_string(&row.options)?,
            ranking,
            row.chosen,
            row.previous,
            row.decided_at,
        ],
    )
    .context("recording verdict decision")?;
    Ok(conn.last_insert_rowid())
}

/// Rows of `kind` decided at or after `since`, oldest first.
pub fn list_since(conn: &Connection, kind: DecisionKind, since: &str) -> Result<Vec<DecisionRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM verdict_decisions
          WHERE kind = ?1 AND decided_at >= ?2 ORDER BY decided_at, id"
    ))?;
    let rows = stmt.query_map(params![kind_str(kind), since], row_from)?;
    rows.collect::<std::result::Result<_, _>>()
        .context("listing verdict decisions")
}

/// Titles of the events the Owner corrected TO `folder`, newest first.
pub fn examples_for(conn: &Connection, folder: &str, limit: usize) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.title FROM verdict_decisions d
           JOIN events e ON e.id = CAST(d.subject AS INTEGER)
          WHERE d.kind = 'project' AND d.source = 'owner' AND d.chosen = ?1
          ORDER BY d.decided_at DESC, d.id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![folder, limit as i64], |r| r.get(0))?;
    rows.collect::<std::result::Result<_, _>>()
        .context("listing correction examples")
}

pub fn latest_for(
    conn: &Connection,
    kind: DecisionKind,
    subject: &str,
) -> Result<Option<DecisionRow>> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM verdict_decisions
              WHERE kind = ?1 AND subject = ?2 ORDER BY decided_at DESC, id DESC LIMIT 1"
        ),
        params![kind_str(kind), subject],
        row_from,
    )
    .optional()
    .context("reading latest verdict decision")
}

/// Unlabelled browser/Slack events of the local `day` with no Verdict project decision.
pub fn unchecked_count(conn: &Connection, day: &str) -> Result<u32> {
    let day = NaiveDate::parse_from_str(day, "%Y-%m-%d").context("parsing day")?;
    let (start, end) = utc_window_for_local_day(day);
    conn.query_row(
        "SELECT COUNT(*) FROM events e
          WHERE e.source IN (?3, ?4) AND e.label_origin IS NULL
            AND e.started_at >= ?1 AND e.started_at < ?2
            AND NOT EXISTS (SELECT 1 FROM verdict_decisions d
                             WHERE d.kind = 'project' AND d.source = 'verdict'
                               AND d.subject = CAST(e.id AS TEXT))",
        params![
            start.to_rfc3339(),
            end.to_rfc3339(),
            crate::routing_contract::SOURCE_FIREFOX,
            crate::routing_contract::SOURCE_SLACK
        ],
        |r| r.get(0),
    )
    .context("counting unchecked events")
}

#[path = "verdict_decisions_test.rs"]
#[cfg(test)]
mod tests;
