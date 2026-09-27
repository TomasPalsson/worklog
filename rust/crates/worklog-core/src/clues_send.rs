//! The only producer of anything sent off-machine (spec 006, D-02).

use crate::billing;
use crate::block_details::{self, DetailRow};
use crate::clues_contract::{
    BillingLineKey, DescriptionInput, RawRecord, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_TOOL,
};
use crate::repo;
use crate::routing_contract::{SOURCE_FIREFOX, SOURCE_SLACK};
use crate::scrub;
use anyhow::{anyhow, Result};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::OnceLock;
const SOURCE_CLAUDE_WORK: &str = "claude_work";
const SOURCE_GIT_REFLOG: &str = "git_reflog";
const SOURCE_SHELL: &str = "shell";
const SOURCE_GITHUB_COMMIT: &str = "github_commit";
const SOURCE_GITHUB_PR: &str = "github_pr";
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
    let mut collected = Collected::default();
    for row in &rows {
        collected.absorb(conn, row);
    }

    let own_key = block.jira_issue.as_deref().filter(|s| !s.is_empty());
    let candidate_ticket_titles = ticket_titles(conn, own_key, &rows);
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
    })
}

/// A billing line's `DescriptionInput`: merges every folded-in block.
pub fn build_line_input(conn: &Connection, key: &BillingLineKey) -> Result<DescriptionInput> {
    let rows = billing::rows_for_day(conn, &key.day)?;
    let matching: Vec<&billing::BillingRow> =
        rows.iter().filter(|r| row_matches_key(r, key)).collect();
    if matching.is_empty() {
        let where_ = format!("{}/{}/{}", key.day, key.folder, key.customer);
        return Err(anyhow!("no billing line for {where_}"));
    }
    let (block_ids, total_seconds) = union_block_ids(&matching);

    let mut merged = Collected::default();
    let mut candidate_ticket_titles = Vec::new();
    let mut block_descriptions = Vec::new();
    let mut jira_keys = Vec::new();
    for &id in &block_ids {
        let input = build_block_input(conn, id)?;
        merged.branches.extend(input.branches);
        merged.change_titles.extend(input.change_titles);
        merged.file_basenames.extend(input.file_basenames);
        merged.programs.extend(input.programs);
        merged.web_domains.extend(input.web_domains);
        merged.slack_channels.extend(input.slack_channels);
        candidate_ticket_titles.extend(input.candidate_ticket_titles);
        jira_keys.push(input.jira_key);
        let desc = repo::get_block(conn, id)?.and_then(|b| b.description);
        if let Some(desc) = desc.filter(|d| !d.trim().is_empty()) {
            block_descriptions.push(desc);
        }
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

/// Raw (unscrubbed, uncapped) clue accumulator for one block's rows.
#[derive(Default)]
struct Collected {
    branches: Vec<String>,
    change_titles: Vec<String>,
    file_basenames: Vec<String>,
    programs: Vec<String>,
    web_domains: Vec<String>,
    slack_channels: Vec<String>,
}

impl Collected {
    fn absorb(&mut self, conn: &Connection, row: &DetailRow) {
        match row.source.as_str() {
            SOURCE_CLAUDE_TOOL => self.absorb_claude_tool(row),
            SOURCE_CLAUDE_WORK => self.absorb_claude_work(row),
            SOURCE_CLAUDE_HELPER => self.absorb_claude_helper(row),
            SOURCE_GIT_REFLOG => self.absorb_reflog(row),
            SOURCE_SHELL => self.absorb_shell(row),
            SOURCE_FIREFOX => self.absorb_firefox(row),
            SOURCE_SLACK => self.absorb_slack(conn, row),
            SOURCE_GITHUB_COMMIT | SOURCE_GITHUB_PR => self.absorb_change_title(row),
            _ => {}
        }
    }

    fn absorb_claude_tool(&mut self, row: &DetailRow) {
        if let Some(RawRecord::ClaudeTool { files, .. }) = &row.raw {
            for f in files {
                self.file_basenames.push(basename(f));
            }
        }
    }

    fn absorb_claude_work(&mut self, row: &DetailRow) {
        let Some(details) = &row.details else { return };
        if let Some(b) = branch_from_summary(details) {
            self.branches.push(b);
        }
        self.file_basenames.extend(edited_basenames(details));
    }

    fn absorb_claude_helper(&mut self, row: &DetailRow) {
        if let Some(RawRecord::Helper { summary, .. }) = &row.raw {
            if let Some(b) = branch_from_summary(summary) {
                self.branches.push(b);
            }
        }
    }

    fn absorb_reflog(&mut self, row: &DetailRow) {
        let Some(RawRecord::Reflog { message }) = &row.raw else {
            return;
        };
        if let Some(b) = reflog_branch(message) {
            self.branches.push(b);
        }
        if let Some(subject) = reflog_commit_subject(message) {
            if let Some(t) = strip_pr_ticket(&subject) {
                self.change_titles.push(t);
            }
        }
    }

    fn absorb_shell(&mut self, row: &DetailRow) {
        if !is_env_assignment(&row.title) {
            self.programs.push(row.title.clone());
        }
    }

    fn absorb_firefox(&mut self, row: &DetailRow) {
        if let Some(details) = &row.details {
            if let Some(host) = url_host(details) {
                self.web_domains.push(host);
            }
        }
    }

    fn absorb_slack(&mut self, conn: &Connection, row: &DetailRow) {
        if slack_channel_id(conn, row.id) && !row.title.starts_with("mpdm-") {
            self.slack_channels
                .push(row.title.trim_start_matches('#').to_string());
        }
    }

    fn absorb_change_title(&mut self, row: &DetailRow) {
        if let Some(t) = strip_pr_ticket(&row.title) {
            self.change_titles.push(t);
        }
    }
}

/// `jira_tickets.summary` for `own_key` + every Jira key in `rows`' titles.
fn ticket_titles(conn: &Connection, own_key: Option<&str>, rows: &[DetailRow]) -> Vec<String> {
    let mut keys: Vec<String> = own_key.map(str::to_string).into_iter().collect();
    for row in rows {
        keys.extend(
            jira_key_re()
                .find_iter(&row.title)
                .map(|m| m.as_str().to_string()),
        );
    }
    let mut seen = HashSet::new();
    let mut titles = Vec::new();
    for key in keys {
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Ok(summary) = conn.query_row(
            "SELECT summary FROM jira_tickets WHERE key = ?1",
            [&key],
            |r| r.get::<_, String>(0),
        ) {
            if !summary.is_empty() {
                titles.push(summary);
            }
        }
    }
    titles
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

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// `true` for a channel/group event (`source_id` `"C…:ts"`/`"G…:ts"`);
/// `false` for a DM (`"D…:ts"`) or else. `G` group DMs (`mpdm-…`) name people.
fn slack_channel_id(conn: &Connection, event_id: i64) -> bool {
    let sql = "SELECT source_id FROM events WHERE id = ?1";
    let id: String = conn
        .query_row(sql, [event_id], |r| r.get(0))
        .unwrap_or_default();
    matches!(id.chars().next(), Some('C') | Some('G'))
}

/// The value right after `"branch "` in a `WorkMinute::summary` string.
fn branch_from_summary(summary: &str) -> Option<String> {
    summary
        .split(" · ")
        .find_map(|part| part.strip_prefix("branch "))
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string)
}

/// Basenames out of a `WorkMinute::summary` string's `"edited a, b +N"`.
fn edited_basenames(summary: &str) -> Vec<String> {
    for part in summary.split(" · ") {
        let Some(rest) = part.strip_prefix("edited ") else {
            continue;
        };
        let files_part = match rest.rfind(" +") {
            Some(idx) if rest[idx + 2..].chars().all(|c| c.is_ascii_digit()) => &rest[..idx],
            _ => rest,
        };
        return files_part
            .split(", ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(basename)
            .collect();
    }
    Vec::new()
}

/// Target branch `B` out of a raw `"checkout: moving from A to B"`.
fn reflog_branch(message: &str) -> Option<String> {
    let rest = message.strip_prefix("checkout:")?;
    let idx = rest.rfind(" to ")?;
    let branch = rest[idx + 4..].trim();
    (!branch.is_empty()).then(|| branch.to_string())
}

/// Subject out of a raw `"commit: <subject>"` / `"commit (amend): <s>"`.
fn reflog_commit_subject(message: &str) -> Option<String> {
    let rest = message
        .strip_prefix("commit (amend):")
        .or_else(|| message.strip_prefix("commit:"))?;
    let subject = rest.trim();
    (!subject.is_empty()).then(|| subject.to_string())
}

/// `true` for a bare `NAME=value` token (defence in depth).
fn is_env_assignment(title: &str) -> bool {
    let mut chars = title.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    let mut saw_eq = false;
    for c in chars {
        if c == '=' {
            saw_eq = true;
            break;
        }
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    saw_eq
}

/// The host of an `http(s)` URL, no path/query/port; `None` otherwise.
fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host_port = &rest[..end];
    let host_port = host_port.rsplit('@').next().unwrap_or(host_port);
    let host = host_port.split(':').next().unwrap_or(host_port);
    (!host.is_empty()).then(|| host.to_string())
}

fn jira_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z][A-Z0-9]{1,9}-\d+\b").unwrap())
}
fn pr_prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^PR #\d+:\s*").unwrap())
}
fn whitespace_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}
fn ticket_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"#\d+|\b[A-Z][A-Z0-9]{1,9}-\d+\b").unwrap())
}

/// Title with `"PR #123: "`, `"#123"` and every Jira key stripped (D-13).
fn strip_pr_ticket(title: &str) -> Option<String> {
    let s = pr_prefix_re().replace(title, "");
    let s = ticket_number_re().replace_all(&s, "");
    let s = whitespace_re().replace_all(s.trim(), " ");
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[path = "clues_send_test.rs"]
#[cfg(test)]
mod tests;
