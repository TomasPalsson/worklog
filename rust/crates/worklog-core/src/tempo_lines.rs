//! Stored Tempo ticket lines (spec 011): one line per day and `jira_issue`,
//! with a stored worklog text and an optional hours override. Sibling of
//! `line_text.rs`, deliberately not generalised with it.

use crate::billing::{block_interval, union_seconds};
use crate::clues_contract::LineTextOrigin;
use crate::collectors::tempo::{
    ask_model, distinct_descriptions, round_to_half_hour, summarize_descriptions,
    try_summarize_descriptions,
};
use crate::estimate::ModelInvoker;
use crate::models::Block;
use crate::repo;
use crate::tempo_line_contract::{
    SetTempoLineHours, SetTempoLineText, TempoLine, TempoLineKey, HALF_HOUR_SECONDS,
};
use crate::updater::crypto::sha256_hex;
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

/// Caller-supplied hours that are not a positive multiple of half an hour;
/// the daemon maps it to 400 by type, not by message.
#[derive(Debug, thiserror::Error)]
#[error("hours override must be a positive multiple of 30 minutes, got {0}s")]
pub struct InvalidHours(pub i64);

struct StoredRow {
    text: Option<String>,
    text_origin: Option<LineTextOrigin>,
    source_hash: Option<String>,
    hours_override_seconds: Option<i64>,
}

fn stored_row(conn: &Connection, key: &TempoLineKey) -> Result<Option<StoredRow>> {
    conn.query_row(
        "SELECT text, text_origin, source_hash, hours_override_seconds
           FROM tempo_line_texts WHERE day = ?1 AND jira_issue = ?2",
        params![key.day, key.jira_issue],
        |r| {
            let origin: Option<String> = r.get(1)?;
            Ok(StoredRow {
                text: r.get(0)?,
                text_origin: origin.map(|o| {
                    if o == "manual" {
                        LineTextOrigin::Manual
                    } else {
                        LineTextOrigin::Generated
                    }
                }),
                source_hash: r.get(2)?,
                hours_override_seconds: r.get(3)?,
            })
        },
    )
    .optional()
    .context("stored_row")
}

/// The day's non-personal, ticketed blocks grouped by ticket, in start order.
fn blocks_by_ticket(conn: &Connection, day: &str) -> Result<BTreeMap<String, Vec<Block>>> {
    let mut grouped: BTreeMap<String, Vec<Block>> = BTreeMap::new();
    for block in repo::list_blocks_for_day(conn, day)? {
        if block.is_personal {
            continue;
        }
        match block.jira_issue.clone() {
            Some(issue) if !issue.is_empty() => grouped.entry(issue).or_default().push(block),
            _ => {}
        }
    }
    Ok(grouped)
}

fn descriptions_of(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| block.description.as_deref())
        .map(|description| description.trim().to_string())
        .filter(|description| !description.is_empty())
        .collect()
}

fn source_hash(descriptions: &[String]) -> String {
    let mut sorted = descriptions.to_vec();
    sorted.sort();
    sha256_hex(sorted.join("\n").as_bytes())
}

fn build_line(conn: &Connection, key: &TempoLineKey, blocks: &[Block]) -> Result<TempoLine> {
    let stored = stored_row(conn, key)?;
    let union = round_to_half_hour(union_seconds(blocks.iter().map(block_interval).collect()));
    let override_seconds = stored.as_ref().and_then(|row| row.hours_override_seconds);
    Ok(TempoLine {
        day: key.day.clone(),
        jira_issue: key.jira_issue.clone(),
        text: stored.as_ref().and_then(|row| row.text.clone()),
        text_origin: stored.as_ref().and_then(|row| row.text_origin),
        fallback_text: summarize_descriptions(
            None,
            &key.jira_issue,
            &descriptions_of(blocks),
            "",
            true,
        ),
        union_seconds: union,
        hours_override_seconds: override_seconds,
        effective_seconds: override_seconds.unwrap_or(union),
    })
}

/// One line per distinct ticket among the day's non-personal blocks,
/// sorted by ticket.
pub fn lines_for_day(conn: &Connection, day: &str) -> Result<Vec<TempoLine>> {
    blocks_by_ticket(conn, day)?
        .into_iter()
        .map(|(issue, blocks)| {
            let key = TempoLineKey {
                day: day.to_string(),
                jira_issue: issue,
            };
            build_line(conn, &key, &blocks)
        })
        .collect()
}

/// `None` when no non-personal block has that ticket that day.
pub fn line_for(conn: &Connection, key: &TempoLineKey) -> Result<Option<TempoLine>> {
    let mut grouped = blocks_by_ticket(conn, &key.day)?;
    grouped
        .remove(&key.jira_issue)
        .map(|blocks| build_line(conn, key, &blocks))
        .transpose()
}

fn ensure_row(conn: &Connection, key: &TempoLineKey) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO tempo_line_texts (day, jira_issue, updated_at) VALUES (?1, ?2, ?3)",
        params![key.day, key.jira_issue, Utc::now().to_rfc3339()],
    )
    .context("ensure_row")?;
    Ok(())
}

/// Drops a row left with neither text nor override, and marks the line's
/// already-synced blocks for re-sync.
fn finish_write(conn: &Connection, key: &TempoLineKey) -> Result<()> {
    conn.execute(
        "DELETE FROM tempo_line_texts
          WHERE day = ?1 AND jira_issue = ?2
            AND text IS NULL AND hours_override_seconds IS NULL",
        params![key.day, key.jira_issue],
    )
    .context("finish_write delete")?;
    conn.execute(
        "UPDATE blocks SET dirty = 1
          WHERE day = ?1 AND jira_issue = ?2 AND is_personal = 0
            AND tempo_worklog_id IS NOT NULL AND tempo_worklog_id != ''",
        params![key.day, key.jira_issue],
    )
    .context("finish_write dirty")?;
    Ok(())
}

