// Mirrors rust/crates/worklog-core/src/stats_contract.rs — GET /stats?from&to.
// Every number is computed in Rust; the web only draws. Times of day are
// local to $WORKLOG_TZ ("HH:MM"). Days are YYYY-MM-DD.

/** A ranked row for a top-N list (tools, commands, channels, ...). */
export interface Ranked {
  label: string;
  value: number;
}

export interface StatsTotals {
  work_seconds: number;
  personal_seconds: number;
  ignored_seconds: number;
  blocks: number;
  /** Days in range with any work block. */
  days_worked: number;
  /** claude_turn events. */
  prompts: number;
  /** claude_tool events. */
  tool_calls: number;
  /** claude_helper events (subagents). */
  helpers: number;
  /** claude_message events (cross-session messages). */
  claude_messages: number;
  /** claude_work rows: one per busy session-minute. */
  claude_busy_minutes: number;
  shell_commands: number;
  reflog_entries: number;
  /** github_commit events + reflog entries whose message starts with "commit". */
  commits: number;
  prs: number;
  slack_messages: number;
  /** firefox rows: one per heartbeat minute. */
  browser_minutes: number;
  meetings: number;
  meeting_seconds: number;
}

/** One row per day in range, zero-filled. */
export interface DailyStat {
  day: string;
  work_seconds: number;
  personal_seconds: number;
  ignored_seconds: number;
  prompts: number;
  tool_calls: number;
  shell: number;
  slack: number;
  browser_minutes: number;
  claude_busy_minutes: number;
  commits: number;
  meeting_seconds: number;
  /** First / last human event of the day, local "HH:MM"; null when none. */
  first_at: string | null;
  last_at: string | null;
  /** Distinct billable work folders touched by work blocks that day. */
  folders: number;
  /** Distinct tickets on work blocks that day. */
  tickets: number;
}

/** Same shape the day page feeds DayFlow. */
export interface FlowBlockStat {
  seconds: number;
  kind: "work" | "personal" | "ignored";
  ticket: string | null;
  sources: { source: string; n: number }[];
}

export interface TicketStat {
  key: string;
  summary: string | null;
  seconds: number;
  blocks: number;
  first_day: string;
  last_day: string;
  /** Distinct days with a block on this ticket. */
  days_active: number;
  /** Sorted distinct local days (YYYY-MM-DD) with a block on this ticket. */
  days: string[];
  /** Work seconds on this ticket per entry of `days` (same order, same length). */
  day_seconds: number[];
  /** Jira status name ("In Progress"), null when the ticket isn't cached. */
  status: string | null;
  /** Jira status category key ("new" | "indeterminate" | "done"), null when unknown. */
  status_category: string | null;
}

export interface PromptStats {
  count: number;
  avg_chars: number;
  longest_chars: number;
  /** Prompts ending in "?". */
  questions: number;
  please: number;
  thanks: number;
  sorry: number;
  /** fuck / shit / damn / wtf / crap, any case, word-ish match. */
  swears: number;
  /** Prompts containing "!". */
  exclaims: number;
  /** "[Request interrupted by user…]" turns (not counted in `count`). */
  interrupts: number;
  /** Slash-command turns (not counted in `count`). */
  slash_commands: number;
  /** First word, lowercased, top 8. */
  top_openers: Ranked[];
}

export interface DayRecord {
  day: string;
  seconds: number;
}
export interface CountRecord {
  day: string;
  n: number;
}
export interface TimeRecord {
  day: string;
  time: string;
}

export interface StatsRecords {
  busiest_day: DayRecord | null;
  longest_block: { day: string; seconds: number; ticket: string | null } | null;
  earliest_start: TimeRecord | null;
  latest_finish: TimeRecord | null;
  most_prompts: CountRecord | null;
  most_tools: CountRecord | null;
  /** Consecutive days with work, longest in range / ending today (or the range end). */
  longest_streak: number;
  current_streak: number;
}

export interface StatsReport {
  from: string;
  to: string;
  today: string;
  /** Earliest day with any event at all (for the "all time" chip). */
  first_day: string | null;
  totals: StatsTotals;
  daily: DailyStat[];
  /** [weekday 0=Mon..6=Sun][hour 0..23] — human events (claude_turn, shell, commits, PRs, slack, firefox). */
  punchcard: number[][];
  flow_blocks: FlowBlockStat[];
  /** Claude tool names by calls, top 12. */
  tools: Ranked[];
  /** Shell commands by first word, top 12. */
  shell: Ranked[];
  /** Slack channel / DM names by messages, top 10. */
  slack_channels: Ranked[];
  /** Browser hosts (no "www.") by minutes, top 10. */
  domains: Ranked[];
  /** Work folders by work seconds, top 10. */
  folders: Ranked[];
  /** Subagent kinds by count, top 8. */
  helpers: Ranked[];
  /** Top 40 tickets by work seconds. */
  tickets: TicketStat[];
  prompt: PromptStats;
  records: StatsRecords;
  /** Blocks by estimated_by: label "manual" | "auto" | other values verbatim. */
  estimates: Ranked[];
  /** Blocks by ticket_origin ("event" | "auto" | "manual" | "none"). */
  ticket_origin: Ranked[];
  sync: { synced_seconds: number; unsynced_seconds: number; exported_blocks: number };
}
