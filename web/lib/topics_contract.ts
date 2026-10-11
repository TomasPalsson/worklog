// Mirrors rust/crates/worklog-core/src/topics_contract.rs —
// GET /stats/topics?from&to and GET /days/:day/noise-hosts.
// Every number is computed in Rust; the web only draws. Topic output is
// display-only. Days are YYYY-MM-DD.

import type { Ranked } from "./stats_contract";

export type TopicsStatus = "ready" | "helper_down" | "too_few_blocks";

export interface Topic {
  /** 0-based, ordered by seconds descending. */
  id: number;
  /** Top ticket's summary by hours, else the longest block's description. */
  name: string;
  name_ticket: string | null;
  seconds: number;
  blocks: number;
  top_tickets: Ranked[];
}

export interface DaySwitches {
  day: string;
  /** Consecutive work blocks whose topics differ. */
  switches: number;
  work_seconds: number;
  longest_run_seconds: number;
}

export interface TopicDrift {
  topic_id: number;
  name: string;
  seconds_now: number;
  seconds_before: number;
}

export interface TicketSprawl {
  ticket: string;
  summary: string | null;
  blocks: number;
  /** 1 − mean pairwise cosine similarity (0 = all alike). */
  spread: number;
}

export interface SimilarDay {
  day: string;
  similarity: number;
  work_seconds: number;
  top_tickets: Ranked[];
}

export interface TopicsReport {
  status: TopicsStatus;
  from: string;
  to: string;
  topics: Topic[];
  switches: DaySwitches[];
  drift: TopicDrift[];
  sprawl: TicketSprawl[];
  similar_days: SimilarDay[];
  embedded_blocks: number;
  skipped_blocks: number;
}

export interface NoiseHost {
  /** Lowercased hostname, same as `new URL(details).hostname`. */
  host: string;
  dismissed_before: number;
}

export interface NoiseHosts {
  hosts: NoiseHost[];
}
