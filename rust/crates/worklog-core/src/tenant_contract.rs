//! Shared types for multi-tenant work folders (spec 005): a folder like
//! `vitinn-infra` holds many customers' tenants, so its blocks are split
//! between customers by time instead of billing the folder's one pin.
//! Owned by the spec; task code imports from here and never redeclares.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Always the lowest-priority customer: if any other customer has a clue
/// in a block, this one's clues in that block are dropped (D-01).
pub const HOUSE_CUSTOMER: &str = "APRÓ";

/// A tenant root under a multi-tenant folder, relative to the folder,
/// `/`-separated. A `*` segment matches exactly one directory name, e.g.
/// `terraform/workspaces/*`. The tenant is the segment right after it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantRoot {
    pub folder: String,
    pub root: String,
}

/// How a tenant got its customer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TenantOrigin {
    /// Tenant name matched exactly one customer's name/alias.
    Alias,
    /// The owner linked it by hand in the Billing panel.
    Link,
    /// No match and no link yet — shown in the Billing panel, never guessed.
    Unmatched,
    /// The owner marked it "not a customer" (`builds`, `uat`, …).
    Ignored,
}

/// One tenant directory found on disk under a folder's tenant roots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tenant {
    pub folder: String,
    /// Directory name, e.g. `sjukra`, `apro-prod`.
    pub name: String,
    /// `None` for `Unmatched` and `Ignored`.
    pub customer: Option<String>,
    pub origin: TenantOrigin,
}

/// Body of `POST /billing/tenants/link`. `customer: None` + `ignored: false`
/// removes the link (back to alias matching).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantLink {
    pub folder: String,
    pub tenant: String,
    #[serde(default)]
    pub customer: Option<String>,
    #[serde(default)]
    pub ignored: bool,
}

/// How strong a clue is. Derives `Ord`: a stronger clue beats a weaker one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClueStrength {
    /// Block description or Jira summary — block-wide, no timestamp.
    Summary,
    /// Branch name or worktree name.
    Branch,
    /// An edited file under `<tenant root>/<tenant>/`.
    TenantPath,
}

/// One timestamped hint that a moment of work was for `customer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clue {
    pub at: DateTime<Utc>,
    pub customer: String,
    pub strength: ClueStrength,
}

/// Where a block's split came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SplitOrigin {
    /// Computed from timestamped clues.
    Clues,
    /// The owner's saved shares.
    Manual,
    /// No timestamped clue: the summary clue, else the folder's normal
    /// resolution (pin, then text alias match).
    Fallback,
}

/// One customer's part of one block. `intervals` are `[start, end)` epoch
/// seconds inside the block; across a block's slices they never overlap and
/// sum exactly to the block's duration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerSlice {
    /// `None` only on `Fallback` when nothing resolved — the user fills it in.
    pub customer: Option<String>,
    pub intervals: Vec<(i64, i64)>,
    pub origin: SplitOrigin,
}

/// The owner's hand-set split for one block, keyed by the block's day and
/// `started_at` so it survives re-inference (blocks are rebuilt with new ids).
/// `shares` values are fractions in `(0, 1]` summing to 1 (±0.001).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomerShares {
    pub day: String,
    pub started_at: String,
    pub shares: BTreeMap<String, f64>,
}
