//! Claude Code transcript collector.
//!
//! Claude Code keeps a per-session JSON-lines transcript at
//! `~/.claude/projects/<dir>/<sessionId>.jsonl` — one line per turn, each a
//! JSON object with `type` ("user"/"assistant"/...), `timestamp` (ISO
//! UTC), `cwd`, `sessionId`, `uuid`. This collector turns every real user
//! prompt (never a `tool_result` reply relayed back as a "user" line) into
//! one event, source `claude_turn`. Fixes the "afternoon is invisible"
//! defect: the `claude` hook only records session start/end, so a long
//! autonomous session with hundreds of turns could show as two events.
//!
//! PRIVACY: only the fact that a prompt happened is recorded — the prompt
//! text (`message.content`) is never read into an `Event` field. `title`
//! is always the literal `"prompt"`; `details` is always `None`. A
//! "claude working" minute stores only names and paths in `details`: the
//! git branch, the tool names used and the files it edited — never a
//! command, a reply or any other text.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

use crate::collectors::fish::repo_root_for;
use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Collect from the default transcripts root (`~/.claude/projects`).
/// Missing directory is not an error.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "claude_turn",
            ..Default::default()
        });
    };
    collect_from_dir(conn, &home.join(".claude/projects"), since, until)
}

pub fn collect_from_dir(
    conn: &Connection,
    dir: &Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "claude_turn",
        ..Default::default()
    };
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());

    // A resumed session copies its history into a new file with the same
    // line uuids: count each line once across every file.
    let mut seen = std::collections::HashSet::new();
    let since_ts = since.and_time(NaiveTime::MIN).and_utc().timestamp();
    let until_ts = until.and_time(NaiveTime::MIN).and_utc().timestamp();

    let Ok(project_dirs) = std::fs::read_dir(dir) else {
        return Ok(report);
    };
    for project_entry in project_dirs.flatten() {
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&project_dir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(meta) = file_entry.metadata() else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            let modified_ts = modified
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            // Only files untouched since before the window can be skipped: a
            // session still writing after `until` holds lines from inside it.
            if modified_ts < since_ts {
                continue;
            }
            collect_file(
                conn,
                &path,
                since_ts,
                until_ts,
                home.as_deref(),
                &mut seen,
                &mut report,
            )?;
        }
    }
    Ok(report)
}