/// Hand-written text; blank clears it. `None` when the line has no blocks.
pub fn set_text(conn: &Connection, body: &SetTempoLineText) -> Result<Option<TempoLine>> {
    let key = TempoLineKey {
        day: body.day.clone(),
        jira_issue: body.jira_issue.clone(),
    };
    if line_for(conn, &key)?.is_none() {
        return Ok(None);
    }
    ensure_row(conn, &key)?;
    let trimmed = body.text.trim();
    let (text, origin) = if trimmed.is_empty() {
        (None, None)
    } else {
        (Some(trimmed), Some("manual"))
    };
    conn.execute(
        "UPDATE tempo_line_texts
            SET text = ?3, text_origin = ?4, source_hash = NULL, updated_at = ?5
          WHERE day = ?1 AND jira_issue = ?2",
        params![
            key.day,
            key.jira_issue,
            text,
            origin,
            Utc::now().to_rfc3339()
        ],
    )
    .context("set_text")?;
    finish_write(conn, &key)?;
    line_for(conn, &key)
}

/// Hours override; `None` clears it. Rejects anything but a positive
/// multiple of half an hour before touching the database.
pub fn set_hours(conn: &Connection, body: &SetTempoLineHours) -> Result<Option<TempoLine>> {
    if let Some(seconds) = body.seconds {
        if seconds <= 0 || seconds % HALF_HOUR_SECONDS != 0 {
            return Err(InvalidHours(seconds).into());
        }
    }
    let key = TempoLineKey {
        day: body.day.clone(),
        jira_issue: body.jira_issue.clone(),
    };
    if line_for(conn, &key)?.is_none() {
        return Ok(None);
    }
    ensure_row(conn, &key)?;
    conn.execute(
        "UPDATE tempo_line_texts SET hours_override_seconds = ?3, updated_at = ?4
          WHERE day = ?1 AND jira_issue = ?2",
        params![
            key.day,
            key.jira_issue,
            body.seconds,
            Utc::now().to_rfc3339()
        ],
    )
    .context("set_hours")?;
    finish_write(conn, &key)?;
    line_for(conn, &key)
}

/// `(key, descriptions, source_hash)` for every line with no text or a
/// generated text whose descriptions changed. `force` returns only that
/// line, even when its text is hand-written.
pub fn pending_generation(
    conn: &Connection,
    day: &str,
    force: Option<&TempoLineKey>,
) -> Result<Vec<(TempoLineKey, Vec<String>, String)>> {
    let mut pending = Vec::new();
    for (issue, blocks) in blocks_by_ticket(conn, day)? {
        let key = TempoLineKey {
            day: day.to_string(),
            jira_issue: issue,
        };
        let descriptions = descriptions_of(&blocks);
        let hash = source_hash(&descriptions);
        let wanted = match force {
            Some(forced) => forced == &key,
            None => match stored_row(conn, &key)? {
                Some(StoredRow {
                    text: Some(_),
                    text_origin: Some(LineTextOrigin::Manual),
                    ..
                }) => false,
                Some(StoredRow {
                    text: Some(_),
                    source_hash,
                    ..
                }) => source_hash.as_deref() != Some(hash.as_str()),
                _ => true,
            },
        };
        if wanted {
            pending.push((key, descriptions, hash));
        }
    }
    Ok(pending)
}

/// The one-line worklog text for a line, via the model when several
/// distinct descriptions need summarising.
pub fn generate_text(
    invoker: Option<&dyn ModelInvoker>,
    key: &TempoLineKey,
    descriptions: &[String],
    model: &str,
) -> Option<String> {
    try_summarize_descriptions(invoker, &key.jira_issue, descriptions, model, false)
}

/// An explicit Generate/Regenerate: always asks the model (copying a lone
/// description back would look like the button did nothing) and errors loudly.
pub fn rewrite_text(
    invoker: &dyn ModelInvoker,
    key: &TempoLineKey,
    descriptions: &[String],
    model: &str,
) -> Result<String> {
    let unique = distinct_descriptions(descriptions);
    if unique.is_empty() {
        anyhow::bail!(
            "no block on {} has a description yet — describe a block first",
            key.jira_issue
        );
    }
    ask_model(invoker, &key.jira_issue, &unique, model)
}

/// Stores generated text; a hand-written text is kept unless `force`.
pub fn commit_generated(
    conn: &Connection,
    key: &TempoLineKey,
    text: &str,
    source_hash: &str,
    force: bool,
) -> Result<()> {
    let manual =
        stored_row(conn, key)?.is_some_and(|row| row.text_origin == Some(LineTextOrigin::Manual));
    if manual && !force {
        return Ok(());
    }
    ensure_row(conn, key)?;
    conn.execute(
        "UPDATE tempo_line_texts
            SET text = ?3, text_origin = 'generated', source_hash = ?4, updated_at = ?5
          WHERE day = ?1 AND jira_issue = ?2",
        params![
            key.day,
            key.jira_issue,
            text,
            source_hash,
            Utc::now().to_rfc3339()
        ],
    )
    .context("commit_generated")?;
    finish_write(conn, key)
}

#[path = "tempo_lines_test.rs"]
#[cfg(test)]
mod tests;
