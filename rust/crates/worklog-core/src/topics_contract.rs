//! JSON contract for GET /stats/topics?from&to and the classifier
//! helper's POST /embed. Mirrors web/lib/topics_contract.ts field for
//! field (snake_case); change both together. Every number is computed in
//! `stats_topics.rs`; the web only draws. Topic output is display-only:
//! nothing here is ever written to a block, ticket, customer or deild.

use serde::{Deserialize, Serialize};

pub use crate::stats_contract::Ranked;

/// Hugging Face repo of the embedding model the helper loads.
pub const EMBED_MODEL: &str = "google/embeddinggemma-2";
/// Pinned revision, so every install embeds with the same weights.
pub const EMBED_MODEL_REVISION: &str = "914f7f89142e33e77833254d9c9b90c3cef7303b";
/// Length of every vector /embed returns (unit length, float32).
pub const EMBED_DIM: usize = 768;
/// Texts per POST /embed. The helper is single-threaded; small batches
/// keep /classify calls queued behind an embed under their 10 s timeout.
pub const EMBED_BATCH: usize = 32;
/// Prompts appended to a block's description when building its text.
pub const PROMPTS_PER_BLOCK: usize = 5;
/// Each prompt is cut to this many chars.
pub const PROMPT_CHARS: usize = 300;
/// Fewer embedded work blocks than this in range → `TooFewBlocks`.
pub const MIN_BLOCKS: usize = 6;
/// Topic count bounds; k = clamp(round(sqrt(n / 2)), MIN_TOPICS, MAX_TOPICS).
pub const MIN_TOPICS: usize = 2;
pub const MAX_TOPICS: usize = 10;
/// Fewer embedded blocks than this in range → `points` is empty (no map).
pub const MAP_MIN_POINTS: usize = 10;
/// A block is an oddball when its closest other block in range is below this
/// cosine (on centered vectors).
pub const ODDBALL_MAX_COS: f64 = 0.30;
/// Rows in `first_times` and `oddballs`.
pub const HIGHLIGHTS_TOP: usize = 5;
/// `day_similarity` covers at most this many most-recent days.
pub const MIRROR_MAX_DAYS: usize = 62;
/// Ranges longer than this many days bin the river by week instead of day.
pub const RIVER_WEEK_AFTER_DAYS: i64 = 90;
/// c-TF-IDF keywords kept per topic, and representative items sent to the namer.
pub const TOPIC_KEYWORDS: usize = 10;
pub const NAMING_EXAMPLES: usize = 5;
/// A cached AI name is reused for a new member set whose Jaccard overlap with
/// the cached set is at least this.
pub const NAME_REUSE_JACCARD: f64 = 0.6;

/// Body of POST /embed on the classifier helper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbedRequest {
    pub texts: Vec<String>,
}

