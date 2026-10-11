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
pub const MAX_TOPICS: usize = 8;
/// A ticket needs this many blocks in range to get a sprawl score.
pub const SPRAWL_MIN_BLOCKS: usize = 3;
/// Rows returned in `sprawl` and `similar_days`.
pub const SPRAWL_TOP: usize = 3;
pub const SIMILAR_DAYS_TOP: usize = 3;

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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Topic {
    /// 0-based, ordered by `seconds` descending.
    pub id: usize,
    /// Summary of the ticket with the most hours in this topic, else the
    /// description of the topic's longest block.
    pub name: String,
    /// Key of the ticket `name` came from, if any.
    pub name_ticket: Option<String>,
    pub seconds: i64,
    pub blocks: i64,
    /// Up to 3 tickets by seconds inside this topic.
    pub top_tickets: Vec<Ranked>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DaySwitches {
    pub day: String,
    /// Consecutive work blocks (by started_at) whose topics differ.
    pub switches: i64,
    pub work_seconds: i64,
    /// Longest wall-clock run of consecutive blocks in one topic.
    pub longest_run_seconds: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicDrift {
    pub topic_id: usize,
    pub name: String,
    pub seconds_now: i64,
    /// Seconds of the previous equal-length range assigned to this topic.
    pub seconds_before: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TicketSprawl {
    pub ticket: String,
    pub summary: Option<String>,
    pub blocks: i64,
    /// 1 − mean pairwise cosine similarity of the ticket's blocks (0 = all alike).
    pub spread: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SimilarDay {
    pub day: String,
    /// Cosine similarity of hour-weighted day centroids.
    pub similarity: f64,
    pub work_seconds: i64,
    /// That day's tickets by seconds, up to 3.
    pub top_tickets: Vec<Ranked>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicsReport {
    pub status: TopicsStatus,
    pub from: String,
    pub to: String,
    pub topics: Vec<Topic>,
    /// One row per day in range with work blocks, oldest first.
    pub switches: Vec<DaySwitches>,
    /// One row per topic, same order as `topics`.
    pub drift: Vec<TopicDrift>,
    /// Up to SPRAWL_TOP tickets, widest spread first.
    pub sprawl: Vec<TicketSprawl>,
    /// Days before `to` most like `to` (the "today" of the range), best first.
    pub similar_days: Vec<SimilarDay>,
    /// Work blocks in range with a vector.
    pub embedded_blocks: i64,
    /// Work blocks in range with no text to embed.
    pub skipped_blocks: i64,
}

impl TopicsReport {
    /// The empty report for a non-Ready status.
    pub fn empty(status: TopicsStatus, from: &str, to: &str) -> Self {
        Self {
            status,
            from: from.to_string(),
            to: to.to_string(),
            topics: vec![],
            switches: vec![],
            drift: vec![],
            sprawl: vec![],
            similar_days: vec![],
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
