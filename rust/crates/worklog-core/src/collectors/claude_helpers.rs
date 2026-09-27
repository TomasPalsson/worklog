//! Subagent, sidechain, background-job and `agents` teammate activity, plus
//! inter-session messages, linked to their parent session by the
//! transcript's session/turn id — never by shared cwd (spec 006, FR-16,
//! FR-18). Adds no time to any block (FR-17).
use super::CollectReport;
use crate::clues_contract::{HelperKind, RawRecord, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_MESSAGE};
use crate::collectors::claude_transcripts::{line_key, Window};
use crate::collectors::fish::repo_root_for;
use crate::models::Event;
use crate::repo;
use crate::scrub;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What Claude did in one minute, as names and paths only.
pub(super) struct WorkMinute {
    pub(super) first: DateTime<Utc>,
    pub(super) project_path: Option<String>,
    branch: Option<String>,
    tools: std::collections::BTreeMap<String, u32>,
    edited: std::collections::BTreeSet<String>,
}

/// Tools whose `file_path` is a file Claude changed.
const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

impl WorkMinute {
    pub(super) fn new(first: DateTime<Utc>, project_path: Option<String>) -> Self {
        Self {
            first,
            project_path,
            branch: None,
            tools: Default::default(),
            edited: Default::default(),
        }
    }

    pub(super) fn add(&mut self, line: &Value) {
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
    pub(super) fn summary(&self) -> Option<String> {
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

/// A `<teammate-message teammate_id="X">…</teammate-message>` user line, or
/// a peer `queued_command` attachment line: `(sender, text)`. Never a
/// `claude_turn` prompt (FR-18).
pub(super) fn session_message(value: &Value) -> Option<(String, String)> {
    match value.get("type").and_then(Value::as_str)? {
        "user" => {
            let content = value.pointer("/message/content")?.as_str()?;
            if !content.contains("<teammate-message") {
                return None;
            }
            Some((
                extract_attr(content, "teammate_id")?,
                strip_tag(content, "teammate-message")?,
            ))
        }
        "attachment" => {
            let attachment = value.get("attachment")?;
            if attachment.get("type").and_then(Value::as_str) != Some("queued_command") {
                return None;
            }
            let origin = attachment.get("origin")?;
            if origin.get("kind").and_then(Value::as_str) != Some("peer") {
                return None;
            }
            Some((
                origin.get("from").and_then(Value::as_str)?.to_string(),
                attachment
                    .get("prompt")
                    .and_then(Value::as_str)?
                    .to_string(),
            ))
        }
        _ => None,
    }
}

/// The `attr="…"` value in an XML-ish tag.
fn extract_attr(s: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = s.find(&needle)? + needle.len();
    let end = start + s[start..].find('"')?;
    Some(s[start..end].to_string())
}

/// The text between a tag's `>` and its matching `</tag>`.
fn strip_tag(s: &str, tag: &str) -> Option<String> {
    let open_end = s.find('>')? + 1;
    let close_start = s.rfind(&format!("</{tag}>"))?;
    (open_end <= close_start).then(|| s[open_end..close_start].to_string())
}

pub(super) fn emit_message_event(
    conn: &Connection,
    session_id: &str,
    uuid: &str,
    ts_utc: DateTime<Utc>,
    project_path: Option<String>,
    (sender, text): (String, String),
    report: &mut CollectReport,
) -> Result<()> {
    let raw = RawRecord::SessionMessage {
        from: sender.clone(),
        text: scrub::scrub_secrets(&text),
    };
    let ev = Event {
        project_path,
        session_id: Some(session_id.to_string()),
        raw_json: serde_json::to_string(&raw).ok(),
        ..Event::minimal(
            SOURCE_CLAUDE_MESSAGE,
            format!("{session_id}:{uuid}"),
            ts_utc.to_rfc3339(),
            format!("message from {sender}"),
        )
    };
    repo::upsert_event(conn, &ev)?;
    report.events_written += 1;
    Ok(())
}

pub(super) fn emit_helper_minute(
    conn: &Connection,
    source_id: String,
    parent_session_id: String,
    helper_kind: HelperKind,
    title: String,
    w: &WorkMinute,
    report: &mut CollectReport,
) -> Result<()> {
    let raw = RawRecord::Helper {
        parent_session_id: parent_session_id.clone(),
        helper_kind,
        summary: scrub::scrub_secrets(&w.summary().unwrap_or_default()),
    };
    let ev = Event {
        project_path: w.project_path.clone(),
        session_id: Some(parent_session_id),
        raw_json: serde_json::to_string(&raw).ok(),
        ..Event::minimal(SOURCE_CLAUDE_HELPER, source_id, w.first.to_rfc3339(), title)
    };
    repo::upsert_event(conn, &ev)?;
    report.events_written += 1;
    Ok(())
}

/// `agentType`/`description`/`taskKind` from an `agent-<id>.meta.json`
/// sibling; missing or unreadable meta falls back to a bare "subagent".
struct AgentMeta {
    helper_kind: HelperKind,
    title: String,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAgentMeta {
    agent_type: Option<String>,
    description: Option<String>,
    task_kind: Option<String>,
}

fn read_agent_meta(jsonl_path: &Path) -> AgentMeta {
    let meta_path = jsonl_path.with_extension("meta.json");
    let raw: RawAgentMeta = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    let title = match (raw.agent_type, raw.description) {
        (Some(t), Some(d)) => format!("{t}: {d}"),
        _ => "subagent".to_string(),
    };
    let helper_kind = if raw.task_kind.as_deref() == Some("in_process_teammate") {
        HelperKind::Teammate
    } else {
        HelperKind::Subagent
    };
    AgentMeta { helper_kind, title }
}

/// Every `agent-*.jsonl` directly in `subagents_dir` and under
/// `subagents_dir/workflows/*/` (never `journal.jsonl`).
pub(super) fn collect_helpers_for_session(
    conn: &Connection,
    subagents_dir: &Path,
    win: &Window,
    seen: &mut HashSet<String>,
    report: &mut CollectReport,
) -> Result<()> {
    for path in agent_files_in(subagents_dir) {
        process_agent_file(conn, &path, win, seen, report)?;
    }
    let Ok(workflow_dirs) = std::fs::read_dir(subagents_dir.join("workflows")) else {
        return Ok(());
    };
    for entry in workflow_dirs.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        for path in agent_files_in(&entry.path()) {
            process_agent_file(conn, &path, win, seen, report)?;
        }
    }
    Ok(())
}

fn agent_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("agent-") && n.ends_with(".jsonl"))
        })
        .collect()
}

