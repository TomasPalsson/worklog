//! The per-block card kept after a block's raw events are deleted.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use crate::billing_registry::Registry;
use crate::block_details;
use crate::clues_collect;
use crate::clues_contract::RawRecord;
use crate::digest_contract::*;
use crate::models::{Block, Event};
use crate::tenant_contract::SplitOrigin;
use crate::{billing, personal, repo, scrub, tenant_split};

pub fn horizon(today: NaiveDate) -> NaiveDate {
    today - Duration::days(HORIZON_DAYS)
}

/// The repos and first few distinct titles of a block's events — what the
/// eval helper reads. Titles are never scrubbed: the card must equal what
/// eval sends from live events.
pub fn eval_evidence(events: &[Event]) -> (Vec<String>, Vec<String>) {
    let mut repos: Vec<String> = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    for event in events {
        if let Some(repo_name) = event.repo.clone().filter(|r| !repos.contains(r)) {
            repos.push(repo_name);
        }
        let title: String = event.title.chars().take(EVAL_TITLE_CHARS).collect();
        if titles.len() < MAX_EVAL_TITLES && !titles.contains(&title) {
            titles.push(title);
        }
    }
    (repos, titles)
}

pub fn build_digest(conn: &Connection, block_id: i64) -> Result<BlockDigest> {
    let block =
        repo::get_block(conn, block_id)?.ok_or_else(|| anyhow!("block {block_id} not found"))?;
    let events = repo::list_events_for_block(conn, block_id)?;
    let (eval_repos, eval_titles) = eval_evidence(&events);

    let mut card = BlockDigest {
        eval_repos,
        eval_titles,
        paths: capped(
            ranked_by_count(events.iter().filter_map(|e| e.project_path.clone())),
            MAX_PATHS,
            PROJECT_PATH_CHARS,
        ),
        invoice_titles: capped(
            ranked_by_count(
                events
                    .iter()
                    .filter(|e| !e.source.starts_with("claude"))
                    .map(|e| e.title.trim().to_string()),
            ),
            MAX_INVOICE_TITLES,
            INVOICE_TITLE_CHARS,
        ),
        event_count: events.len() as i64,
        events_by_source: count_by(events.iter().map(|e| e.source.clone())),
        session_count: events
            .iter()
            .filter_map(|e| e.session_id.as_deref())
            .collect::<HashSet<_>>()
            .len() as i64,
        active_minutes: block.duration_seconds / 60,
        first_at: events.first().map(|e| e.started_at.clone()),
        last_at: events.last().map(|e| e.started_at.clone()),
        jira_summary: jira_summary(conn, &block)?,
        ..BlockDigest::default()
    };
    if block.is_personal {
        return Ok(card);
    }

    card.folder = billing::work_folder_for_block(conn, block_id)?
        .map(|folder| truncate(&folder, FOLDER_CHARS));
    card.project_path = personal::dominant_project_path_for_block(conn, block_id)?
        .map(|path| truncate(&path, PROJECT_PATH_CHARS));
    card.pinned_customer = pinned_customer(conn, &block, card.folder.as_deref())?;
    add_estimation_evidence(conn, block_id, &mut card)?;
    Ok(card)
}

/// INSERT OR IGNORE: a card, once written, is never replaced.
pub fn write_digest(conn: &Connection, block_id: i64, digest: &BlockDigest) -> Result<bool> {
    let written = conn.execute(
        "INSERT OR IGNORE INTO block_digest (block_id, version, built_at, json)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?3)",
        params![block_id, DIGEST_VERSION, serde_json::to_string(digest)?],
    )?;
    Ok(written > 0)
}

pub fn digest_for_block(conn: &Connection, block_id: i64) -> Result<Option<BlockDigest>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM block_digest WHERE block_id = ?1",
            [block_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json.and_then(|text| match serde_json::from_str(&text) {
        Ok(card) => Some(card),
        Err(err) => {
            eprintln!("block_digest: block {block_id} card is unreadable: {err}");
            None
        }
    }))
}

