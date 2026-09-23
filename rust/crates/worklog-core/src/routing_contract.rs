//! Shared names and shapes for browser + Slack event routing (spec 003).
//!
//! Every task that touches routing imports from here. A type or constant
//! a task needs that is missing here is an escalation to the orchestrator,
//! never a local re-declaration.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `events.source` for one Firefox add-on heartbeat (one row per minute).
pub const SOURCE_FIREFOX: &str = "firefox";
/// `events.source` for one Slack message the user sent.
pub const SOURCE_SLACK: &str = "slack";

/// Keychain key for the Slack user token (added to `secrets::KNOWN_KEYS`).
pub const SLACK_TOKEN_KEY: &str = "slack_user_token";
/// Envfile key: work-hours window, e.g. `Mon-Fri 09:00-17:00`.
pub const WORK_HOURS_KEY: &str = "WORKLOG_WORK_HOURS";
pub const DEFAULT_WORK_HOURS: &str = "Mon-Fri 09:00-17:00";
/// Envfile key: minimum model confidence to apply a guess, e.g. `0.90`.
pub const ROUTE_THRESHOLD_KEY: &str = "WORKLOG_ROUTE_THRESHOLD";
pub const DEFAULT_ROUTE_THRESHOLD: f64 = 0.90;
/// Loopback address of the optional Laya helper process.
pub const LAYA_ADDR: &str = "127.0.0.1:9324";
/// Container whose tabs are never recorded (D-08).
pub const PERSONAL_CONTAINER: &str = "Personal";
/// Max recent fixes passed to the model as hints (FR-20).
pub const MAX_HINT_EXAMPLES: usize = 5;

/// Where an event's project label came from. Stored in
/// `events.label_origin` as the lowercase string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelOrigin {
    Rule,
    Fix,
    Guess,
}

impl LabelOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelOrigin::Rule => "rule",
            LabelOrigin::Fix => "fix",
            LabelOrigin::Guess => "guess",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rule" => Some(LabelOrigin::Rule),
            "fix" => Some(LabelOrigin::Fix),
            "guess" => Some(LabelOrigin::Guess),
            _ => None,
        }
    }
}

/// What a hard rule matches on. Stored in `routing_rules.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// URL host, lowercase, no port (`aws.tomasari.is`).
    Domain,
    /// Slack channel name as stored in `events.title`.
    SlackChannel,
    /// Firefox container name as stored in `events.container`.
    Container,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::Domain => "domain",
            RuleKind::SlackChannel => "slack_channel",
            RuleKind::Container => "container",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "domain" => Some(RuleKind::Domain),
            "slack_channel" => Some(RuleKind::SlackChannel),
            "container" => Some(RuleKind::Container),
            _ => None,
        }
    }
}

/// Body of `POST /browser/heartbeat`, sent by the add-on. Untrusted.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Heartbeat {
    pub ts: DateTime<Utc>,
    pub url: String,
    pub title: String,
    pub container: Option<String>,
    pub incognito: bool,
}

/// One row of `routing_rules`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub kind: RuleKind,
    pub pattern: String,
    pub folder: String,
    pub created_at: String,
}

/// Body of `POST /events/:id/label`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelRequest {
    /// Project key (work-root folder name or named project).
    pub folder: String,
    /// `Some(kind)` = also create an "always" rule of this kind.
    pub always: Option<RuleKind>,
}

/// A browser/Slack event as the UI sees it (`GET /days/:day/routed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedEvent {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub title: String,
    pub details: Option<String>,
    pub container: Option<String>,
    /// `None` = unsorted.
    pub folder: Option<String>,
    pub label_origin: Option<LabelOrigin>,
    pub label_confidence: Option<f64>,
}

/// The model's pick among `options`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    pub folder: String,
    pub confidence: f64,
}

/// Test seam for the model. Production: the Laya helper over HTTP.
/// `Ok(None)` = helper unreachable; the caller leaves the event unsorted.
pub trait Classifier {
    fn classify(&self, state: &Value, options: &[String]) -> Result<Option<Guess>>;
}
