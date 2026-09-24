//! Fish shell history collector.
//!
//! Reads `~/.local/share/fish/fish_history` (format: repeating blocks of
//! `- cmd: <text>`, `  when: <epoch>`, optional `  paths:` + indented
//! path lines). PRIVACY: the command text is only used in-process to
//! track `cd` and pick a program name — it is never written to the DB.
//! `title` is the first whitespace token of the command; `details` is
//! always `None`.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime};
use rusqlite::Connection;

use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Collect from the default fish history location. Missing file is not
/// an error — most machines running this collector won't use fish.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "shell",
            ..Default::default()
        });
    };
    let path = home.join(".local/share/fish/fish_history");
    collect_from_path(conn, &path, since, until)
}

pub fn collect_from_path(
    conn: &Connection,
    path: &std::path::Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "shell",
        ..Default::default()
    };

    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(report);
    };

    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    let since_ts = since.and_time(NaiveTime::MIN).and_utc().timestamp();
    let until_ts = until.and_time(NaiveTime::MIN).and_utc().timestamp();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut entry_index: usize = 0;
    let mut cwd: Option<String> = None;

    while i < lines.len() {
        let Some(cmd) = lines[i].strip_prefix("- cmd: ") else {
            i += 1;
            continue;
        };
        let cmd = cmd.to_string();
        let idx = entry_index;
        entry_index += 1;
        i += 1;

        let mut when: Option<i64> = None;
        if let Some(w) = lines.get(i).and_then(|l| l.strip_prefix("  when: ")) {
            when = w.trim().parse::<i64>().ok();
            i += 1;
        }
        if lines.get(i).map(|l| l.trim_start()) == Some("paths:") {
            i += 1;
            while lines.get(i).is_some_and(|l| l.starts_with("    - ")) {
                i += 1;
            }
        }

        let Some(when) = when else { continue };

        let segments = split_command_segments(&cmd);
        let title = segments
            .first()
            .map(|s| program_name(s))
            .unwrap_or_else(|| "shell".to_string());
        for segment in &segments {
            let mut tokens = segment.splitn(2, char::is_whitespace);
            if tokens.next() == Some("cd") {
                let arg = parse_cd_target(tokens.next().unwrap_or(""));
                cwd = resolve_cwd(cwd.as_deref(), &arg, home.as_deref());
            }
        }

        if when < since_ts || when >= until_ts {
            continue;
        }

        let Some(started_at) = DateTime::from_timestamp(when, 0).map(|dt| dt.to_rfc3339()) else {
            continue;
        };
        let project_path = cwd
            .as_deref()
            .and_then(|c| repo_root_for(c, home.as_deref()));

        let ev = Event {
            id: None,
            source: "shell".into(),
            source_id: format!("{when}:{idx}"),
            started_at,
            ended_at: None,
            duration_seconds: None,
            title,
            details: None,
            repo: None,
            project_path,
            jira_issue: None,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    Ok(report)
}

/// Split a (possibly multi-line, `fish_history`-escaped) command entry
/// into its individual command segments, so each `cd` only ever tracks
/// the cwd for its own segment, never for text that follows a `&&`,
/// `||`, `;`, `|`, `&` or a line break (real or the `\n` escape).
fn split_command_segments(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut segments = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let two = (c, chars.get(i + 1).copied());
        let (advance, split) = match two {
            ('\\', Some('n')) => (2, true),
            ('\n', _) => (1, true),
            ('&', Some('&')) => (2, true),
            ('|', Some('|')) => (2, true),
            (';', _) | ('|', _) | ('&', _) => (1, true),
            _ => (1, false),
        };
        if split {
            segments.push(std::mem::take(&mut cur).trim().to_string());
        } else {
            cur.push(c);
        }
        i += advance;
    }
    segments.push(cur.trim().to_string());
    segments.retain(|s| !s.is_empty());
    segments
}

