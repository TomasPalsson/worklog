//! Strongly-typed rows. ISO-8601 UTC timestamps kept as `String` on purpose —
//! SQLite stores them as TEXT and the Python runtime writes them the same way,
//! so we round-trip without parse/format divergence.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: Option<i64>,
    pub source: String,
    pub source_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub title: String,
    pub details: Option<String>,
    pub repo: Option<String>,
    pub project_path: Option<String>,
    pub jira_issue: Option<String>,
    pub session_id: Option<String>,
    pub tempo_worklog_id: Option<String>,
    pub raw_json: Option<String>,
}

impl Event {
    pub fn minimal(
        source: impl Into<String>,
        source_id: impl Into<String>,
        started_at: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            source: source.into(),
            source_id: source_id.into(),
            started_at: started_at.into(),
            ended_at: None,
            duration_seconds: None,
            title: title.into(),
            details: None,
            repo: None,
            project_path: None,
            jira_issue: None,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub id: i64,
    pub day: String,
    pub jira_issue: Option<String>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub description: Option<String>,
    pub estimated_by: Option<String>,
    pub flagged: bool,
    pub tempo_worklog_id: Option<String>,
    /// Auto-classified from the dominant project_path of the block's events
    /// (see worklog-core::personal). Personal blocks are dimmed in the
    /// review UI, skipped by the estimator, and excluded from Tempo sync.
    #[serde(default)]
    pub is_personal: bool,
    /// True when the block has been edited since `tempo_worklog_id` was
    /// recorded — the next `worklog sync` PUTs the new values to Tempo
    /// instead of POSTing a duplicate, then clears the flag.
    #[serde(default)]
    pub dirty: bool,
    /// Billing-export "has been billed" canary — set (idempotently) by
    /// `block_service::mark_exported` when the block's day is marked
    /// exported via `worklog export --mark`. Analogous to
    /// `tempo_worklog_id`, but Tempo-independent: the team moved off
    /// Tempo, so `exported_at` is the marker `purge` now also accepts
    /// as "billed" for blocks that never get a `tempo_worklog_id`.
    /// Never set or cleared by Tempo sync.
    #[serde(default)]
    pub exported_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JiraTicket {
    pub key: String,
    pub summary: String,
    pub status: Option<String>,
    pub project_key: Option<String>,
    pub updated: Option<String>,
    /// Numeric Atlassian issue ID. Tempo Cloud v4's `/worklogs` endpoint
    /// requires `issueId` (numeric) — `issueKey` was removed mid-2025.
    /// Populated by `worklog collect jira`; the tempo collector self-heals
    /// any missing ones with an inline Jira lookup.
    #[serde(default)]
    pub issue_id: Option<String>,
}

/// A Jira project, for the create-ticket project picker. `id` is the
/// numeric project id; `key` is the human prefix (e.g. `PROJ`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JiraProject {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
}

/// A Tempo account — the billing bucket that maps logged time to a real
/// customer. Surfaced in the create-ticket account picker; the chosen
/// account is written onto the new issue's Tempo account custom field so
/// every worklog against the ticket rolls up to the right customer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TempoAccount {
    /// Numeric Tempo account id — the value written to the Jira account
    /// custom field when creating an issue.
    pub id: i64,
    pub key: String,
    pub name: String,
    /// Customer name, when the account is linked to one. Shown alongside
    /// the account so the user picks the right customer at a glance.
    #[serde(default)]
    pub customer: Option<String>,
}
