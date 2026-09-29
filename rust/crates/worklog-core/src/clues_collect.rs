//! Per-block clue collection out of a block's `DetailRow`s — split out of
//! `clues_send.rs` (which stays the only *producer* of a
//! `DescriptionInput`, D-02) purely to keep that file under the repo's
//! size guard. Every string here is raw (unscrubbed, uncapped);
//! `clues_send::scrub_str`/`finalize_list` do that work.

use crate::block_details::DetailRow;
use crate::clues_contract::{RawRecord, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_TOOL};
use crate::routing_contract::{SOURCE_FIREFOX, SOURCE_SLACK};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::OnceLock;

const SOURCE_CLAUDE_TURN: &str = "claude_turn";
const SOURCE_CLAUDE_WORK: &str = "claude_work";
const SOURCE_GIT_REFLOG: &str = "git_reflog";
const SOURCE_SHELL: &str = "shell";
const SOURCE_GITHUB_COMMIT: &str = "github_commit";
const SOURCE_GITHUB_PR: &str = "github_pr";

/// Raw (unscrubbed, uncapped) clue accumulator for one block's rows.
#[derive(Default)]
pub struct Collected {
    pub branches: Vec<String>,
    pub change_titles: Vec<String>,
    pub file_basenames: Vec<String>,
    pub programs: Vec<String>,
    pub web_domains: Vec<String>,
    pub slack_channels: Vec<String>,
    pub prompts: Vec<String>,
    pub tool_calls: Vec<String>,
    pub helper_work: Vec<String>,
    pub shell_commands: Vec<String>,
    pub commit_bodies: Vec<String>,
}

/// Collects every row's clues into one [`Collected`] accumulator.
pub fn collect(conn: &Connection, rows: &[DetailRow]) -> Collected {
    let mut collected = Collected::default();
    for row in rows {
        collected.absorb(conn, row);
    }
    collected
}

impl Collected {
    fn absorb(&mut self, conn: &Connection, row: &DetailRow) {
        match row.source.as_str() {
            SOURCE_CLAUDE_TURN => self.absorb_prompt(row),
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

    fn absorb_prompt(&mut self, row: &DetailRow) {
        if let Some(RawRecord::ClaudePrompt { text, .. }) = &row.raw {
            self.prompts.push(crate::estimate::redact_code(text));
        }
    }

    fn absorb_claude_tool(&mut self, row: &DetailRow) {
        if let Some(RawRecord::ClaudeTool {
            tool, input, files, ..
        }) = &row.raw
        {
            for f in files {
                self.file_basenames.push(basename(f));
            }
            self.tool_calls.push(format!("{tool} {input}"));
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
            self.file_basenames.extend(edited_basenames(summary));
            self.helper_work.push(format!("{} · {summary}", row.title));
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
        if let Some(RawRecord::Shell { command, .. }) = &row.raw {
            self.shell_commands.push(command.clone());
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
        if let Some(RawRecord::Commit { body, .. }) = &row.raw {
            self.commit_bodies.push(body.clone());
        }
    }
}

/// `jira_tickets.summary` for `own_key` + every Jira key in `rows`' titles.
pub fn ticket_titles(conn: &Connection, own_key: Option<&str>, rows: &[DetailRow]) -> Vec<String> {
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
