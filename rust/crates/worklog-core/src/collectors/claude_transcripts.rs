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
//! PRIVACY: prompt text, tool inputs and tool outputs (capped to 2 KB) are
//! now stored locally in `raw_json`, secret-scrubbed via `scrub::
//! scrub_secrets`/`scrub::scrub_json` before they ever reach the row
//! (D-03/D-04). `title` is always the literal `"prompt"`; `details` is
//! always `None`. A "claude working" minute still stores only names and
//! paths in `details`: the git branch, the tool names used and the files
//! it edited — never a command, a reply or any other text.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use crate::clues_contract::{HelperKind, RawRecord};
use crate::collectors::claude_helpers::{self, WorkMinute};
use crate::collectors::claude_tools;
use crate::collectors::fish::repo_root_for;
use crate::models::Event;
use crate::repo;
use crate::scrub;

use super::CollectReport;

/// One transcript line's identity: when it happened and which session/turn
/// it belongs to. `None` when the line is outside `[since_ts, until_ts)` or
/// missing a timestamp, session id or uuid.
pub(super) struct LineKey<'a> {
    pub(super) ts_utc: DateTime<Utc>,
    pub(super) session_id: &'a str,
    pub(super) uuid: &'a str,
}

pub(super) fn line_key<'a>(value: &'a Value, since_ts: i64, until_ts: i64) -> Option<LineKey<'a>> {
    let timestamp = value.get("timestamp").and_then(Value::as_str)?;
    let parsed = DateTime::parse_from_rfc3339(timestamp).ok()?;
    let ts_utc: DateTime<Utc> = parsed.with_timezone(&Utc);
    let epoch = ts_utc.timestamp();
    if epoch < since_ts || epoch >= until_ts {
        return None;
    }
    Some(LineKey {
        ts_utc,
        session_id: value.get("sessionId").and_then(Value::as_str)?,
        uuid: value.get("uuid").and_then(Value::as_str)?,
    })
}

/// The collection window and the owner's home dir, threaded through every
/// file/line a collector run touches (keeps their signatures under
/// clippy's `too_many_arguments`).
pub(super) struct Window<'a> {
    pub(super) since_ts: i64,
    pub(super) until_ts: i64,
    pub(super) home: Option<&'a str>,
}

/// Session ids belonging to a background job (`<jobs_dir>/<id>/state.json`
/// -> `sessionId`): their Claude-busy minutes add no time (D-05).
fn background_job_sessions(jobs_dir: &Path) -> HashSet<String> {
    let mut sessions = HashSet::new();
    let Ok(entries) = std::fs::read_dir(jobs_dir) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path().join("state.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            sessions.insert(id.to_string());
        }
    }
    sessions
}

/// Collect from the default transcripts root (`~/.claude/projects`) and the
/// default background-job store (`~/.claude/jobs`). Missing directories are
/// not an error.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "claude_turn",
            ..Default::default()
        });
    };
    collect_from_dirs(
        conn,
        &home.join(".claude/projects"),
        &home.join(".claude/jobs"),
        since,
        until,
    )
}

/// Collect from a transcripts root, using the default background-job store.
pub fn collect_from_dir(
    conn: &Connection,
    dir: &Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let jobs_dir = dirs::home_dir().unwrap_or_default().join(".claude/jobs");
    collect_from_dirs(conn, dir, &jobs_dir, since, until)
}

