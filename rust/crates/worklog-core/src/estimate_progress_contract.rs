use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const PROGRESS_STALE_SECS: i64 = 600;
pub const PROGRESS_CHART_DAYS: usize = 14;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PersonHours {
    pub account_id: String,
    pub name: String,
    pub is_you: bool,
    pub seconds: i64,
    pub by_day: Vec<(NaiveDate, i64)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TicketProgress {
    pub key: String,
    pub estimate_seconds: Option<i64>,
    pub people: Vec<PersonHours>,
    pub logged_seconds: i64,
    pub pulled_at: Option<String>,
    pub error: Option<ProgressError>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressError {
    JiraUnavailable,
    NotConfigured,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DayProgress {
    pub day: NaiveDate,
    pub tickets: Vec<TicketProgress>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RawWorklog {
    pub worklog_id: String,
    pub account_id: String,
    pub name: String,
    pub day: NaiveDate,
    pub seconds: i64,
}