fn collect_file(
    conn: &Connection,
    path: &Path,
    since_ts: i64,
    until_ts: i64,
    home: Option<&str>,
    seen: &mut std::collections::HashSet<String>,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    // One marker per session-minute, summarising every line in it.
    let mut working: std::collections::BTreeMap<(String, i64), WorkMinute> = Default::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Claude working in the owner's interactive session is time on that
        // project too; one marker per minute, never the reply text.
        let is_work = value.get("type").and_then(Value::as_str) == Some("assistant")
            && value.get("entrypoint").and_then(Value::as_str) != Some("sdk-cli")
            && value.get("isSidechain").and_then(Value::as_bool) != Some(true);
        if !is_work {
            if value.get("type").and_then(Value::as_str) != Some("user") {
                continue;
            }
            let Some(message) = value.get("message") else {
                continue;
            };
            if !is_owner_typed(&value) || !is_real_prompt(message) {
                continue;
            }
        }
        let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) else {
            continue;
        };
        let ts_utc: DateTime<Utc> = parsed.with_timezone(&Utc);
        let epoch = ts_utc.timestamp();
        if epoch < since_ts || epoch >= until_ts {
            continue;
        }
        let Some(session_id) = value.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        let Some(uuid) = value.get("uuid").and_then(Value::as_str) else {
            continue;
        };
        if !seen.insert(uuid.to_string()) {
            continue;
        }
        let project_path = value
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(|cwd| repo_root_for(cwd, home));
        if is_work {
            working
                .entry((session_id.to_string(), epoch / 60))
                .or_insert_with(|| WorkMinute::new(ts_utc, project_path.clone()))
                .add(&value);
            continue;
        }

        let ev = Event {
            id: None,
            source: "claude_turn".into(),
            source_id: format!("{session_id}:{uuid}"),
            started_at: ts_utc.to_rfc3339(),
            ended_at: None,
            duration_seconds: None,
            title: "prompt".into(),
            details: None,
            repo: None,
            project_path,
            jira_issue: None,
            session_id: Some(session_id.to_string()),
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    // "claude_work" (Claude busy) is kept apart from "claude_turn" (the owner
    // typed) so attention can outweigh background activity.
    for ((session_id, minute), w) in working {
        let ev = Event {
            id: None,
            source: "claude_work".into(),
            source_id: format!("{session_id}:m{minute}"),
            started_at: w.first.to_rfc3339(),
            ended_at: None,
            duration_seconds: None,
            title: "claude working".into(),
            details: w.summary(),
            repo: None,
            project_path: w.project_path.clone(),
            jira_issue: None,
            session_id: Some(session_id),
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }
    Ok(())
}

/// What Claude did in one minute, as names and paths only.
struct WorkMinute {
    first: DateTime<Utc>,
    project_path: Option<String>,
    branch: Option<String>,
    tools: std::collections::BTreeMap<String, u32>,
    edited: std::collections::BTreeSet<String>,
}

/// Tools whose `file_path` is a file Claude changed.
const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

impl WorkMinute {
    fn new(first: DateTime<Utc>, project_path: Option<String>) -> Self {
        Self {
            first,
            project_path,
            branch: None,
            tools: Default::default(),
            edited: Default::default(),
        }
    }

    fn add(&mut self, line: &Value) {
        if let Some(b) = line.get("gitBranch").and_then(Value::as_str) {
            if !b.is_empty() && b != "HEAD" {
                self.branch = Some(b.to_string());
            }
        }
        let content = line.pointer("/message/content").and_then(Value::as_array);
        for item in content.into_iter().flatten() {
            if item.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            *self.tools.entry(name.to_string()).or_default() += 1;
            if EDIT_TOOLS.contains(&name) {
                if let Some(p) = item.pointer("/input/file_path").and_then(Value::as_str) {
                    // Relative to the repo root, else to the session's folder.
                    let cwd = line.get("cwd").and_then(Value::as_str);
                    let rel = [self.project_path.as_deref(), cwd]
                        .into_iter()
                        .flatten()
                        .find_map(|r| p.strip_prefix(&format!("{r}/")).map(str::to_string));
                    self.edited.insert(
                        rel.unwrap_or_else(|| p.rsplit('/').next().unwrap_or(p).to_string()),
                    );
                }
            }
        }
    }

    /// "branch fix-login · Bash ×2, Edit · edited src/login.rs", or `None`
    /// when there is nothing but text (no branch, no tools).
    fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(b) = &self.branch {
            parts.push(format!("branch {b}"));
        }
        if !self.tools.is_empty() {
            let mut tools: Vec<(&String, &u32)> = self.tools.iter().collect();
            tools.sort_by(|a, b| b.1.cmp(a.1));
            let names: Vec<String> = tools
                .iter()
                .map(|(n, c)| {
                    if **c > 1 {
                        format!("{n} ×{c}")
                    } else {
                        n.to_string()
                    }
                })
                .collect();
            parts.push(names.join(", "));
        }
        if !self.edited.is_empty() {
            let files: Vec<&str> = self.edited.iter().map(String::as_str).collect();
            let shown = files.iter().take(3).copied().collect::<Vec<_>>().join(", ");
            let more = files.len().saturating_sub(3);
            parts.push(if more > 0 {
                format!("edited {shown} +{more}")
            } else {
                format!("edited {shown}")
            });
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// A "real" user prompt: plain string content, or a content array that
/// isn't made up entirely of `tool_result` blocks (a `tool_result`-only
/// array is Claude Code relaying a tool's output back as a synthetic
/// "user" turn, never something the owner typed).
fn is_real_prompt(message: &Value) -> bool {
    match message.get("content") {
        Some(Value::String(_)) => true,
        Some(Value::Array(items)) => {
            !items.is_empty()
                && !items
                    .iter()
                    .all(|item| item.get("type").and_then(Value::as_str) == Some("tool_result"))
        }
        _ => false,
    }
}

/// Only lines the owner typed: never headless `claude -p` runs (the
/// estimator, loop children), meta lines, or queued system notifications.
/// Newer transcripts say so in `origin.kind`; older ones have no origin, so
/// fall back to "not a `<tag>` line".
fn is_owner_typed(line: &Value) -> bool {
    if line.get("entrypoint").and_then(Value::as_str) == Some("sdk-cli")
        || line.get("isMeta").and_then(Value::as_bool) == Some(true)
    {
        return false;
    }
    match line
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
    {
        Some(kind) => kind == "human",
        None => !matches!(
            line.get("message").and_then(|m| m.get("content")),
            Some(Value::String(s)) if s.trim_start().starts_with('<')
        ),
    }
}

// Tests live in claude_transcripts_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "claude_transcripts_test.rs"]
mod tests;