fn process_agent_file(
    conn: &Connection,
    path: &Path,
    win: &Window,
    seen: &mut HashSet<String>,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    let Ok(modified) = meta.modified() else {
        return Ok(());
    };
    let modified_ts = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Only files untouched since before the window can be skipped: an
    // agent still writing after `until` holds lines from inside it.
    if modified_ts < win.since_ts {
        return Ok(());
    }
    let agent_meta = read_agent_meta(path);
    collect_agent_file(conn, path, &agent_meta, win, seen, report)
}

fn collect_agent_file(
    conn: &Connection,
    path: &Path,
    meta: &AgentMeta,
    win: &Window,
    seen: &mut HashSet<String>,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    // One marker per (agentId, minute): the PARENT session id, read off the
    // line itself, never inferred from cwd (FR-16).
    let mut working: std::collections::BTreeMap<(String, i64), (String, WorkMinute)> =
        Default::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
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

        if let Some(msg) = session_message(&value) {
            emit_message_event(
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
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(agent_id) = value.get("agentId").and_then(Value::as_str) else {
            continue;
        };
        working
            .entry((agent_id.to_string(), key.ts_utc.timestamp() / 60))
            .or_insert_with(|| {
                (
                    key.session_id.to_string(),
                    WorkMinute::new(key.ts_utc, project_path.clone()),
                )
            })
            .1
            .add(&value);
    }

    for ((agent_id, minute), (parent_session_id, w)) in working {
        emit_helper_minute(
            conn,
            format!("{agent_id}:m{minute}"),
            parent_session_id,
            meta.helper_kind,
            meta.title.clone(),
            &w,
            report,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "claude_helpers_test.rs"]
mod tests;
