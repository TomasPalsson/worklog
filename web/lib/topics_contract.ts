// Mirrors rust/crates/worklog-core/src/topics_contract.rs —
// GET /stats/topics?from&to and GET /days/:day/noise-hosts.
// Every number is computed in Rust; the web only draws. Topic output is
// display-only. Days are YYYY-MM-DD.

import type { Ranked } from "./stats_contract";

export type TopicsStatus = "ready" | "helper_down" | "too_few_blocks";
export type NamedBy = "ai" | "keywords";
export type Naming = "ai" | "pending" | "off";
export type Bin = "day" | "week";

/** Mirrors MAP_MIN_POINTS / ODDBALL_MAX_COS in topics_contract.rs. */
export const MAP_MIN_POINTS = 10;
export const ODDBALL_MAX_COS = 0.3;

export interface Topic {
  /** 0-based, ordered by seconds descending. */
  id: number;
  name: string;
  description: string | null;
  named_by: NamedBy;
  /** Up to 10 c-TF-IDF terms, best first. */
  keywords: string[];
  seconds: number;
  blocks: number;
  days_active: number;
  first_day: string;
  last_day: string;
  /** Up to 3 ticket keys by seconds — context only. */
  top_tickets: Ranked[];
}

/** One block on the 2-D semantic map, x/y in [0,1]. */
export interface MapPoint {
  block_id: number;
  x: number;
  y: number;
  topic_id: number;
  day: string;
  seconds: number;
  label: string;
  ticket: string | null;
  /** 1 − max cosine to any earlier block. */
  novelty: number;
  oddball: boolean;
}

/** Seconds per topic in one bin; seconds.length === topics.length. */
export interface RiverBin {
  start: string;
  seconds: number[];
}

export interface DayRow {
  day: string;
  work_seconds: number;
  /** null with fewer than 2 blocks that day. */
  focus: number | null;
  novelty: number;
  switches: number;
  longest_run_seconds: number;
  top_topic: number;
  topics_touched: number;
}

export interface Highlight {
  block_id: number;
  day: string;
  label: string;
  ticket: string | null;
  topic_id: number;
  seconds: number;
  /** Novelty for first_times; nearest-neighbour cosine for oddballs. */
  score: number;
}

export interface TopicsReport {
  status: TopicsStatus;
  from: string;
  to: string;
  naming: Naming;
  topics: Topic[];
  points: MapPoint[];
  bin: Bin;
  river: RiverBin[];
  days: DayRow[];
  /** Symmetric, diagonal 1; covers the last day_similarity.length entries of days. */
  day_similarity: number[][];
  first_times: Highlight[];
  oddballs: Highlight[];
  oddball_median: number;
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
