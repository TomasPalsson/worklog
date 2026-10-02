//! Shared types for the Tempo hub (spec 012): My Tasks, Jira status and
//! comments, the AI ticket update draft, Tempo read-back and week
//! close-out. Owned by the spec; task code imports from here and never
//! redeclares. All seconds are a ticket line's effective seconds (see
//! `tempo_line_contract::TempoLine`) unless a field says otherwise.

use serde::{Deserialize, Serialize};

/// Longest comment the Owner may post (characters, not bytes).
pub const COMMENT_MAX_CHARS: usize = 5000;

/// Tempo v4 page size used by read-back.
pub const TEMPO_PAGE_LIMIT: usize = 1000;

/// Jira `statusCategory.key`. Stored in `jira_tickets.status_category`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusCategory {
    New,
    Indeterminate,
    Done,
}

impl StatusCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            StatusCategory::New => "new",
            StatusCategory::Indeterminate => "indeterminate",
            StatusCategory::Done => "done",
        }
    }

    /// Unknown keys (Jira's "undefined") read as `None`.
    pub fn parse(s: &str) -> Option<StatusCategory> {
        match s {
            "new" => Some(StatusCategory::New),
            "indeterminate" => Some(StatusCategory::Indeterminate),
            "done" => Some(StatusCategory::Done),
            _ => None,
        }
    }
}

/// One card on My Tasks. `assigned` = in the cached assigned-open set
/// (external = 0 and category not done); false = shown only because it
/// has a ticket line this week ("Worked this week").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRow {
    pub key: String,
    pub summary: String,
    pub status: Option<String>,
    pub status_category: Option<StatusCategory>,
    /// `{jira_base_url}/browse/{key}`; `None` when the base URL secret is unset.
    pub url: Option<String>,
    pub assigned: bool,
    pub week_seconds: i64,
    pub today_seconds: i64,
    /// Latest day (YYYY-MM-DD) with a ticket line for this key, any week.
    pub last_worked_day: Option<String>,
    /// Jira issue type name ("Bug", "Story", "Task", "Epic", "Sub-task", …); None until a refresh stores it.
    pub issue_type: Option<String>,
    /// Jira priority name ("Highest", "High", "Medium", "Low", "Lowest", …).
    pub priority: Option<String>,
    /// Jira `duedate`, YYYY-MM-DD.
    pub due_date: Option<String>,
    /// Jira labels, as Jira sent them; empty when none.
    pub labels: Vec<String>,
    /// Summary of the parent issue (epic or parent story), if any.
    pub parent_summary: Option<String>,
    /// Jira `updated` from the cache (ISO-8601 as Jira sent it).
    pub updated: Option<String>,
    /// Effective seconds per day of the requested week, Monday first, always 7 entries.
    pub day_seconds: Vec<i64>,
}

/// `GET /tasks?monday=` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TasksResponse {
    pub monday: String,
    pub today: String,
    pub tasks: Vec<TaskRow>,
    /// `MAX(jira_tickets.fetched_at)`.
    pub last_fetched: Option<String>,
}

/// One live Jira transition for a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to_status: String,
    pub to_category: Option<StatusCategory>,
}

/// `POST /tickets/:key/transition` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionBody {
    pub transition_id: String,
}

/// `POST /tickets/:key/transition` response: Jira's status after the move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketStatus {
    pub key: String,
    pub status: String,
    pub status_category: Option<StatusCategory>,
}

/// `POST /tickets/:key/comment` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentBody {
    pub text: String,
}

/// `POST /tickets/:key/draft` response. `suggested_transition_id` is
/// always one of `transitions[].id` or `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDraft {
    pub comment: String,
    pub suggested_transition_id: Option<String>,
    pub transitions: Vec<Transition>,
}

/// Who created a pulled Tempo worklog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorklogOwner {
    /// Its id is on at least one block (`blocks.tempo_worklog_id`).
    Worklog,
    /// Typed in Tempo (or another tool); worklog never edits or deletes it.
    Outside,
}

impl WorklogOwner {
    pub fn as_str(self) -> &'static str {
        match self {
            WorklogOwner::Worklog => "worklog",
            WorklogOwner::Outside => "outside",
        }
    }

    pub fn parse(s: &str) -> Option<WorklogOwner> {
        match s {
            "worklog" => Some(WorklogOwner::Worklog),
            "outside" => Some(WorklogOwner::Outside),
            _ => None,
        }
    }
}