/// Reply of POST /embed: one unit-length EMBED_DIM vector per input text,
/// same order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbedResponse {
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopicsStatus {
    /// Topics computed; every list below may be read.
    Ready,
    /// The classifier helper is off, down or errored. Lists are empty.
    HelperDown,
    /// Fewer than MIN_BLOCKS embedded work blocks in range. Lists are empty.
    TooFewBlocks,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NamedBy {
    /// Name and description written by the AI namer (cached in `topic_names`).
    Ai,
    /// Fallback: top c-TF-IDF keywords joined with " · "; `description` is None.
    Keywords,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Naming {
    /// Every topic has an AI name.
    Ai,
    /// Some topics show keyword names while a background naming job runs.
    Pending,
    /// No model is configured; keyword names stay.
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Bin {
    Day,
    Week,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Topic {
    /// 0-based, ordered by `seconds` descending.
    pub id: usize,
    pub name: String,
    pub description: Option<String>,
    pub named_by: NamedBy,
    /// Up to TOPIC_KEYWORDS c-TF-IDF terms, best first.
    pub keywords: Vec<String>,
    pub seconds: i64,
    pub blocks: i64,
    /// Distinct days in range with a block in this topic.
    pub days_active: i64,
    pub first_day: String,
    pub last_day: String,
    /// Up to 3 ticket keys by seconds inside this topic — context only.
    pub top_tickets: Vec<Ranked>,
}

/// One block on the 2-D semantic map (t-SNE of centered vectors, in [0,1]²).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MapPoint {
    pub block_id: i64,
    pub x: f32,
    pub y: f32,
    pub topic_id: usize,
    pub day: String,
    pub seconds: i64,
    /// Block description, at most 90 chars.
    pub label: String,
    pub ticket: Option<String>,
    /// 1 − max cosine to any earlier block (history + range).
    pub novelty: f32,
    pub oddball: bool,
}

/// Seconds per topic in one river bin; `seconds.len() == topics.len()`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RiverBin {
    /// YYYY-MM-DD of the bin's first day.
    pub start: String,
    pub seconds: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DayRow {
    pub day: String,
    pub work_seconds: i64,
    /// Hours-weighted mean pairwise cosine of the day's blocks; None with < 2 blocks.
    pub focus: Option<f64>,
    /// Hours-weighted mean novelty of the day's blocks.
    pub novelty: f64,
    /// Consecutive blocks (by started_at) whose topics differ.
    pub switches: i64,
    pub longest_run_seconds: i64,
    /// Topic with the most seconds that day.
    pub top_topic: usize,
    pub topics_touched: i64,
}

/// A notable block: a first time (highest novelty) or an oddball (lowest
/// nearest-neighbour cosine).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Highlight {
    pub block_id: i64,
    pub day: String,
    pub label: String,
    pub ticket: Option<String>,
    pub topic_id: usize,
    pub seconds: i64,
    /// Novelty for first_times; nearest-neighbour cosine for oddballs.
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicsReport {
    pub status: TopicsStatus,
    pub from: String,
    pub to: String,
    pub naming: Naming,
    pub topics: Vec<Topic>,
    /// Empty when fewer than MAP_MIN_POINTS embedded blocks.
    pub points: Vec<MapPoint>,
    pub bin: Bin,
    /// Oldest first; every bin in range, zeros included.
    pub river: Vec<RiverBin>,
    /// One row per day with work blocks, oldest first.
    pub days: Vec<DayRow>,
    /// Symmetric, diagonal 1.0; rows/cols = the last min(days, MIRROR_MAX_DAYS)
    /// entries of `days`.
    pub day_similarity: Vec<Vec<f64>>,
    /// Up to HIGHLIGHTS_TOP, highest novelty first.
    pub first_times: Vec<Highlight>,
    /// Up to HIGHLIGHTS_TOP oddball blocks, lowest nearest cosine first.
    pub oddballs: Vec<Highlight>,
    /// Median over blocks in range of the nearest-other-block cosine.
    pub oddball_median: f64,
    pub embedded_blocks: i64,
    pub skipped_blocks: i64,
}

impl TopicsReport {
    /// The empty report for a non-Ready status.
    pub fn empty(status: TopicsStatus, from: &str, to: &str) -> Self {
        Self {
            status,
            from: from.to_string(),
            to: to.to_string(),
            naming: Naming::Off,
            topics: vec![],
            points: vec![],
            bin: Bin::Day,
            river: vec![],
            days: vec![],
            day_similarity: vec![],
            first_times: vec![],
            oddballs: vec![],
            oddball_median: 0.0,
            embedded_blocks: 0,
            skipped_blocks: 0,
        }
    }
}

/// One host in GET /days/:day/noise-hosts.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NoiseHost {
    /// Lowercased hostname, as `routing::domain_of` returns it.
    pub host: String,
    /// Past firefox events on this host the user dismissed.
    pub dismissed_before: i64,
}

/// Body of GET /days/:day/noise-hosts.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NoiseHosts {
    pub hosts: Vec<NoiseHost>,
}

/// A host is noise when it has at least this many labelled firefox events
/// on days before `day` and every one of them is `dismissed`.
pub const NOISE_MIN_DISMISSED: i64 = 2;
