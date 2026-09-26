//! Shared types for per-customer deildir, (customer, deild, %) splits and
//! the change log (spec 006). Owned by the spec; task code imports from here
//! and never redeclares. A deild is the Verkefni text sent to the invoicing
//! form — a name only, no pricing (D-01).

use serde::{Deserialize, Serialize};

use crate::tenant_contract::SplitOrigin;

/// Pop-up poll interval for the web UI (§5: change → pop-up ≤ 15 s).
pub const LIVE_POLL_SECONDS: u64 = 10;
/// Change-log rows older than this are purged (§5).
pub const CHANGE_RETENTION_DAYS: i64 = 30;
/// A split's fractions must sum to 1 within this (FR-05).
pub const SHARE_TOLERANCE: f64 = 0.001;

/// One deild under one customer. `name` is unique per customer (FR-02);
/// `keywords` are matched like customer aliases (case-insensitive, word
/// boundaries) against a block's ticket summary + description (FR-03).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Deild {
    #[serde(default)]
    pub id: Option<i64>,
    pub customer: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// How a slice got its deild — the FR-03 ladder, strongest first.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeildOrigin {
    /// A row of the Owner's saved split.
    Manual,
    /// Exactly one of the slice customer's deildir had a keyword match.
    Keyword,
    /// The folder's pinned Verkefni, and the slice's customer is the pin's.
    FolderDefault,
    /// Nothing resolved — the Owner fills it in.
    Blank,
}

/// One row of the Owner's split (D-05). A customer may repeat with
/// different deildir. `fraction` is in `(0, 1]`; a block's rows sum to 1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareRow {
    pub customer: String,
    #[serde(default)]
    pub deild: Option<String>,
    pub fraction: f64,
}

/// The Owner's hand-set split for one block, keyed by day + `started_at`
/// so it survives block rebuilds (new ids). Replaces the v1
/// `tenant_contract::CustomerShares` map; v1 rows read as `deild: None`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockShares {
    pub day: String,
    pub started_at: String,
    pub rows: Vec<ShareRow>,
}

/// One (customer, deild) part of one block — what a billing line sums.
/// `intervals` are `[start, end)` epoch seconds; across a block's slices
/// they never overlap and sum to the block's duration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BillingSlice {
    pub customer: Option<String>,
    pub deild: Option<String>,
    pub intervals: Vec<(i64, i64)>,
    pub origin: SplitOrigin,
    pub deild_origin: DeildOrigin,
}

/// What changed on a block (D-08). New/deleted blocks and durations are
/// never logged (FR-12).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeField {
    Customer,
    Deild,
    Split,
    Description,
}

/// Who made a change (D-06). Stored as the snake_case string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSource {
    /// The Claude estimator (`estimated_by = 'claude_p'`).
    Claude,
    /// The Verdict router re-labelling events.
    Verdict,
    /// Re-resolution after the Owner edited keywords, aliases or pins.
    Keyword,
    /// Block rebuild (`infer`).
    Rebuild,
    /// The Owner's own edit. Logged, never popped up live (A5).
    User,
}

/// A block's last-seen resolution, compared after every write to detect
/// changes (A3). `parts` reuses [`ShareRow`] with the resolved fractions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolutionSnapshot {
    pub day: String,
    pub started_at: String,
    pub description: Option<String>,
    pub parts: Vec<ShareRow>,
}

/// One logged change. `old`/`new` are display text: a customer or deild
/// name, a description, or a split rendered as `"Sjúkra·Rekstur 50 / APRÓ 50"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockChange {
    pub id: i64,
    pub day: String,
    pub started_at: String,
    pub field: ChangeField,
    pub old: Option<String>,
    pub new: Option<String>,
    pub source: ChangeSource,
    /// One run of one writer; the pop-up unit (D-07).
    pub batch: String,
    pub created_at: String,
    pub seen: bool,
}

/// `GET /changes` body: changes after the cursor plus one summary per
/// (batch, source) for the pop-up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeFeed {
    pub changes: Vec<BlockChange>,
    pub batches: Vec<ChangeBatch>,
    /// Highest change id returned; the next poll's `after`.
    pub cursor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeBatch {
    pub batch: String,
    pub source: ChangeSource,
    pub count: i64,
}
