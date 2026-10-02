//! The only producer of anything sent off-machine (spec 006, D-02).

use crate::billing;
use crate::block_details;
use crate::clues_collect;
use crate::clues_contract::{BillingLineKey, DescriptionInput};
use crate::clues_work_items::{self, BlockClue};
use crate::repo;
use crate::scrub;
use crate::tempo_line_contract::TempoLineKey;
use crate::tempo_lines;
use anyhow::{anyhow, Result};
use regex::Regex;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const MAX_STRING_CHARS: usize = 200; // longest string in a DescriptionInput
const MAX_LIST_ENTRIES: usize = 30; // longest list in a DescriptionInput

// (chars per entry, entries) for the D-02-amended evidence lists. Worst
// case ≈ 60k + 30k + 12k + 8k + 10k chars ≈ 30k tokens ≈ $0.004 a call
// at $0.12/M input; a typical block is a small fraction of that.
const PROMPTS_CAP: (usize, usize) = (1500, 40);
const TOOL_CALLS_CAP: (usize, usize) = (300, 100);
const HELPER_WORK_CAP: (usize, usize) = (200, 60);
const SHELL_COMMANDS_CAP: (usize, usize) = (200, 40);
const COMMIT_BODIES_CAP: (usize, usize) = (500, 20);

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
        prompts: finalize_capped(collected.prompts, PROMPTS_CAP),
        tool_calls: finalize_capped(collected.tool_calls, TOOL_CALLS_CAP),
        helper_work: finalize_capped(collected.helper_work, HELPER_WORK_CAP),
        shell_commands: finalize_capped(collected.shell_commands, SHELL_COMMANDS_CAP),
        commit_bodies: finalize_capped(collected.commit_bodies, COMMIT_BODIES_CAP),
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
    build_input_for_blocks(conn, &key.day, Some(&key.folder), &block_ids, total_seconds)
}

/// A ticket line's `DescriptionInput`: the day's blocks on `key.jira_issue`
/// (same set `tempo_lines` shows), folder = their most common work folder.
pub fn build_ticket_line_input(conn: &Connection, key: &TempoLineKey) -> Result<DescriptionInput> {
    let blocks = tempo_lines::blocks_for_ticket(conn, key)?;
    if blocks.is_empty() {
        return Err(anyhow!("no blocks for {} on {}", key.jira_issue, key.day));
    }
    let total = billing::union_seconds(blocks.iter().map(billing::block_interval).collect());
    let ids: Vec<i64> = blocks.iter().map(|b| b.id).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for &id in &ids {
        if let Some(folder) = billing::work_folder_for_block(conn, id)? {
            *counts.entry(folder).or_default() += 1;
        }
    }
    // Ties break alphabetically so the folder is deterministic.
    let folder = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(f, _)| f);
    build_input_for_blocks(conn, &key.day, folder.as_deref(), &ids, total)
}

fn build_input_for_blocks(
    conn: &Connection,
    day: &str,
    folder: Option<&str>,
    block_ids: &[i64],
    total_seconds: i64,
) -> Result<DescriptionInput> {
    let mut merged = clues_collect::Collected::default();
    let mut candidate_ticket_titles = Vec::new();
    let mut block_descriptions = Vec::new();
    let mut jira_keys = Vec::new();
    let mut block_clues = Vec::new();
    for &id in block_ids {
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
        candidate_ticket_titles.extend(input.candidate_ticket_titles.clone());
        jira_keys.push(input.jira_key.clone());
        merge_block(&mut merged, input);
    }

    Ok(DescriptionInput {
        day: scrub_str(day),
        minutes: total_seconds / 60,
        folder: folder.map(scrub_str),
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
        prompts: finalize_capped(merged.prompts, PROMPTS_CAP),
        tool_calls: finalize_capped(merged.tool_calls, TOOL_CALLS_CAP),
        helper_work: finalize_capped(merged.helper_work, HELPER_WORK_CAP),
        shell_commands: finalize_capped(merged.shell_commands, SHELL_COMMANDS_CAP),
        commit_bodies: finalize_capped(merged.commit_bodies, COMMIT_BODIES_CAP),
    })
}

/// Folds one block's lists into a line's accumulator.
fn merge_block(merged: &mut clues_collect::Collected, input: DescriptionInput) {
    merged.branches.extend(input.branches);
    merged.change_titles.extend(input.change_titles);
    merged.file_basenames.extend(input.file_basenames);
    merged.programs.extend(input.programs);
    merged.web_domains.extend(input.web_domains);
    merged.slack_channels.extend(input.slack_channels);
    merged.prompts.extend(input.prompts);
    merged.tool_calls.extend(input.tool_calls);
    merged.helper_work.extend(input.helper_work);
    merged.shell_commands.extend(input.shell_commands);
    merged.commit_bodies.extend(input.commit_bodies);
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
    finalize_capped(items, (MAX_STRING_CHARS, MAX_LIST_ENTRIES))
}

/// [`finalize_list`] with its own `(chars per entry, entries)` caps.
fn finalize_capped(items: Vec<String>, (chars, entries): (usize, usize)) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let capped = scrub_capped(&item, chars);
        if capped.is_empty() || !seen.insert(capped.clone()) {
            continue;
        }
        out.push(capped);
        if out.len() == entries {
            break;
        }
    }
    out
}

/// `scrub::scrub_identifiers` then a hard cap at [`MAX_STRING_CHARS`].
fn scrub_str(s: &str) -> String {
    scrub_capped(s, MAX_STRING_CHARS)
}

/// `scrub::scrub_identifiers`, home dirs shortened to `~/` (no user
/// names off-machine), then a hard cap at `chars`.
fn scrub_capped(s: &str, chars: usize) -> String {
    let scrubbed = scrub::scrub_identifiers(s);
    home_dir_re()
        .replace_all(&scrubbed, "~/")
        .chars()
        .take(chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn home_dir_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"/(?:Users|home)/[^/\s"']+/"#).expect("valid regex"))
}

#[path = "clues_send_test.rs"]
#[cfg(test)]
mod tests;
