import type { DailyStat } from "@/lib/stats_contract";

/** Test helper: a zero-filled DailyStat with overrides. */
export const day = (o: Partial<DailyStat> & { day: string }): DailyStat => ({
  work_seconds: 0,
  personal_seconds: 0,
  ignored_seconds: 0,
  prompts: 0,
  tool_calls: 0,
  shell: 0,
  slack: 0,
  browser_minutes: 0,
  claude_busy_minutes: 0,
  commits: 0,
  meeting_seconds: 0,
  first_at: null,
  last_at: null,
  folders: 0,
  tickets: 0,
  ...o,
});