/// Collect from a transcripts root and a background-job store (spec 006,
/// D-05): a job's own session gets its Claude-busy minutes recorded as
/// helper activity, never `claude_work`.
pub fn collect_from_dirs(
    conn: &Connection,
    dir: &Path,
    jobs_dir: &Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "claude_turn",
        ..Default::default()
    };
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    let job_sessions = background_job_sessions(jobs_dir);

    // A resumed session copies its history into a new file with the same
    // line uuids: count each line once across every file.
    let mut seen = std::collections::HashSet::new();
    let win = Window {
        since_ts: since.and_time(NaiveTime::MIN).and_utc().timestamp(),
        until_ts: until.and_time(NaiveTime::MIN).and_utc().timestamp(),
        home: home.as_deref(),
    };

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
            if modified_ts < win.since_ts {
                continue;
            }
            collect_file(conn, &path, &win, &mut seen, &job_sessions, &mut report)?;
            // A session's subagent/workflow-task transcripts live in a
            // sibling directory named after its own id (spec 006, FR-16).
            if let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) {
                let helper_dir = project_dir.join(session_id).join("subagents");
                if helper_dir.is_dir() {
                    claude_helpers::collect_helpers_for_session(
                        conn,
                        &helper_dir,
                        &win,
                        &mut seen,
                        &mut report,
                    )?;
                }
            }
        }
    }
    Ok(report)
}

fn collect_file(
    conn: &Connection,
    path: &Path,
    win: &Window,
    seen: &mut HashSet<String>,
    job_sessions: &HashSet<String>,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    // tool_use -> its tool_result's output text, paired across the whole
    // file (a resumed session's tool_result can be many lines later).
    let tool_outputs = claude_tools::collect_tool_outputs(&content);
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
        // An inter-session message (FR-18) is never a prompt, whatever type
        // its line carries.
        let msg = claude_helpers::session_message(&value);
        if !is_work && msg.is_none() {
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
        let Some(key) = line_key(&value, win.since_ts, win.until_ts) else {
            continue;
        };
        if !seen.insert(key.uuid.to_string()) {
            continue;
        }
        let project_path = value
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(|cwd| repo_root_for(cwd, win.home));

        if let Some(msg) = msg {
            claude_helpers::emit_message_event(
                conn,
                key.session_id,
                key.uuid,
                key.ts_utc,
                project_path,
                msg,
                report,
            )?;
            continue;
        }
        if is_work {
            working
                .entry((key.session_id.to_string(), key.ts_utc.timestamp() / 60))
                .or_insert_with(|| WorkMinute::new(key.ts_utc, project_path.clone()))
                .add(&value);
            for tool_event in claude_tools::build_tool_events(
                &value,
                key.session_id,
                key.ts_utc,
                project_path.clone(),
                &tool_outputs,
            ) {
                repo::upsert_event(conn, &tool_event)?;
                report.events_written += 1;
            }
            continue;
        }

        let raw = RawRecord::ClaudePrompt {
            session_id: key.session_id.to_string(),
            text: scrub::scrub_secrets(&prompt_text(value.get("message").unwrap_or(&Value::Null))),
        };
        let ev = Event {
            project_path,
            session_id: Some(key.session_id.to_string()),
            raw_json: serde_json::to_string(&raw).ok(),
            ..Event::minimal(
                "claude_turn",
                format!("{}:{}", key.session_id, key.uuid),
                key.ts_utc.to_rfc3339(),
                "prompt",
            )
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    // "claude_work" (Claude busy) is kept apart from "claude_turn" (the owner
    // typed) so attention can outweigh background activity.
    for ((session_id, minute), w) in working {
        if job_sessions.contains(&session_id) {
            claude_helpers::emit_helper_minute(
                conn,
                format!("{session_id}:m{minute}"),
                session_id,
                HelperKind::BackgroundJob,
                "claude working".to_string(),
                &w,
                report,
            )?;
            continue;
        }
        let ev = Event {
            details: w.summary(),
            project_path: w.project_path.clone(),
            session_id: Some(session_id.clone()),
            ..Event::minimal(
                "claude_work",
                format!("{session_id}:m{minute}"),
                w.first.to_rfc3339(),
                "claude working",
            )
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }
    Ok(())
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

/// The owner's prompt text: the plain string content, or the text items of
/// a content array joined with `\n`.
fn prompt_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Only lines the owner typed: never headless `claude -p` runs (the
/// estimator, loop children), meta lines, or queued system notifications.
/// Newer transcripts say so in `origin.kind`; older ones have no origin, so
/// fall back to "not a `<tag>` line".
pub(super) fn is_owner_typed(line: &Value) -> bool {
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
