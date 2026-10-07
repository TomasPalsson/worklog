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
/// FR-43: a stretch with no block counts as a gap from this long.
pub const RECAP_MIN_GAP_SECONDS: i64 = 900;
/// FR-43: how many gaps the recap lists, longest first.
pub const RECAP_TOP_GAPS: usize = 3;
/// §5: the Tempo read before a send gives up after this and sends as today (FR-09).
pub const TEMPO_READ_TIMEOUT_SECONDS: u64 = 10;
/// FR-07, FR-10: `tempo_line_texts.match_status` for a line Tempo already holds.
pub const ALREADY_IN_TEMPO: &str = "already_in_tempo";

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
    /// The text the Owner edits and posts: one numbered line per answer,
    /// no question text (the team knows the questions), "Nothing." when
    /// a list is empty.
    pub fn to_text(&self) -> String {
        [&self.today, &self.next, &self.blockers]
            .iter()
            .enumerate()
            .map(|(n, answers)| {
                let answer = if answers.is_empty() {
                    "Nothing.".to_owned()
                } else {
                    answers.join(" ")
                };
                format!("{}. {answer}\n", n + 1)
            })
            .collect()
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

/// FR-06..09: what the pre-send check decided for one line.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum MatchVerdict {
    /// Same work within [`SAME_HOURS_TOLERANCE_SECONDS`]: not sent (FR-07).
    AlreadyInTempo { tempo_worklog_id: String },
    /// Different work, or no entry on the ticket that day: sent as today (FR-08).
    Different,
    /// Verdict off, unreachable or abstaining, or the Tempo read failed: sent as today (FR-09).
    Unchecked { reason: String },
}

/// FR-43: a line the 17:00 run sent.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RecapLine {
    pub jira_issue: String,
    pub seconds: i64,
}

/// FR-43: a line the 17:00 run did not send, and why.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HeldBackLine {
    pub jira_issue: String,
    pub reason: String,
}

/// FR-43: a stretch inside the work-hours window with no block (RFC3339 UTC).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RecapGap {
    pub started_at: String,
    pub ended_at: String,
    pub minutes: i64,
}

/// FR-43: what the Owner sees right after the 17:00 run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Recap {
    pub day: String,
    pub sent: Vec<RecapLine>,
    pub held_back: Vec<HeldBackLine>,
    /// Block seconds inside the work-hours window / the window's seconds, 0..=100, rounded.
    pub coverage_percent: u32,
    /// At most [`RECAP_TOP_GAPS`], longest first.
    pub gaps: Vec<RecapGap>,
}

/// FR-44: how the Owner resolves one gap.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GapAction {
    /// A personal block over the gap: never billed, never sent.
    Personal,
    /// Recorded as a break: never billed, never sent, gone from the gap list.
    Break,
    /// A work block over the gap on this ticket.
    PickTicket { jira_issue: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standup_text_numbers_answers_without_questions() {
        let d = StandupDraft {
            today: vec![
                "Digging into the tenant stack.".into(),
                "Then a review.".into(),
            ],
            next: vec![],
            blockers: vec![],
        };
        // catches: repeating the questions, or dropping an empty answer's line
        assert_eq!(
            d.to_text(),
            "1. Digging into the tenant stack. Then a review.\n2. Nothing.\n3. Nothing.\n"
        );
    }

    #[test]
    fn undo_outcome_wire_shape() {
        let v = serde_json::to_value(UndoOutcome::RefusedSynced { block_id: 7 }).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"outcome": "refused_synced", "block_id": 7})
        );
    }

    #[test]
    fn match_verdict_and_gap_action_wire_shapes() {
        let v = serde_json::to_value(MatchVerdict::AlreadyInTempo {
            tempo_worklog_id: "77".into(),
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"verdict": "already_in_tempo", "tempo_worklog_id": "77"})
        );
        let a = serde_json::to_value(GapAction::PickTicket {
            jira_issue: "GENAI-12".into(),
        })
        .unwrap();
        assert_eq!(
            a,
            serde_json::json!({"action": "pick_ticket", "jira_issue": "GENAI-12"})
        );
        assert_eq!(
            serde_json::to_value(GapAction::Break).unwrap(),
            serde_json::json!({"action": "break"})
        );
    }
}
