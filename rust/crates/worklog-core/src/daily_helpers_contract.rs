//! Spec 018 shared shapes. Mirrored by `web/lib/daily_helpers_contract.ts`.
//! Owned by the orchestrator: a task that needs a new shape escalates.

use serde::{Deserialize, Serialize};

/// FR-26: how many Owner block changes undo remembers.
pub const UNDO_DEPTH: usize = 20;
/// FR-07: an existing Tempo entry counts as the same hours within this.
pub const SAME_HOURS_TOLERANCE_SECONDS: i64 = 1800;
/// FR-40: an In Progress ticket with no activity this long is stale.
pub const STALE_TICKET_DAYS: i64 = 14;
/// §5: nudge data is at most this old.
pub const NUDGE_CACHE_SECONDS: i64 = 600;
/// FR-21: today's team message starts with this.
pub const DAILY_THREAD_PREFIX: &str = "Daily:thread";
/// FR-21: envfile key holding the Slack channel name of the Daily thread.
pub const SLACK_DAILY_CHANNEL_KEY: &str = "WORKLOG_SLACK_DAILY_CHANNEL";
/// FR-18: the team's three questions, in order.
pub const STANDUP_QUESTIONS: [&str; 3] = [
    "What are you working on today?",
    "What is next/coming up?",
    "Are there any blockers we need to clear?",
];

/// FR-25: every Owner block change undo can reverse.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockChange {
    Delete,
    Merge,
    Split,
    Ticket,
    Hours,
    Text,
    Personal,
    Ignored,
}

/// FR-25..28: result of one undo.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum UndoOutcome {
    Restored {
        change: BlockChange,
        block_ids: Vec<i64>,
    },
    NothingToUndo,
    RefusedSynced {
        block_id: i64,
    },
}

/// FR-11, FR-14: one checklist row kind.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreflightCheck {
    Ticketed,
    NoDoubleCount,
    DayHours,
    LineText,
    ReadBack,
}

/// FR-11..14: one checklist row. `target` names the block id, line
/// (`DAY ISSUE`) or day at fault (FR-12); `None` when `ok`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PreflightRow {
    pub check: PreflightCheck,
    pub ok: bool,
    pub detail: String,
    pub target: Option<String>,
}

/// FR-18: the three answers, one bullet per string.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct StandupDraft {
    pub today: Vec<String>,
    pub next: Vec<String>,
    pub blockers: Vec<String>,
}

impl StandupDraft {
    /// The text the Owner edits and posts: each question numbered, its
    /// bullets under it, "None" when a list is empty.
    pub fn to_text(&self) -> String {
        let answers = [&self.today, &self.next, &self.blockers];
        let mut out = String::new();
        for (n, (question, bullets)) in STANDUP_QUESTIONS.iter().zip(answers).enumerate() {
            out.push_str(&format!("{}. {}\n", n + 1, question));
            if bullets.is_empty() {
                out.push_str("• None\n");
            }
            for b in bullets {
                out.push_str(&format!("• {b}\n"));
            }
            if n < 2 {
                out.push('\n');
            }
        }
        out
    }
}

/// FR-21..23: result of posting the standup.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PostOutcome {
    Posted { permalink: String },
    NoChannel,
    NoThread { channel: String },
    SlackRefused { error: String },
}

/// FR-40: the three nudge kinds.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NudgeKind {
    ReviewRequested,
    MergedNotDone,
    Stale,
}

/// FR-39..42: one footer line.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Nudge {
    pub kind: NudgeKind,
    pub text: String,
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standup_text_numbers_questions_and_fills_empty() {
        let d = StandupDraft {
            today: vec!["GENAI-12 tenant stack".into()],
            next: vec![],
            blockers: vec![],
        };
        let t = d.to_text();
        assert!(t.starts_with("1. What are you working on today?\n• GENAI-12 tenant stack\n"));
        assert!(t.contains("2. What is next/coming up?\n• None\n"));
        assert!(t.ends_with("3. Are there any blockers we need to clear?\n• None\n"));
    }

    #[test]
    fn undo_outcome_wire_shape() {
        let v = serde_json::to_value(UndoOutcome::RefusedSynced { block_id: 7 }).unwrap();
        assert_eq!(v, serde_json::json!({"outcome": "refused_synced", "block_id": 7}));
    }
}
