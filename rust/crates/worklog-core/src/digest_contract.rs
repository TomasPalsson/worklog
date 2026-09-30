//! Contract for spec 010 — the per-block card kept after raw events are
//! deleted. T001 copies this file verbatim to
//! `rust/crates/worklog-core/src/digest_contract.rs`; nobody else edits it.

use std::collections::BTreeMap;

/// Blocks whose `day` is strictly before `today - HORIZON_DAYS` are compressed.
pub const HORIZON_DAYS: i64 = 90;

/// Bumped only if the card shape changes; stored per row.
pub const DIGEST_VERSION: i64 = 1;

/// Error text every refusal on a compressed day uses (CLI + daemon).
pub const DAY_COMPRESSED: &str = "day is compressed";

/// Caps. Truncation is by `char`, after dedupe, keeping first-seen order
/// (events ordered by `started_at`).
pub const MAX_EVAL_TITLES: usize = 6;
pub const EVAL_TITLE_CHARS: usize = 80;
pub const MAX_PATHS: usize = 4;
pub const MAX_INVOICE_TITLES: usize = 5;
pub const INVOICE_TITLE_CHARS: usize = 120;
pub const MAX_BRANCHES: usize = 3;
pub const BRANCH_CHARS: usize = 60;
pub const MAX_CHANGE_TITLES: usize = 8;
pub const CHANGE_TITLE_CHARS: usize = 120;
pub const MAX_PROMPTS: usize = 5;
pub const PROMPT_MIN_CHARS: usize = 20;
pub const PROMPT_CHARS: usize = 200;
pub const MAX_FILES: usize = 15;
pub const MAX_TOOLS: usize = 8;
pub const FOLDER_CHARS: usize = 80;
pub const PROJECT_PATH_CHARS: usize = 200;
pub const JIRA_SUMMARY_CHARS: usize = 120;

/// Stored as JSON in `block_digest.json`. Every field has a serde default
/// so a v1 row still decodes if fields are added later.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BlockDigest {
    // --- eval (must equal what block_eval sends today, byte for byte)
    pub eval_repos: Vec<String>,
    pub eval_titles: Vec<String>,
    // --- billing
    pub folder: Option<String>,
    pub project_path: Option<String>,
    pub paths: Vec<String>,
    pub invoice_titles: Vec<String>,
    pub pinned_customer: Option<String>,
    // --- day summary / detail panel
    pub event_count: i64,
    pub events_by_source: BTreeMap<String, i64>,
    pub session_count: i64,
    pub active_minutes: i64,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
    // --- estimation (empty for personal blocks except jira_summary)
    pub jira_summary: Option<String>,
    pub branches: Vec<String>,
    pub change_titles: Vec<String>,
    pub prompts: Vec<String>,
    pub prompt_count: i64,
    pub files: Vec<String>,
    pub files_distinct: i64,
    pub tool_counts: BTreeMap<String, i64>,
}
