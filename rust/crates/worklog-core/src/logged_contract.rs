//! Shared types for the Logged section (spec 014): the Owner's real Tempo
//! entries per day, read back from the pulled store. Owned by the spec;
//! task code imports from here and never redeclares.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::tempo_hub_contract::WorklogOwner;

/// Widest range one request may read or fetch, inclusive `from..=to`
/// (a six-week month grid).
pub const LOGGED_MAX_RANGE_DAYS: i64 = 42;

/// Longest dismissal reason, counted in chars after trim.
pub const DISMISS_REASON_MAX_CHARS: usize = 80;

/// What a logged day reads as. `day_state` is the single definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayState {
    /// Nothing stored for the day yet — never read as 0h.
    NotFetched,
    /// Today or later: still being worked, never flagged.
    Pending,
    /// Required is unknown or 0 (weekend, holiday).
    Off,
    /// Logged reached required.
    Full,
    /// Under required, but the Owner dismissed it with a reason.
    Dismissed,
    /// A past day under required: "is this filled out?".
    Under,
}

/// One Tempo worklog on a logged day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedEntry {
    pub tempo_worklog_id: String,
    pub issue_id: i64,
    /// Jira key when the issue id is cached; `None` → show `issue_id`.
    pub jira_issue: Option<String>,
    pub seconds: i64,
    pub description: String,
    pub owner: WorklogOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedDay {
    pub day: String,
    pub logged_seconds: i64,
    pub required_seconds: Option<i64>,
    pub state: DayState,
    pub dismissal_reason: Option<String>,
    pub entries: Vec<LoggedEntry>,
}

/// `GET /logged` and `POST /logged/pull` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedRange {
    pub from: String,
    pub to: String,
    /// The daemon's local date (`$WORKLOG_TZ`).
    pub today: String,
    pub days: Vec<LoggedDay>,
    /// RFC3339 UTC of the newest pull over the range; `None` when no rows.
    pub pulled_at: Option<String>,
}

/// `POST /logged/pull` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeBody {
    pub from: String,
    pub to: String,
}

/// `POST /logged/dismiss` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DismissBody {
    pub day: String,
    pub reason: String,
}

/// `POST /logged/undismiss` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayBody {
    pub day: String,
}

/// The flag rule, first match wins.
pub fn day_state(
    day: NaiveDate,
    today: NaiveDate,
    logged_seconds: i64,
    required_seconds: Option<i64>,
    entry_count: usize,
    dismissed: bool,
) -> DayState {
    if required_seconds.is_none() && entry_count == 0 {
        return DayState::NotFetched;
    }
    if day >= today {
        return DayState::Pending;
    }
    let required = match required_seconds {
        None | Some(0) => return DayState::Off,
        Some(r) => r,
    };
    if logged_seconds >= required {
        DayState::Full
    } else if dismissed {
        DayState::Dismissed
    } else {
        DayState::Under
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3600;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn state(day: &str, logged: i64, required: Option<i64>, n: usize, dismissed: bool) -> DayState {
        day_state(d(day), d("2026-10-04"), logged, required, n, dismissed)
    }

    #[test]
    fn past_day_under_required_is_under() {
        assert_eq!(
            state("2026-10-01", 5 * H, Some(8 * H), 2, false),
            DayState::Under
        );
    }

    #[test]
    fn dismissed_under_day_is_dismissed() {
        assert_eq!(
            state("2026-10-01", 5 * H, Some(8 * H), 2, true),
            DayState::Dismissed
        );
    }

    #[test]
    fn full_day_ignores_a_stored_dismissal() {
        assert_eq!(
            state("2026-10-01", 8 * H, Some(8 * H), 3, true),
            DayState::Full
        );
    }

    #[test]
    fn required_zero_or_unknown_is_off() {
        assert_eq!(state("2026-10-03", 0, Some(0), 0, false), DayState::Off);
        assert_eq!(state("2026-10-01", H, None, 1, false), DayState::Off);
    }

    #[test]
    fn today_and_later_are_pending() {
        assert_eq!(
            state("2026-10-04", 0, Some(8 * H), 0, false),
            DayState::Pending
        );
        assert_eq!(
            state("2026-10-05", 0, Some(8 * H), 0, false),
            DayState::Pending
        );
    }

    #[test]
    fn nothing_stored_is_not_fetched() {
        assert_eq!(state("2026-10-01", 0, None, 0, false), DayState::NotFetched);
        assert_eq!(state("2026-10-09", 0, None, 0, false), DayState::NotFetched);
    }

    #[test]
    fn day_state_serialises_snake_case() {
        let json = serde_json::to_string(&DayState::NotFetched).unwrap();
        assert_eq!(json, "\"not_fetched\"");
    }
}
