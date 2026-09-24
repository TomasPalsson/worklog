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
/// Loopback address of the optional Verdict classifier helper process.
pub const CLASSIFIER_ADDR: &str = "127.0.0.1:9324";
/// Envfile key: how many times higher than the abstain score the winner
/// must be, e.g. `1.20`.
pub const ABSTAIN_MARGIN_KEY: &str = "WORKLOG_ROUTE_ABSTAIN_MARGIN";
pub const DEFAULT_ABSTAIN_MARGIN: f64 = 1.20;
/// Envfile key: how many times higher than the runner-up the winner must
/// be, e.g. `1.10`.
pub const RUNNER_UP_RATIO_KEY: &str = "WORKLOG_ROUTE_RUNNER_UP_RATIO";
pub const DEFAULT_RUNNER_UP_RATIO: f64 = 1.10;
/// Both ratios must lie in this closed range.
pub const RATIO_RANGE: (f64, f64) = (1.0, 5.0);
pub const VERDICT_GIT_URL: &str = "git+https://github.com/Heman10x-NGU/Verdict-open-jev";
pub const VERDICT_GIT_REV: &str = "30f15564821626ca5c1ad5b2638c4eb7078787dd";
pub const VERDICT_MODEL_REPO: &str = "heman10x/rlcd-modernbert-151m";
pub const VERDICT_MODEL_REVISION: &str = "8af2496eb63c7fa66d7d234e1f62629380030eb4";
/// Container whose tabs are never recorded (D-08).
pub const PERSONAL_CONTAINER: &str = "Personal";
/// Sentinel `routing_rules.folder` value meaning "dismiss on match" rather
/// than file under a real project (never a real project-key folder name).
pub const IGNORE_FOLDER: &str = "__ignore__";
/// Max recent fixes passed to the model as hints (FR-20).
pub const MAX_HINT_EXAMPLES: usize = 5;

/// Where an event's project label came from. Stored in
/// `events.label_origin` as the lowercase string.
///
/// `Link` = the event's own text named the project (an exact
/// `github.com/<org>/<key>` or `Desktop/Work/<key>` mention — `named_project`
/// in routing.rs). `Context` = the day's `claude`/`shell`/`git_reflog`
/// activity around the event's time named it (routing_context.rs), OR the
/// end-of-day absorb step filed it inside a stretch of other-source work
/// activity (routing_absorb.rs); either way `label_confidence` is always
/// `NULL`. `Dismissed` = the owner (or an `__ignore__` rule) marked the
/// event as noise — never sorted, never counted as work time; its
/// `project_path`/`label_confidence` are always `NULL`. `Noise` = the
/// absorb step's last resort: nothing labelled it and no work stretch
/// bracketed it, so it's auto-hidden the same way `Dismissed` is — but
/// still an owner rule/fix away from being re-labelled. All but `Fix`,
/// `Dismissed` and `Noise` are automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelOrigin {
    Rule,
    Link,
    Context,
    Fix,
    Guess,
    Dismissed,
    Noise,
}

impl LabelOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelOrigin::Rule => "rule",
            LabelOrigin::Link => "link",
            LabelOrigin::Context => "context",
            LabelOrigin::Fix => "fix",
            LabelOrigin::Guess => "guess",
            LabelOrigin::Dismissed => "dismissed",
            LabelOrigin::Noise => "noise",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rule" => Some(LabelOrigin::Rule),
            "link" => Some(LabelOrigin::Link),
            "context" => Some(LabelOrigin::Context),
            "fix" => Some(LabelOrigin::Fix),
            "guess" => Some(LabelOrigin::Guess),
            "dismissed" => Some(LabelOrigin::Dismissed),
            "noise" => Some(LabelOrigin::Noise),
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

/// Body of `POST /events/:id/dismiss`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DismissRequest {
    /// `Some(Domain | SlackChannel)` = also create an `__ignore__` rule of
    /// this kind. `Container` is not a valid dismiss rule kind.
    #[serde(default)]
    pub rule_kind: Option<RuleKind>,
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
    pub runner_up: f64,
    pub abstain: f64,
}
// `confidence` = the winner's probability. All three are model probabilities in 0.0–1.0.

/// The two owner-tunable ratios a guess's scores must clear to be filed.
#[derive(Debug, Clone, Copy)]
pub struct RouteRule {
    pub abstain_margin: f64,
    pub runner_up_ratio: f64,
}

/// Test seam for the model. Production: the Verdict helper over HTTP.
/// `Ok(None)` = helper unreachable; the caller leaves the event unsorted.
pub trait Classifier {
    fn classify(&self, state: &Value, options: &[String]) -> Result<Option<Guess>>;
}
