//! Claude transcript tool-call capture: builds `clues_contract::
//! RawRecord::ClaudeTool` from a transcript tool-use turn, applying
//! `scrub::scrub_secrets` and the `clues_contract::TOOL_OUTPUT_CAP_BYTES`
//! output cap (spec 006, FR-14, FR-15). Populated by T010.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::clues_contract::{RawRecord, SOURCE_CLAUDE_TOOL, TOOL_OUTPUT_CAP_BYTES};
use crate::models::Event;
use crate::scrub;

/// `tool_use` id -> its `tool_result`'s raw (unscrubbed) output text,
/// scanned from every "user" line in one transcript file.
pub fn collect_tool_outputs(content: &str) -> HashMap<String, String> {
    let mut outputs = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(items) = value.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            if item.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = item.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(text) = tool_result_text(item.get("content")) {
                outputs.insert(id.to_string(), text);
            }
        }
    }
    outputs
}

/// A `tool_result` block's own `content`: a plain string, or an array of
/// items whose `text` fields are joined.
fn tool_result_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let joined: Vec<&str> = items
                .iter()
                .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|i| i.get("text").and_then(Value::as_str))
                .collect();
            (!joined.is_empty()).then(|| joined.join("\n"))
        }
        _ => None,
    }
}

/// One `Event` per `tool_use` item on an already-filtered `claude_work`
/// assistant line — the tool-call detail behind the per-minute summary
/// (FR-14, FR-15).
pub fn build_tool_events(
    line: &Value,
    session_id: &str,
    started_at: DateTime<Utc>,
    project_path: Option<String>,
    outputs: &HashMap<String, String>,
) -> Vec<Event> {
    let Some(items) = line.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|item| {
            build_tool_event(item, session_id, started_at, project_path.clone(), outputs)
        })
        .collect()
}

fn build_tool_event(
    item: &Value,
    session_id: &str,
    started_at: DateTime<Utc>,
    project_path: Option<String>,
    outputs: &HashMap<String, String>,
) -> Option<Event> {
    let id = item.get("id").and_then(Value::as_str)?;
    let tool = item.get("name").and_then(Value::as_str)?.to_string();
    let input = scrub::scrub_json(item.get("input").unwrap_or(&Value::Null));
    let files = files_named_in(&input);

    let (output, output_cut_bytes) = match outputs.get(id) {
        Some(raw) => {
            let scrubbed = scrub::scrub_secrets(raw);
            let (kept, cut) = cap_bytes(&scrubbed, TOOL_OUTPUT_CAP_BYTES);
            (Some(kept), cut)
        }
        None => (None, 0),
    };

    let raw = RawRecord::ClaudeTool {
        session_id: session_id.to_string(),
        tool: tool.clone(),
        input,
        output,
        output_cut_bytes,
        files,
    };

    Some(Event {
        id: None,
        source: SOURCE_CLAUDE_TOOL.into(),
        source_id: format!("{session_id}:{id}"),
        started_at: started_at.to_rfc3339(),
        ended_at: None,
        duration_seconds: None,
        title: tool,
        details: None,
        repo: None,
        project_path,
        jira_issue: None,
        session_id: Some(session_id.to_string()),
        tempo_worklog_id: None,
        raw_json: serde_json::to_string(&raw).ok(),
    })
}

/// File paths named in a tool's (already-scrubbed) input, deduped.
fn files_named_in(input: &Value) -> Vec<String> {
    let mut files = Vec::new();
    for key in ["file_path", "notebook_path", "path"] {
        if let Some(p) = input.get(key).and_then(Value::as_str) {
            if !files.iter().any(|f: &String| f == p) {
                files.push(p.to_string());
            }
        }
    }
    files
}

/// Slice `s` to at most `max_bytes` bytes on a char boundary. Returns the
/// kept string and the number of bytes cut (0 when nothing was cut).
fn cap_bytes(s: &str, max_bytes: usize) -> (String, usize) {
    if s.len() <= max_bytes {
        return (s.to_string(), 0);
    }
    let mut cut = max_bytes;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    (s[..cut].to_string(), s.len() - cut)
}