/// One worklog as the Tempo client returns it, before classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PulledWorklog {
    pub tempo_worklog_id: String,
    pub day: String,
    pub issue_id: i64,
    pub seconds: i64,
    pub description: String,
}

/// One day of the Owner's Tempo schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredDay {
    pub day: String,
    pub required_seconds: i64,
}

/// One row of `tempo_remote_worklogs`, as Tempo reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteWorklog {
    /// Stringified Tempo id (Tempo returns an integer; blocks store TEXT).
    pub tempo_worklog_id: String,
    /// Tempo `startDate`, YYYY-MM-DD.
    pub day: String,
    pub issue_id: i64,
    /// Resolved via `jira_tickets.issue_id`; `None` when not cached.
    pub jira_issue: Option<String>,
    /// Tempo `timeSpentSeconds` (raw, not re-rounded).
    pub seconds: i64,
    pub description: String,
    pub owner: WorklogOwner,
}

/// `POST /tempo/pull` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullReport {
    pub monday: String,
    pub worklogs: usize,
    pub outside: usize,
    pub schedule_days: usize,
    pub pulled_at: String,
}

/// One day of week close-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseoutDay {
    pub day: String,
    /// Sum of the day's ticket lines' effective seconds.
    pub logged_seconds: i64,
    /// Part of `logged_seconds` whose lines are fully synced and clean.
    pub synced_seconds: i64,
    /// Sum of all pulled worklogs that day (worklog + outside).
    pub tempo_seconds: i64,
    /// Part of `tempo_seconds` with owner = outside.
    pub outside_seconds: i64,
    /// Tempo `requiredSeconds`; `None` when never pulled.
    pub required_seconds: Option<i64>,
    /// Raw seconds of work blocks with no `jira_issue` (not personal, not ignored).
    pub unticketed_seconds: i64,
    /// Ticket lines with at least one unsynced or dirty block.
    pub pending_lines: i64,
}

/// `GET /weeks/:monday/closeout` response. `days` is always 7, Mon..Sun.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekCloseout {
    pub monday: String,
    pub days: Vec<CloseoutDay>,
    /// Latest `pulled_at` across the week's pulled rows; `None` = never pulled.
    pub pulled_at: Option<String>,
}

/// The closed set of hub failures and their HTTP mapping (in `daemon_tasks`
/// / `daemon_week`): `InvalidInput` → 400, `NotFound` → 404, `Upstream` → 502.
/// Body is always `{"error": <Display>}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubError {
    InvalidInput(String),
    NotFound(String),
    /// Jira or Tempo answered non-2xx; `body` is their text, verbatim.
    Upstream {
        service: &'static str,
        status: u16,
        body: String,
    },
}

impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::InvalidInput(m) | HubError::NotFound(m) => f.write_str(m),
            HubError::Upstream {
                service,
                status,
                body,
            } => write!(f, "{service} said {status}: {body}"),
        }
    }
}

impl std::error::Error for HubError {}

/// One Jira comment, body flattened from ADF to plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketComment {
    pub id: String,
    pub author: String,
    pub created: String,
    pub body: String,
}

/// `GET /tickets/:key/detail` response — read live from Jira, not cached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDetail {
    pub key: String,
    pub summary: String,
    pub status: Option<String>,
    pub status_category: Option<StatusCategory>,
    pub issue_type: Option<String>,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    /// Jira `fields.updated`, as Jira sent it (ISO-8601).
    pub updated: Option<String>,
    /// `{base_url}/browse/{key}`.
    pub url: String,
    /// Plain text; "" when Jira has no description.
    pub description: String,
    /// Oldest first.
    pub comments: Vec<TicketComment>,
}

/// One day of work on a ticket, newest day first in `TicketBlocks.days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TicketDay {
    /// YYYY-MM-DD (local day bucket).
    pub day: String,
    /// The day's ticket-line effective seconds (what Tempo gets; override-aware). 0 when there is no line.
    pub line_seconds: i64,
    /// The line text Tempo gets that day (stored text, else fallback); "" when none.
    pub line_text: String,
    /// The ticket's blocks that day, oldest first. Personal and ignored blocks are excluded.
    pub blocks: Vec<crate::models::Block>,
}

/// `GET /tickets/:key/blocks` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TicketBlocks {
    pub key: String,
    /// First and last day of the window (YYYY-MM-DD).
    pub from: String,
    pub to: String,
    /// Only days that have at least one block, newest first.
    pub days: Vec<TicketDay>,
}
