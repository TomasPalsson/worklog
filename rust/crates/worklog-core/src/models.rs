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
