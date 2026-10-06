//! The scorecard (spec 017 FR-22, FR-23): replays the last 30 days of
//! project and ticket decisions through Verdict, counts right / wrong /
//! unsure against the final value, and picks the safest pair of thresholds.
//! Split into load / replay / score / save so the slow model calls and the
//! grid search run without the sqlite lock.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::purge::{meta_get, meta_set};
use crate::routing::{decide, Pending};
use crate::routing_contract::{
    Classifier, RouteRule, RoutedEvent, ABSTAIN_MARGIN_KEY, RUNNER_UP_RATIO_KEY,
};
use crate::verdict_contract::{
    DecisionKind, DecisionRow, DecisionSource, LineCheck, Ranking, SCORECARD_DAYS,
};
use crate::verdict_decisions;
use crate::{line_check, verdict};

/// `meta` key holding the summary line of the latest scorecard.
const LAST_KEY: &str = "scorecard_last";
/// Both thresholds run 1.00..=2.00 in steps of 0.05 (FR-23).
const GRID_STEPS: u32 = 20;

/// One logged Verdict decision with the value it should have ended on.
pub struct Case {
    kind: DecisionKind,
    pending: Pending,
    final_value: Option<String>,
}

/// Cases Verdict answered, with its ranking and how long each call took.
#[derive(Default)]
pub struct Replayed {
    answered: Vec<(Case, Ranking)>,
    skipped: usize,
    millis: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub right: usize,
    pub wrong: usize,
    pub unsure: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scorecard {
    pub project: Tally,
    pub ticket: Tally,
    /// Decisions Verdict could not be asked about; left out of every count.
    pub skipped: usize,
    pub p95_ms: u64,
    /// (abstain margin, runner-up ratio) with zero wrong and the most right.
    pub tuned: Option<(f64, f64)>,
    pub applied: bool,
}

impl Scorecard {
    /// Verdict could not be asked about anything; there is nothing to keep.
    pub fn nothing_answered(&self) -> bool {
        let total = |t: &Tally| t.right + t.wrong + t.unsure;
        self.skipped > 0 && total(&self.project) + total(&self.ticket) == 0
    }

    pub fn summary(&self) -> String {
        let t = |t: &Tally| format!("{} right, {} wrong, {} unsure", t.right, t.wrong, t.unsure);
        let thresholds = match self.tuned {
            Some((m, r)) if self.applied => format!("saved {m:.2}/{r:.2}"),
            Some((m, r)) => format!("best {m:.2}/{r:.2} (not saved)"),
            None => "kept current thresholds (no pair with zero wrong)".to_owned(),
        };
        let skipped = match self.skipped {
            0 => String::new(),
            n => format!("; {n} not replayed"),
        };
        format!(
            "project {}; ticket {}; p95 {} ms{skipped}; {thresholds}",
            t(&self.project),
            t(&self.ticket),
            self.p95_ms
        )
    }
}

/// Tempo lines with their ticket summary and whether the line check should pass them.
const LINE_FIXTURE: [(&str, &str, bool); 10] = [
    (
        "Fixed the redirect loop after login in auth middleware",
        "Fix login redirect",
        true,
    ),
    (
        "Added retry with backoff to the Jira collector",
        "Jira collector retries",
        true,
    ),
    (
        "Reviewed the invoice export PR and fixed the rounding bug",
        "Invoice export rounding",
        true,
    ),
    (
        "Wrote migration for the verdict_decisions table",
        "Verdict decision log",
        true,
    ),
    (
        "Sprint planning meeting: scoped the billing deild work",
        "Billing deild split",
        true,
    ),
    ("Worked on stuff", "Fix login redirect", false),
    ("Various tasks", "Jira collector retries", false),
    ("Development", "Invoice export rounding", false),
    ("Meetings and other work", "Verdict decision log", false),
    ("Misc", "Billing deild split", false),
];

/// FR-19: how many fixture lines `line_check` judges as expected. `matcher` is
/// `verdict::match_texts` in production.
pub fn line_fixture<F>(matcher: F) -> String
where
    F: Fn(&str, &[String]) -> Result<Vec<bool>>,
{
    let mut right = 0;
    for (text, summary, good) in LINE_FIXTURE {
        match line_check::check(&matcher, text, summary) {
            Ok(Some(found)) => right += usize::from((found == LineCheck::Passed) == good),
            _ => return "line check fixture: skipped (Verdict not answering)".to_owned(),
        }
    }
    format!("line check fixture: {right}/{} right", LINE_FIXTURE.len())
}

/// [`line_fixture`] against the running Verdict helper.
pub fn live_line_fixture() -> String {
    line_fixture(verdict::match_texts)
}

pub fn run(conn: &Connection, classifier: &dyn Classifier, apply: bool) -> Result<Scorecard> {
    let rule = crate::daemon::configured_route_rule();
    let replayed = replay(load(conn, Utc::now())?, classifier);
    if !apply {
        return Ok(score(replayed, rule, false));
    }
    finish(conn, replayed, rule, apply, None)
}

pub fn finish(
    conn: &Connection,
    replayed: Replayed,
    rule: RouteRule,
    apply: bool,
    fixture: Option<&str>,
) -> Result<Scorecard> {
    let card = score(replayed, rule, apply);
    if !card.nothing_answered() {
        save(conn, &card, fixture)?;
    }
    Ok(card)
}

/// The summary line of the latest saved scorecard.
pub fn last_summary(conn: &Connection) -> Result<Option<String>> {
    meta_get(conn, LAST_KEY)
}

/// Verdict decisions of the window ending at `now`, each with its final value:
/// the Owner's latest correction, else what was applied.
pub fn load(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<Case>> {
    let since = (now - Duration::days(SCORECARD_DAYS)).to_rfc3339();
    let mut cases = Vec::new();
    for kind in [DecisionKind::Project, DecisionKind::Ticket] {
        let (owner, verdict): (Vec<_>, Vec<_>) = verdict_decisions::list_since(conn, kind, &since)?
            .into_iter()
            .partition(|r| r.source == DecisionSource::Owner);
        // Oldest first, so a later correction replaces an earlier one.
        let corrections: HashMap<String, Option<String>> =
            owner.into_iter().map(|r| (r.subject, r.chosen)).collect();
        for row in verdict {
            let final_value = corrections
                .get(&row.subject)
                .cloned()
                .unwrap_or_else(|| row.chosen.clone());
            cases.push(Case {
                kind,
                final_value,
                pending: pending_for(row)?,
            });
        }
    }
    Ok(cases)
}

fn pending_for(row: DecisionRow) -> Result<Pending> {
    Ok(Pending {
        event: RoutedEvent {
            id: 0,
            source: "scorecard".into(),
            started_at: row.decided_at,
            title: String::new(),
            details: None,
            container: None,
            folder: None,
            label_origin: None,
            label_confidence: None,
            ranking: None,
        },
        options: row.options,
        state: serde_json::from_str::<Value>(&row.state_json).context("logged state")?,
        // Examples are not logged, so a replay shows none.
        examples: BTreeMap::new(),
    })
}

pub fn replay(cases: Vec<Case>, classifier: &dyn Classifier) -> Replayed {
    let mut out = Replayed::default();
    for case in cases {
        let p = &case.pending;
        let started = Instant::now();
        let ranking = classifier
            .classify(&p.state, &p.options, &p.examples)
            .ok()
            .flatten();
        let ms = started.elapsed().as_millis() as u64;
        match ranking {
            Some(r) => {
                out.millis.push(ms);
                out.answered.push((case, r));
            }
            None => out.skipped += 1,
        }
    }
    out
}

struct Recorded<'a>(&'a Ranking);

impl Classifier for Recorded<'_> {
    fn classify(
        &self,
        _state: &Value,
        _options: &[String],
        _examples: &BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(Some(self.0.clone()))
    }
}

