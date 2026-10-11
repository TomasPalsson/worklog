//! Spec 021 shared shapes. Mirrored by `web/lib/time_leaks_contract.ts`.
//! Owned by the orchestrator: a task that needs a new shape escalates.

use serde::{Deserialize, Serialize};

/// FR-04: why the Owner ignored a block. Stored in `blocks.ignore_reason`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreReason {
    Noise,
    Duplicate,
}

impl IgnoreReason {
    pub fn as_str(self) -> &'static str {
        match self {
            IgnoreReason::Noise => "noise",
            IgnoreReason::Duplicate => "duplicate",
        }
    }

    /// Unknown text → `None` (a DB row is not trusted to hold a known value).
    pub fn parse(s: &str) -> Option<IgnoreReason> {
        match s {
            "noise" => Some(IgnoreReason::Noise),
            "duplicate" => Some(IgnoreReason::Duplicate),
            _ => None,
        }
    }
}

/// FR-11: what a loose end still lacks.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LooseEndMissing {
    Tempo,
    Export,
}

/// FR-11: one ticketed work block in the cycle that is not in Tempo or not exported.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LooseEnd {
    pub block_id: i64,
    pub day: String,
    pub jira_issue: String,
    pub started_at: String,
    pub ended_at: String,
    pub seconds: i64,
    pub description: Option<String>,
    pub missing: Vec<LooseEndMissing>,
}

/// FR-11: the current billing cycle's loose ends.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LooseEnds {
    pub cycle_start: String,
    pub cycle_end: String,
    pub items: Vec<LooseEnd>,
}

/// FR-16: meta key holding a source's last success time (ISO-8601 UTC).
pub fn collect_ok_key(source: &str) -> String {
    format!("collect.{source}.last_ok")
}

/// FR-16: meta key holding `"<ISO-8601 UTC> <message>"` of a source's last error.
pub fn collect_err_key(source: &str) -> String {
    format!("collect.{source}.last_err")
}

/// FR-17: one row of `worklog status`'s source list.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SourceHealth {
    pub source: String,
    pub last_ok: Option<String>,
    pub last_err: Option<String>,
    pub events_24h: i64,
}

/// FR-18 / §5: GitHub search returns at most 1,000 results = 10 pages of 100.
pub const GITHUB_MAX_PAGES: u32 = 10;

/// FR-23: a gap up to this long keeps a focus run going.
pub const FOCUS_MAX_GAP_SECONDS: i64 = 900;

/// FR-23: the longest run of consecutive same-ticket blocks.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FocusRun {
    pub jira_issue: String,
    pub started_at: String,
    pub ended_at: String,
    pub seconds: i64,
}

/// FR-23: yesterday's switches and longest focus, for the local draft only.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FocusSummary {
    pub day: String,
    pub switches: u32,
    pub longest: Option<FocusRun>,
}

/// FR-21: the deild the Owner saved most often for a customer (all history, ties → most recent).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DeildSuggestion {
    pub customer: String,
    pub deild: String,
    pub uses: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_reason_round_trips_and_rejects_unknown() {
        for r in [IgnoreReason::Noise, IgnoreReason::Duplicate] {
            assert_eq!(IgnoreReason::parse(r.as_str()), Some(r));
            assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{}\"", r.as_str()));
        }
        assert_eq!(IgnoreReason::parse("internal"), None);
    }

    #[test]
    fn collect_keys_are_namespaced() {
        assert_eq!(collect_ok_key("github"), "collect.github.last_ok");
        assert_eq!(collect_err_key("github"), "collect.github.last_err");
    }
}
