//! JSON contract for GET /stats?from&to. Mirrors
//! web/lib/stats_contract.ts field for field (snake_case); change both
//! together. Every number is computed in `stats.rs`; the web only draws.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Ranked {
    pub label: String,
    pub value: i64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct StatsTotals {
    pub work_seconds: i64,
    pub personal_seconds: i64,
    pub ignored_seconds: i64,
    pub blocks: i64,
    pub days_worked: i64,
    pub prompts: i64,
    pub tool_calls: i64,
    pub helpers: i64,
    pub claude_messages: i64,
    pub claude_busy_minutes: i64,
    pub shell_commands: i64,
    pub reflog_entries: i64,
    pub commits: i64,
    pub prs: i64,
    pub slack_messages: i64,
    pub browser_minutes: i64,
    pub meetings: i64,
    pub meeting_seconds: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyStat {
    pub day: String,
    pub work_seconds: i64,
    pub personal_seconds: i64,
    pub ignored_seconds: i64,
    pub prompts: i64,
    pub tool_calls: i64,
    pub shell: i64,
    pub slack: i64,
    pub browser_minutes: i64,
    pub claude_busy_minutes: i64,
    pub commits: i64,
    pub meeting_seconds: i64,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
    pub folders: i64,
    pub tickets: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FlowSource {
    pub source: String,
    pub n: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FlowBlockStat {
    pub seconds: i64,
    /// "work" | "personal" | "ignored"
    pub kind: &'static str,
    pub ticket: Option<String>,
    pub sources: Vec<FlowSource>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TicketStat {
    pub key: String,
    pub summary: Option<String>,
    pub seconds: i64,
    pub blocks: i64,
    pub first_day: String,
    pub last_day: String,
    pub days_active: i64,
    /// Sorted distinct local days with a block on this ticket.
    pub days: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PromptStats {
    pub count: i64,
    pub avg_chars: i64,
    pub longest_chars: i64,
    pub questions: i64,
    pub please: i64,
    pub thanks: i64,
    pub sorry: i64,
    pub swears: i64,
    pub exclaims: i64,
    /// "[Request interrupted by user…]" turns (not counted as prompts).
    pub interrupts: i64,
    /// Slash-command turns ("<command-…>", not counted as prompts).
    pub slash_commands: i64,
    pub top_openers: Vec<Ranked>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DayRecord {
    pub day: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CountRecord {
    pub day: String,
    pub n: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimeRecord {
    pub day: String,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LongestBlock {
    pub day: String,
    pub seconds: i64,
    pub ticket: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct StatsRecords {
    pub busiest_day: Option<DayRecord>,
    pub longest_block: Option<LongestBlock>,
    pub earliest_start: Option<TimeRecord>,
    pub latest_finish: Option<TimeRecord>,
    pub most_prompts: Option<CountRecord>,
    pub most_tools: Option<CountRecord>,
    pub longest_streak: i64,
    pub current_streak: i64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SyncStats {
    pub synced_seconds: i64,
    pub unsynced_seconds: i64,
    pub exported_blocks: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StatsReport {
    pub from: String,
    pub to: String,
    pub today: String,
    pub first_day: Option<String>,
    pub totals: StatsTotals,
    pub daily: Vec<DailyStat>,
    pub punchcard: Vec<Vec<i64>>,
    pub flow_blocks: Vec<FlowBlockStat>,
    pub tools: Vec<Ranked>,
    pub shell: Vec<Ranked>,
    pub slack_channels: Vec<Ranked>,
    pub domains: Vec<Ranked>,
    pub folders: Vec<Ranked>,
    pub helpers: Vec<Ranked>,
    pub tickets: Vec<TicketStat>,
    pub prompt: PromptStats,
    pub records: StatsRecords,
    pub estimates: Vec<Ranked>,
    pub ticket_origin: Vec<Ranked>,
    pub sync: SyncStats,
}