/// A day is compressed once any of its blocks holds a card.
pub fn day_is_compressed(conn: &Connection, day: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM blocks b JOIN block_digest d ON d.block_id = b.id
                        WHERE b.day = ?1)",
        [day],
        |r| r.get(0),
    )?)
}

fn jira_summary(conn: &Connection, block: &Block) -> Result<Option<String>> {
    let Some(key) = block.jira_issue.as_deref().filter(|k| !k.is_empty()) else {
        return Ok(None);
    };
    let summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM jira_tickets WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(summary.map(|s| truncate(&s, JIRA_SUMMARY_CHARS)))
}

/// The customer the block's pinned sessions name, when the tenant split
/// resolved the whole block to that single pin.
fn pinned_customer(
    conn: &Connection,
    block: &Block,
    folder: Option<&str>,
) -> Result<Option<String>> {
    let Some(folder) = folder else {
        return Ok(None);
    };
    let registry = Registry::load(conn)?;
    let slices = tenant_split::tenant_slices_for_block(conn, block, folder, &registry)?;
    Ok(slices.and_then(|slices| match slices.as_slice() {
        [only] if only.origin == SplitOrigin::Pinned => only.customer.clone(),
        _ => None,
    }))
}

/// Everything the description estimator reads, scrubbed of secrets and
/// then capped.
fn add_estimation_evidence(conn: &Connection, block_id: i64, card: &mut BlockDigest) -> Result<()> {
    let rows = block_details::details_for_block(conn, block_id)?;
    let collected = clues_collect::collect(conn, &rows);

    card.branches = capped(
        scrubbed_distinct(&collected.branches),
        MAX_BRANCHES,
        BRANCH_CHARS,
    );
    card.change_titles = capped(
        scrubbed_distinct(&collected.change_titles),
        MAX_CHANGE_TITLES,
        CHANGE_TITLE_CHARS,
    );
    let prompts: Vec<String> = scrubbed_distinct(&collected.prompts)
        .into_iter()
        .filter(|p| p.chars().count() >= PROMPT_MIN_CHARS)
        .collect();
    card.prompt_count = prompts.len() as i64;
    card.prompts = capped(prompts, MAX_PROMPTS, PROMPT_CHARS);
    let files = scrubbed_distinct(&collected.file_basenames);
    card.files_distinct = files.len() as i64;
    card.files = capped(files, MAX_FILES, PROJECT_PATH_CHARS);

    let tool_names = rows.iter().filter_map(|row| match &row.raw {
        Some(RawRecord::ClaudeTool { tool, .. }) => Some(tool.clone()),
        _ => None,
    });
    let counts = count_by(tool_names);
    let mut ranked: Vec<(&String, &i64)> = counts.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    card.tool_counts = ranked
        .into_iter()
        .take(MAX_TOOLS)
        .map(|(tool, count)| (tool.clone(), *count))
        .collect();
    Ok(())
}

fn count_by(items: impl Iterator<Item = String>) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}

/// Distinct non-empty items, most frequent first, ties alphabetical.
fn ranked_by_count(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for item in items.filter(|i| !i.is_empty()) {
        *counts.entry(item).or_insert(0) += 1;
    }
    let mut ranked: Vec<(String, i64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().map(|(item, _)| item).collect()
}

/// Secrets redacted, then distinct in first-seen order. Redacting first
/// keeps a cut token fragment from ever reaching the card.
fn scrubbed_distinct(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|item| scrub::scrub_secrets(item))
        .filter(|item| !item.is_empty() && seen.insert(item.clone()))
        .collect()
}

fn capped(items: Vec<String>, max_items: usize, max_chars: usize) -> Vec<String> {
    items
        .into_iter()
        .take(max_items)
        .map(|item| truncate(&item, max_chars))
        .collect()
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
#[path = "block_digest_test.rs"]
mod tests;