/// Wrapper words whose own leading flags/env-assignments are skipped
/// along with the wrapper itself, per the "title = program name only"
/// invariant.
const WRAPPER_WORDS: [&str; 8] = [
    "env", "sudo", "command", "builtin", "exec", "time", "nohup", "nice",
];

/// `true` if `tok` is a `NAME=value` env assignment (`NAME` matching
/// `[A-Za-z_][A-Za-z0-9_]*`).
fn is_env_assignment(tok: &str) -> bool {
    let mut chars = tok.chars();
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

/// Derive a sanitised program name for a command `segment`: never lets
/// an env value, argument, or path directory reach the result.
///
/// 1. Skip leading env assignments.
/// 2. Skip leading wrapper words (and their own flags/env assignments).
/// 3. Take the basename of what remains.
/// 4. Fall back to the literal `shell` unless the basename is 1-40
///    chars of `[A-Za-z0-9._+-]`.
fn program_name(segment: &str) -> String {
    let mut tokens = segment.split_whitespace().peekable();
    loop {
        let mut progressed = false;
        while tokens.peek().is_some_and(|t| is_env_assignment(t)) {
            tokens.next();
            progressed = true;
        }
        if tokens.peek().is_some_and(|t| WRAPPER_WORDS.contains(t)) {
            tokens.next();
            progressed = true;
            while tokens
                .peek()
                .is_some_and(|t| t.starts_with('-') || is_env_assignment(t))
            {
                tokens.next();
            }
        }
        if !progressed {
            break;
        }
    }

    let Some(prog) = tokens.next() else {
        return "shell".to_string();
    };
    let basename = prog.rsplit('/').next().unwrap_or(prog);
    let valid = !basename.is_empty()
        && basename.len() <= 40
        && basename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));

    if valid {
        basename.to_string()
    } else {
        "shell".to_string()
    }
}

/// Parse a `cd` argument, ending at the first whitespace or shell
/// operator, or (for a quoted target) the closing quote.
fn parse_cd_target(arg: &str) -> String {
    let arg = arg.trim_start();
    if let Some(q) = arg.chars().next().filter(|c| *c == '\'' || *c == '"') {
        return arg[1..].split(q).next().unwrap_or("").to_string();
    }
    arg.chars()
        .take_while(|c| !matches!(c, ' ' | '\t' | ';' | '&' | '|' | ')'))
        .collect()
}

/// Resolve a `cd` argument (absolute, `~`-relative, or relative to
/// `cwd`) into a new absolute cwd. `None` when relative and no base is
/// known yet.
fn resolve_cwd(cwd: Option<&str>, arg: &str, home: Option<&str>) -> Option<String> {
    let target = if arg.is_empty() {
        home.map(str::to_owned)
    } else if let Some(rest) = arg.strip_prefix('~') {
        home.map(|h| format!("{h}{rest}"))
    } else if let Some(rest) = arg.strip_prefix('/') {
        Some(format!("/{rest}"))
    } else {
        cwd.map(|base| format!("{base}/{arg}"))
    };
    target.map(|t| normalize_path(&t))
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The repo root under `~/Desktop/Work/<key>` or `~/Desktop/Projects/<key>`
/// for `path`, collapsing `/.claude/worktrees/*` scaffolding first
/// (mirrors `billing::work_folder_for_path`'s worktree collapse). Shared
/// with `collectors::claude_transcripts`, which derives the same kind of
/// repo root from a transcript's `cwd`.
pub(crate) fn repo_root_for(path: &str, home: Option<&str>) -> Option<String> {
    let home = home?;
    let base = match path.find("/.claude/") {
        Some(i) => &path[..i],
        None => path,
    };
    for root_name in ["Desktop/Work", "Desktop/Projects"] {
        let prefix = format!("{home}/{root_name}");
        let Some(rest) = base.strip_prefix(&prefix) else {
            continue;
        };
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            continue;
        }
        let first = rest.split('/').next().unwrap_or(rest);
        if !first.is_empty() {
            return Some(format!("{prefix}/{first}"));
        }
    }
    None
}

// Tests live in fish_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "fish_test.rs"]
mod tests;
