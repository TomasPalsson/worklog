//! Shared names and shapes for block clues, attribution, the Details view
//! and billing-line texts (spec 006).
//!
//! Every task that touches spec 006 imports from here. A type or constant
//! a task needs that is missing here is an escalation to the orchestrator,
//! never a local re-declaration.

use serde::{Deserialize, Serialize};

/// Replacement for every detected secret, in storage (D-03) and in the
/// description request (D-02).
pub const SECRET_PLACEHOLDER: &str = "[secret]";
/// Max bytes of one tool output kept in `events.raw_json` (D-04).
pub const TOOL_OUTPUT_CAP_BYTES: usize = 2048;

/// `events.source` for helper activity: subagent, sidechain, background
/// job or `agents` teammate work (D-05). Never counted as block time.
pub const SOURCE_CLAUDE_HELPER: &str = "claude_helper";
/// `events.source` for a message one Claude session sent another (FR-18).
/// Never counted as block time.
pub const SOURCE_CLAUDE_MESSAGE: &str = "claude_message";

/// The one JSON shape stored in `events.raw_json` from spec 006 on.
/// Every string field has already passed `scrub::scrub_secrets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawRecord {
    Shell {
        command: String,
        cwd: Option<String>,
    },
    Reflog {
        message: String,
    },
    ClaudePrompt {
        session_id: String,
        text: String,
    },
    ClaudeTool {
        session_id: String,
        tool: String,
        input: serde_json::Value,
        output: Option<String>,
        /// Bytes cut from `output` by `TOOL_OUTPUT_CAP_BYTES`; 0 = whole.
        output_cut_bytes: usize,
        files: Vec<String>,
    },
    Helper {
        parent_session_id: String,
        helper_kind: HelperKind,
        summary: String,
    },
    SessionMessage {
        from: String,
        text: String,
    },
    Commit {
        sha: String,
        body: String,
        /// Local work folder whose clone holds `sha`; `None` = elsewhere.
        local_folder: Option<String>,
    },
    Hook {
        event: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperKind {
    Subagent,
    Sidechain,
    BackgroundJob,
    Teammate,
}

/// Everything the description writer may see (D-02). Built only by
/// `clues_send::build_block_input` / `build_line_input`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DescriptionInput {
    pub day: String,
    pub minutes: i64,
    pub folder: Option<String>,
    pub branches: Vec<String>,
    /// Commit and PR first lines with PR/ticket numbers removed (D-13).
    pub change_titles: Vec<String>,
    pub jira_key: Option<String>,
    pub candidate_ticket_titles: Vec<String>,
    pub file_basenames: Vec<String>,
    pub programs: Vec<String>,
    pub web_domains: Vec<String>,
    pub slack_channels: Vec<String>,
    /// Existing per-block descriptions feeding a billing-line text (D-12).
    pub block_descriptions: Vec<String>,
}

/// Primary key of one billing line — one invoice-form submission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BillingLineKey {
    pub day: String,
    pub folder: String,
    /// `""` when the customer is unresolved.
    pub customer: String,
}

/// `billing_line_texts.origin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineTextOrigin {
    Generated,
    /// Owner-edited; never overwritten by generation (FR-31).
    Manual,
}
