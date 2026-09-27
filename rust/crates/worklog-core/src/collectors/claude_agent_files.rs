//! `agent-*.jsonl` discovery and their `.meta.json` sidecar — split out of
//! `claude_helpers.rs` (file line budget) which is its only caller.

use crate::clues_contract::HelperKind;
use std::path::{Path, PathBuf};

/// `agentType`/`description`/`taskKind` from an `agent-<id>.meta.json`
/// sibling; missing or unreadable meta falls back to a bare "subagent".
pub(super) struct AgentMeta {
    pub(super) helper_kind: HelperKind,
    pub(super) title: String,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAgentMeta {
    agent_type: Option<String>,
    description: Option<String>,
    task_kind: Option<String>,
}

pub(super) fn read_agent_meta(jsonl_path: &Path) -> AgentMeta {
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

/// Every `agent-*.jsonl` directly in `dir` (never `journal.jsonl`).
pub(super) fn agent_files_in(dir: &Path) -> Vec<PathBuf> {
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
