//! The only producer of anything sent off-machine (spec 006, D-02).

use crate::billing;
use crate::block_details;
use crate::clues_collect;
use crate::clues_contract::{BillingLineKey, DescriptionInput};
use crate::clues_work_items::{self, BlockClue};
use crate::repo;
use crate::scrub;
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::collections::HashSet;

const MAX_STRING_CHARS: usize = 200; // longest string in a DescriptionInput
const MAX_LIST_ENTRIES: usize = 30; // longest list in a DescriptionInput

/// A block's `DescriptionInput`. `Err` for a personal block.
pub fn build_block_input(conn: &Connection, block_id: i64) -> Result<DescriptionInput> {
    let block =
        repo::get_block(conn, block_id)?.ok_or_else(|| anyhow!("block {block_id} not found"))?;
    if block.is_personal {
        return Err(anyhow!("block {block_id} is personal"));
    }

    let rows = block_details::details_for_block(conn, block_id)?;
    let collected = clues_collect::collect(conn, &rows);

    let own_key = block.jira_issue.as_deref().filter(|s| !s.is_empty());
    let candidate_ticket_titles = clues_collect::ticket_titles(conn, own_key, &rows);
    let folder = billing::work_folder_for_block(conn, block_id)?;

    Ok(DescriptionInput {
        day: scrub_str(&block.day),
        minutes: block.duration_seconds / 60,
        folder: folder.map(|f| scrub_str(&f)),
        branches: finalize_list(collected.branches),
        change_titles: finalize_list(collected.change_titles),
        jira_key: own_key.map(scrub_str),
        candidate_ticket_titles: finalize_list(candidate_ticket_titles),
        file_basenames: finalize_list(collected.file_basenames),
        programs: finalize_list(collected.programs),
        web_domains: finalize_list(collected.web_domains),
        slack_channels: finalize_list(collected.slack_channels),
        block_descriptions: Vec::new(),
        work_items: Vec::new(),
    })
}

/// A billing line's `DescriptionInput`: merges every folded-in block's
/// clues into the flat lists (for the per-block estimator's own reuse of
/// this shape) AND groups them BY TASK into `work_items` — sharing a
/// Jira key, else a branch, else the same description — so the
/// line-text model can tell which clue belongs to which piece of work
/// instead of gluing unrelated work into one sentence.
pub fn build_line_input(conn: &Connection, key: &BillingLineKey) -> Result<DescriptionInput> {
    let rows = billing::rows_for_day(conn, &key.day)?;
    let matching: Vec<&billing::BillingRow> =
        rows.iter().filter(|r| row_matches_key(r, key)).collect();
    if matching.is_empty() {
        let where_ = format!("{}/{}/{}", key.day, key.folder, key.customer);
        return Err(anyhow!("no billing line for {where_}"));
    }
    let (block_ids, total_seconds) = union_block_ids(&matching);

    let mut merged = clues_collect::Collected::default();
    let mut candidate_ticket_titles = Vec::new();
    let mut block_descriptions = Vec::new();
    let mut jira_keys = Vec::new();
    let mut block_clues = Vec::new();
    for &id in &block_ids {
        let input = build_block_input(conn, id)?;
        let desc = repo::get_block(conn, id)?
            .and_then(|b| b.description)
            .and_then(|d| (!d.trim().is_empty()).then(|| scrub_str(&d)));
        if let Some(desc) = &desc {
            block_descriptions.push(desc.clone());
        }
        block_clues.push(BlockClue {
            minutes: input.minutes,
            ticket: input.jira_key.clone(),
            branches: input.branches.clone(),
            change_titles: input.change_titles.clone(),
            file_basenames: input.file_basenames.clone(),
            description: desc,
        });
        merged.branches.extend(input.branches);
        merged.change_titles.extend(input.change_titles);
        merged.file_basenames.extend(input.file_basenames);
        merged.programs.extend(input.programs);
        merged.web_domains.extend(input.web_domains);
        merged.slack_channels.extend(input.slack_channels);
        candidate_ticket_titles.extend(input.candidate_ticket_titles);
        jira_keys.push(input.jira_key);
    }

    Ok(DescriptionInput {
        day: scrub_str(&key.day),
        minutes: total_seconds / 60,
        folder: Some(scrub_str(&key.folder)),
        branches: finalize_list(merged.branches),
        change_titles: finalize_list(merged.change_titles),
        jira_key: shared_value(&jira_keys),
        candidate_ticket_titles: finalize_list(candidate_ticket_titles),
        file_basenames: finalize_list(merged.file_basenames),
        programs: finalize_list(merged.programs),
        web_domains: finalize_list(merged.web_domains),
        slack_channels: finalize_list(merged.slack_channels),
        block_descriptions: finalize_list(block_descriptions),
        work_items: clues_work_items::group(block_clues),
    })
}

fn row_matches_key(row: &billing::BillingRow, key: &BillingLineKey) -> bool {
    row.folder == key.folder && row.customer.as_deref().unwrap_or("") == key.customer
}

/// Distinct block ids (first-seen) plus their overlap-safe `seconds` total.
fn union_block_ids(rows: &[&billing::BillingRow]) -> (Vec<i64>, i64) {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    let mut seconds = 0i64;
    for row in rows {
        seconds += row.seconds;
        for &id in &row.block_ids {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }
    (ids, seconds)
}

/// `Some(v)` only when every entry agrees (incl. "all `None`"), else `None`.
fn shared_value(values: &[Option<String>]) -> Option<String> {
    let first = values.first()?;
    values.iter().all(|v| v == first).then(|| first.clone())?
}

/// Dedupe (first-seen), cap the list and each scrubbed entry (D-02).
fn finalize_list(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let capped = scrub_str(&item);
        if capped.is_empty() || !seen.insert(capped.clone()) {
            continue;
        }
        out.push(capped);
        if out.len() == MAX_LIST_ENTRIES {
            break;
        }
    }
    out
}

/// `scrub::scrub_identifiers` then a hard cap at [`MAX_STRING_CHARS`].
fn scrub_str(s: &str) -> String {
    scrub::scrub_identifiers(s)
        .chars()
        .take(MAX_STRING_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[path = "clues_send_test.rs"]
#[cfg(test)]
mod tests;