/// Re-asks `routing::decide` with the recorded ranking, so the filing rule
/// is the one production uses.
fn tally<'a>(rows: impl Iterator<Item = &'a (Case, Ranking)>, rule: RouteRule) -> Tally {
    let mut t = Tally::default();
    for (case, ranking) in rows {
        let answer = decide(
            std::slice::from_ref(&case.pending),
            &Recorded(ranking),
            rule,
        )
        .pop();
        match answer.and_then(|a| a.guess) {
            None => t.unsure += 1,
            Some(g) if case.final_value.as_deref() == Some(g.folder.as_str()) => t.right += 1,
            Some(_) => t.wrong += 1,
        }
    }
    t
}

/// Among pairs with zero wrong, the one with the most right; ties go to the
/// stricter pair.
fn tune(answered: &[(Case, Ranking)]) -> Option<(f64, f64)> {
    if answered.is_empty() {
        return None;
    }
    let step = |i: u32| f64::from(100 + 5 * i) / 100.0;
    let mut best: Option<(usize, (f64, f64))> = None;
    for m in (0..=GRID_STEPS).rev() {
        for r in (0..=GRID_STEPS).rev() {
            let pair = (step(m), step(r));
            let rule = RouteRule {
                abstain_margin: pair.0,
                runner_up_ratio: pair.1,
            };
            let t = tally(answered.iter(), rule);
            if t.wrong == 0 && t.right > 0 && best.is_none_or(|(right, _)| t.right > right) {
                best = Some((t.right, pair));
            }
        }
    }
    best.map(|(_, pair)| pair)
}

/// Nearest-rank 95th percentile; 0 for no calls.
fn p95(mut millis: Vec<u64>) -> u64 {
    millis.sort_unstable();
    match (millis.len() * 95).div_ceil(100) {
        0 => 0,
        rank => millis[rank - 1],
    }
}

pub fn score(replayed: Replayed, rule: RouteRule, apply: bool) -> Scorecard {
    let of = |kind| {
        tally(
            replayed.answered.iter().filter(|(c, _)| c.kind == kind),
            rule,
        )
    };
    let tuned = tune(&replayed.answered);
    Scorecard {
        project: of(DecisionKind::Project),
        ticket: of(DecisionKind::Ticket),
        skipped: replayed.skipped,
        p95_ms: p95(replayed.millis),
        tuned,
        applied: apply && tuned.is_some(),
    }
}

/// Writes the tuned thresholds when `card.applied`, and keeps the summary line
/// with the line-check fixture result appended when there is one.
pub fn save(conn: &Connection, card: &Scorecard, fixture: Option<&str>) -> Result<()> {
    if let (true, Some((margin, ratio))) = (card.applied, card.tuned) {
        crate::envfile::upsert(ABSTAIN_MARGIN_KEY, &format!("{margin:.2}"))?;
        crate::envfile::upsert(RUNNER_UP_RATIO_KEY, &format!("{ratio:.2}"))?;
    }
    let line = match fixture {
        Some(fixture) => format!("{}; {fixture}", card.summary()),
        None => card.summary(),
    };
    meta_set(conn, LAST_KEY, &line)
}

#[cfg(test)]
#[path = "scorecard_test.rs"]
mod tests;
