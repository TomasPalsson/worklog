//! Shared types for the Jira assistant (spec 013): start a ticket, create
//! one with a learned billing account, and status suggestions. Owned by
//! the spec; task code imports from here and never redeclares. Errors
//! reuse `tempo_hub_contract::HubError` (InvalidInput → 400, NotFound →
//! 404, Upstream → 502).

use serde::{Deserialize, Serialize};

use crate::tempo_hub_contract::{StatusCategory, TicketDetail};

/// The only Jira project the assistant writes to. Reads accept any key.
pub const WRITE_PROJECT: &str = "GENAI";
/// Issue type every created ticket gets.
pub const CREATE_ISSUE_TYPE: &str = "Story";
/// A clue wrong this many times for one account stops counting for it.
pub const CLUE_DROP_AFTER_WRONG: i64 = 2;
/// Tickets read by a relearn.
pub const RELEARN_LIMIT: usize = 200;
/// Account suggestions returned, best first.
pub const SUGGEST_LIMIT: usize = 3;
/// Free-text search matches returned.
pub const FIND_LIMIT: usize = 5;

/// `true` when `key` belongs to `WRITE_PROJECT` (`GENAI-123`).
pub fn is_writable_key(key: &str) -> bool {
    key.split_once('-')
        .is_some_and(|(project, _)| project == WRITE_PROJECT)
}

/// `GET /tickets/:key/view` and the body of a start result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TicketView {
    pub detail: TicketDetail,
    /// The account field's id as Jira holds it; `None` when unset.
    pub account_id: Option<String>,
    /// The account's name when Jira returned it.
    pub account_name: Option<String>,
}

/// What `POST /tickets/:key/start` did to the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartOutcome {
    /// Was To Do, now In Progress.
    Moved,
    /// Already In Progress or Done — left alone.
    AlreadyStarted,
    /// Not a `WRITE_PROJECT` ticket — left alone.
    NotWritable,
}

/// `POST /tickets/:key/start` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartResult {
    pub view: TicketView,
    pub outcome: StartOutcome,
}

/// `POST /tickets/:key/move` body: the target status by name
/// (case-insensitive match on the transition's `to_status`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveBody {
    pub to_status: String,
}

/// One allowed billing account for `WRITE_PROJECT` / `CREATE_ISSUE_TYPE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedAccount {
    /// Numeric id as a string, sent to Jira as a bare number.
    pub id: String,
    pub name: String,
}

/// One ranked account suggestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSuggestion {
    pub account: AllowedAccount,
    /// Clues found in the text, as stored.
    pub matched_clues: Vec<String>,
    /// Tickets the clue log has seen on this account.
    pub past_tickets: i64,
    pub score: f64,
}

/// `POST /accounts/suggest` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestBody {
    pub text: String,
}

/// `POST /accounts/relearn` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelearnReport {
    pub tickets_read: usize,
    pub accounts: usize,
    pub clues: usize,
}

/// `POST /tickets/assist-create` body. `description` is markdown
/// (headings, bullets, `- [ ]` checkboxes, paragraphs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistCreateBody {
    pub summary: String,
    pub description: String,
    pub account_id: String,
    /// The account worklog suggested first; `None` when the Owner named it.
    #[serde(default)]
    pub guessed_account_id: Option<String>,
    /// Clues from the request text, logged with the decision.
    #[serde(default)]
    pub clues: Vec<String>,
    /// Jira accountId; `None` → the Owner (Jira `/myself`), unless `unassigned`.
    #[serde(default)]
    pub assignee_account_id: Option<String>,
    /// Leave the ticket unassigned; wins over the Owner default.
    #[serde(default)]
    pub unassigned: bool,
}

/// `POST /tickets/assist-create` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistCreated {
    pub key: String,
    pub url: String,
    pub status: Option<String>,
    pub account: AllowedAccount,
}

/// Why a status move is suggested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum HintReason {
    PrMerged {
        repo: String,
        number: i64,
        merged_at: String,
    },
}

/// One suggested move. Never applied without the Owner's yes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusHint {
    pub key: String,
    pub summary: String,
    pub to_category: StatusCategory,
    pub reason: HintReason,
}
