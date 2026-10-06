//! Shared names and shapes for spec 017 (Verdict does more).
//!
//! Every task that touches Verdict control, the decision log, ticket
//! picks, line checks or auto-send imports from here. A type or constant
//! a task needs that is missing here is an escalation to the orchestrator,
//! never a local re-declaration.

use serde::{Deserialize, Serialize};

/// Envfile key: `on` / `off`. Missing means on (FR-06).
pub const VERDICT_ENABLED_KEY: &str = "WORKLOG_VERDICT_ENABLED";
/// Envfile key: `on` / `off`. Missing means off (A7).
pub const AUTO_SEND_KEY: &str = "WORKLOG_TEMPO_AUTO_SEND";
/// Owner-local hour (via `$WORKLOG_TZ`) when ready lines are sent.
pub const AUTO_SEND_HOUR: u32 = 17;

/// Most options Verdict is offered for one decision (FR-10, FR-16).
pub const SHORTLIST_MAX: usize = 6;
/// Most past corrected events shown per option (FR-12).
pub const EXAMPLES_MAX: usize = 5;
/// Example text budget so the event keeps >= 200 of Verdict's 512 input
/// tokens (spec §5): each example cut to this many chars...
pub const EXAMPLE_CHARS_EACH: usize = 60;
/// ...and all examples of one request together capped at this many chars.
pub const EXAMPLE_CHARS_TOTAL: usize = 300;
/// Window for "worked in recently" project options.
pub const RECENT_PROJECT_DAYS: i64 = 14;
/// Window for "logged in this folder recently" ticket options.
pub const RECENT_TICKET_DAYS: i64 = 30;
/// Window the scorecard replays (FR-22).
pub const SCORECARD_DAYS: i64 = 30;

pub const HEALTH_INTERVAL_SECS: u64 = 60;
/// Restarts allowed inside `RESTART_WINDOW_SECS` before giving up (FR-03).
pub const RESTART_LIMIT: u32 = 3;
pub const RESTART_WINDOW_SECS: u64 = 600;
/// Places `uv` is looked for, in order, before `PATH` (A1). `~` is `$HOME`.
pub const UV_CANDIDATES: [&str; 3] = [
    "~/.local/bin/uv",
    "/opt/homebrew/bin/uv",
    "/usr/local/bin/uv",
];

/// What a decision-log row is about. Stored as the snake_case string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Project,
    Ticket,
    LineText,
}

/// Who made the decision. Stored as the snake_case string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    Verdict,
    Owner,
}

/// One option with Verdict's probability for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedOption {
    pub id: String,
    pub probability: f64,
}

/// Verdict's answer to one `/classify` call (FR-07, FR-14).
/// `ranking` is the top 3, best first. `agreed` is false when the
/// reversed-order pass picked a different winner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ranking {
    pub ranking: Vec<RankedOption>,
    pub abstain: f64,
    pub agreed: bool,
}

/// One row of the permanent decision log (table `verdict_decisions`).
/// `subject` is the event id, block id, or `<day>|<jira_issue>` for a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRow {
    pub kind: DecisionKind,
    pub source: DecisionSource,
    pub subject: String,
    /// The JSON Verdict was shown (Verdict rows) or `{}` (Owner rows).
    pub state_json: String,
    pub options: Vec<String>,
    /// `None` for Owner rows.
    pub ranking: Option<Ranking>,
    /// What was applied, or `None` when Verdict abstained.
    pub chosen: Option<String>,
    /// The value before this decision, for Owner corrections (FR-08).
    pub previous: Option<String>,
    /// RFC3339 UTC.
    pub decided_at: String,
}

/// Verdict helper state shown in Settings and on the day page (FR-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VerdictState {
    Off,
    Starting,
    Running,
    NotAnswering,
    NeedsUv,
    Stopped { error: String },
}

/// `GET /verdict/status?day=YYYY-MM-DD` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerdictStatus {
    #[serde(flatten)]
    pub state: VerdictState,
    /// Loose events of `day` with no Verdict decision row (FR-05).
    pub unchecked: u32,
    /// One-line summary of the last scorecard, if any (FR-24).
    pub scorecard: Option<String>,
}

/// Result of the two text checks on a generated Tempo line.
/// Stored on `tempo_line_texts.check_status` as the snake_case string;
/// NULL means not checked (manual lines are never checked, FR-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCheck {
    Passed,
    NeedsLook,
}

/// An auto-sent line's place in the Review section (FR-29, FR-32).
/// Derived, never stored: Sent = its blocks carry a Tempo id and
/// `tempo_line_texts.confirmed_at` is NULL; NotSent = `send_error` is set.
/// Confirmed lines are not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReviewStatus {
    Sent,
    NotSent { error: String },
}

/// One row of `GET /review` (FR-29), newest day first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewLine {
    pub day: String,
    pub jira_issue: String,
    pub seconds: i64,
    pub text: String,
    #[serde(flatten)]
    pub status: ReviewStatus,
}
