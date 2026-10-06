//! Shared types for block ticket origin and stored Tempo ticket lines
//! (spec 011). Owned by the spec; task code imports from here and never
//! redeclares. A ticket line is one day's non-personal blocks sharing one
//! `jira_issue` — exactly one Tempo worklog.

use serde::{Deserialize, Serialize};

use crate::clues_contract::LineTextOrigin;
use crate::verdict_contract::LineCheck;

/// Tempo granularity; an hours override is a positive multiple of this.
pub const HALF_HOUR_SECONDS: i64 = 1800;

/// `blocks.ticket_origin` — who set the block's `jira_issue`. NULL in the
/// DB (pre-spec rows) reads as `None` and behaves like `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketOrigin {
    /// The one Jira key the block's events share (infer).
    Event,
    /// Picked by the estimator from cached open tickets.
    Auto,
    /// Set or cleared by the Owner. Never changed by infer or estimate.
    Manual,
}

impl TicketOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            TicketOrigin::Event => "event",
            TicketOrigin::Auto => "auto",
            TicketOrigin::Manual => "manual",
        }
    }

    /// Unknown strings read as `None` (treated like `Auto`).
    pub fn parse(s: &str) -> Option<TicketOrigin> {
        match s {
            "event" => Some(TicketOrigin::Event),
            "auto" => Some(TicketOrigin::Auto),
            "manual" => Some(TicketOrigin::Manual),
            _ => None,
        }
    }
}

/// True when infer/estimate must leave the block's `jira_issue` alone.
pub fn ticket_locked(origin: Option<TicketOrigin>) -> bool {
    origin == Some(TicketOrigin::Manual)
}

/// Primary key of `tempo_line_texts`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TempoLineKey {
    pub day: String,
    pub jira_issue: String,
}

/// One ticket line as the UI shows it and sync sends it.
/// `effective_seconds` = `hours_override_seconds` if set, else
/// `union_seconds`. `text` is the stored row (generated or manual);
/// `None` = not generated yet, and the UI shows `fallback_text` (the
/// no-LLM summary of the block descriptions). Sync sends `text`; when it
/// is `None`, sync generates, stores, then sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoLine {
    pub day: String,
    pub jira_issue: String,
    pub text: Option<String>,
    pub text_origin: Option<LineTextOrigin>,
    pub fallback_text: String,
    /// Union of the line's block intervals, `round_to_half_hour`-ed.
    pub union_seconds: i64,
    pub hours_override_seconds: Option<i64>,
    pub effective_seconds: i64,
    #[serde(default)]
    pub check_status: Option<LineCheck>,
    #[serde(default)]
    pub billing: Option<LineBilling>,
}

/// Body of `POST /tempo/lines/text`. Empty/blank `text` deletes the
/// stored row (back to generated-or-fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTempoLineText {
    pub day: String,
    pub jira_issue: String,
    pub text: String,
}

/// Body of `POST /tempo/lines/hours`. `None` clears the override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTempoLineHours {
    pub day: String,
    pub jira_issue: String,
    pub seconds: Option<i64>,
}

/// How a ticket line counts toward the 70% billable goal (from Mirres).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingClass {
    /// Mirres `billable` = true (Reikningshæft).
    Billable,
    /// Not billable, but `included_hours.counts_as_billed` = true.
    Included,
    NotBillable,
}

/// Mirres facts for one ticket line. `None` on `TempoLine` = not fetched,
/// or the ticket has no Tempo Account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineBilling {
    pub account_key: String,
    /// "<customer.short_name> · <project_name>", or None if Mirres had no project.
    pub project: Option<String>,
    /// Mirres `project_type`, e.g. "Útseld vinna".
    pub project_type: Option<String>,
    pub class: BillingClass,
    /// One short Icelandic warning, or None.
    pub warning: Option<String>,
}
