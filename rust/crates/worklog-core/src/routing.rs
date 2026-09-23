//! Routes browser/Slack `events` rows to a project folder. A hard rule wins
//! over a model guess; guesses below the confidence threshold leave the
//! event unsorted (spec 003 T005). The label lives on the event itself:
//! `project_path`/`label_origin`/`label_confidence` — keeps `infer`/`personal`/`billing` unchanged.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

use crate::billing_registry::Registry;
use crate::routing_contract::{
    Classifier, Guess, LabelOrigin, LabelRequest, RouteRule, RoutedEvent, Rule, RuleKind,
    IGNORE_FOLDER, SOURCE_FIREFOX, SOURCE_SLACK,
};
#[path = "routing_context.rs"]
mod context;
#[path = "routing_rows.rs"]
mod rows;
use rows::{events_in_window, row_from, EventRow, EVENT_COLUMNS};
pub(crate) use rows::{fetch_event, to_routed}; // shared with routing_dismiss.rs

/// Automatically-resolved events: `(id, folder, origin)`, origin `Rule`/`Link`/`Context`.
type RuleHits = Vec<(i64, String, LabelOrigin)>;

/// An event still waiting on a label: candidate folders + classifier state.
#[derive(Debug, Clone)]
pub struct Pending {
    pub event: RoutedEvent,
    pub options: Vec<String>,
    pub state: Value,
}

/// Counts from one `route_day` pass.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RouteStats {
    pub rules_applied: usize,
    pub guesses_applied: usize,
}

/// Every labellable project key: billing-registry pins + every dir under the work prefix.
pub fn project_keys(conn: &Connection) -> Result<Vec<String>> {
    let mut keys: BTreeSet<String> = crate::billing_registry::list_folders(conn)?
        .into_iter()
        .map(|f| f.folder)
        .collect();
    let dirs = crate::billing::work_prefix()
        .and_then(|p| std::fs::read_dir(p).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned));
    keys.extend(dirs);
    Ok(keys.into_iter().collect())
}

/// `folder`'s synthetic `project_path`, recoverable by `work_folder_for_path`.
fn folder_path(folder: &str) -> String {
    match crate::billing::work_prefix() {
        Some(prefix) => format!("{prefix}/{folder}"),
        None => folder.to_owned(),
    }
}

fn set_label(
    conn: &Connection,
    id: i64,
    folder: &str,
    origin: LabelOrigin,
    confidence: Option<f64>,
) -> Result<()> {
    conn.execute(
        "UPDATE events SET project_path = ?1, label_origin = ?2, label_confidence = ?3 WHERE id = ?4",
        params![folder_path(folder), origin.as_str(), confidence, id],
    )
    .context("setting event label")?;
    Ok(())
}

/// Lowercased host, no scheme/path/query/port; `None` if unparseable.
fn domain_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// Whether `row` matches a `kind`/`pattern` rule (lookup + re-apply).
fn kind_matches(kind: RuleKind, pattern: &str, row: &EventRow) -> bool {
    match kind {
        RuleKind::Domain => {
            row.source == SOURCE_FIREFOX
                && row.details.as_deref().and_then(domain_of).as_deref() == Some(pattern)
        }
        RuleKind::SlackChannel => row.source == SOURCE_SLACK && row.title == pattern,
        RuleKind::Container => row.container.as_deref() == Some(pattern),
    }
}

fn matching_rule(rules: &[Rule], row: &EventRow) -> Option<String> {
    rules
        .iter()
        .find(|rule| kind_matches(rule.kind, &rule.pattern, row))
        .map(|rule| rule.folder.clone())
}

/// An event whose details name exactly one of `options` as `github.com/<org>/<key>` or `Desktop/Work/<key>`
/// (FR-10, origin Link, never classified); zero/two-plus hits is `None`. `details` only, never `title` (attacker-set).
fn named_project(row: &EventRow, options: &[String]) -> Option<String> {
    let text = row.details.as_deref().unwrap_or("");
    let re = Regex::new(r"(?:github\.com/[^/\s]+/|Desktop/Work/)([^/|>)\s'\x60]+)").unwrap();
    let mut found = BTreeSet::new();
    for cap in re.captures_iter(text) {
        let key = cap.get(1).unwrap().as_str();
        if options.iter().any(|o| o == key) {
            found.insert(key);
        }
    }
    let mut hits = found.into_iter();
    match (hits.next(), hits.next()) {
        (Some(key), None) => Some(key.to_owned()),
        _ => None,
    }
}

/// A container naming exactly one customer narrows to that customer's pinned folders (B10); otherwise every project key is a candidate.
fn narrowed_options(registry: &Registry, row: &EventRow, all: &[String]) -> Vec<String> {
    if let Some(container) = row.container.as_deref() {
        if let Some(customer) = registry.customer_in_text(container) {
            return registry
                .folders
                .iter()
                .filter(|f| f.customer.as_deref() == Some(customer.as_str()))
                .map(|f| f.folder.clone())
                .collect();
        }
    }
    all.to_vec()
}

/// Load a day's unlabelled browser/Slack events: rule hits (resolved) and events still needing a model decision.
pub fn load_pending(conn: &Connection, day: NaiveDate) -> Result<(RuleHits, Vec<Pending>)> {
    let rows = events_in_window(conn, day, true)?;
    let rules = list_rules(conn)?;
    let registry = Registry::load(conn)?;
    let all_options = project_keys(conn)?;
    let ctx_events = context::context_events_for_day(conn, day)?;

    let mut rule_hits = Vec::new();
    let mut pending = Vec::new();

    for row in rows {
        if let Some(folder) = matching_rule(&rules, &row) {
            rule_hits.push((row.id, folder, LabelOrigin::Rule));
            continue;
        }
        if let Some(folder) = named_project(&row, &all_options) {
            rule_hits.push((row.id, folder, LabelOrigin::Link));
            continue;
        }
        let options = narrowed_options(&registry, &row, &all_options);
        if row.source == SOURCE_SLACK {
            if let Some(folder) = context::context_hit(&row.started_at, &options, &ctx_events) {
                rule_hits.push((row.id, folder, LabelOrigin::Context));
                continue;
            }
        }
        let state = serde_json::json!({
            "source": row.source,
            "title": row.title,
            "details": row.details,
            "container": row.container,
        });
        let event = to_routed(row);
        pending.push(Pending {
            event,
            options,
            state,
        });
    }
    Ok((rule_hits, pending))
}

/// Ask the classifier for each pending event; keep guesses that name one of the event's own
/// options and clear both the abstain margin and the runner-up ratio (spec 004 FR-01/FR-02),
/// never a single raw-confidence threshold. No connection arg — the slow model call must never hold the sqlite lock.
pub fn decide(
    pending: &[Pending],
    classifier: &dyn Classifier,
    rule: RouteRule,
) -> Vec<(i64, Guess)> {
    pending
        .iter()
        .filter_map(|p| {
            let guess = classifier.classify(&p.state, &p.options).ok().flatten()?;
            let accepted = p.options.contains(&guess.folder)
                && guess.confidence >= guess.abstain * rule.abstain_margin
                && guess.confidence >= guess.runner_up * rule.runner_up_ratio;
            accepted.then_some((p.event.id, guess))
        })
        .collect()
}

/// Persist rule hits (each with its own origin) and accepted guesses.
pub fn commit_labels(
    conn: &Connection,
    rule_hits: &[(i64, String, LabelOrigin)],
    guesses: &[(i64, Guess)],
) -> Result<RouteStats> {
    for (id, folder, origin) in rule_hits {
        if folder == IGNORE_FOLDER {
            crate::routing_dismiss::set_dismissed(conn, *id)?;
        } else {
            set_label(conn, *id, folder, *origin, None)?;
        }
    }
    for (id, guess) in guesses {
        set_label(
            conn,
            *id,
            &guess.folder,
            LabelOrigin::Guess,
            Some(guess.confidence),
        )?;
    }
    Ok(RouteStats {
        rules_applied: rule_hits.len(),
        guesses_applied: guesses.len(),
    })
}

/// CLI convenience: `load_pending` (locked) → `decide` (unlocked) → `commit_labels` (locked), mirroring `estimate::prepare/invoke/commit`.
pub fn route_day(
    conn: &Connection,
    day: NaiveDate,
    classifier: &dyn Classifier,
    rule: RouteRule,
) -> Result<RouteStats> {
    let (rule_hits, pending) = load_pending(conn, day)?;
    let guesses = decide(&pending, classifier, rule);
    commit_labels(conn, &rule_hits, &guesses)
}

/// What a hard rule of `kind` keys on, read off `row`. `kind` must match `row.source` (Domain⇔firefox,
/// SlackChannel⇔slack) — `kind_matches` re-checks this at apply-time too, so a wrong-source rule can't misfire.
pub(crate) fn rule_pattern(row: &EventRow, kind: RuleKind) -> Result<String> {
    match kind {
        RuleKind::Domain => {
            if row.source != SOURCE_FIREFOX {
                anyhow::bail!("domain rules only apply to firefox events");
            }
            row.details
                .as_deref()
                .and_then(domain_of)
                .ok_or_else(|| anyhow::anyhow!("event has no URL to key a domain rule on"))
        }
        RuleKind::SlackChannel => {
            if row.source != SOURCE_SLACK {
                anyhow::bail!("slack_channel rules only apply to slack events");
            }
            Ok(row.title.clone())
        }
        RuleKind::Container => row
            .container
            .clone()
            .ok_or_else(|| anyhow::anyhow!("event has no container to key a rule on")),
    }
}

pub(crate) fn upsert_rule(
    conn: &Connection,
    kind: RuleKind,
    pattern: &str,
    folder: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO routing_rules (kind, pattern, folder) VALUES (?1, ?2, ?3)
         ON CONFLICT(kind, pattern) DO UPDATE SET folder = excluded.folder",
        params![kind.as_str(), pattern, folder],
    )
    .context("upserting routing rule")?;
    Ok(())
}

/// Apply a fresh or edited "always" rule to every other automatically-labelled event that
/// matches it (B9); hand-fixed/dismissed events keep their label. `IGNORE_FOLDER` dismisses
/// instead of filing. ponytail: full unresolved-event scan, not source-scoped — fine at single-user scale.
pub(crate) fn apply_rule_to_existing(
    conn: &Connection,
    kind: RuleKind,
    pattern: &str,
    exclude_id: i64,
) -> Result<()> {
    let sql = format!(
        "SELECT {EVENT_COLUMNS} FROM events
          WHERE id != ?1 AND (label_origin IS NULL OR label_origin IN ('guess', 'rule', 'link', 'context'))"
    );
    let mut stmt = conn.prepare(&sql)?;
    let candidates = stmt
        .query_map(params![exclude_id], row_from)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rules = list_rules(conn)?;
    for row in candidates {
        if !kind_matches(kind, pattern, &row) {
            continue;
        }
        if let Some(folder) = matching_rule(&rules, &row) {
            if folder == IGNORE_FOLDER {
                crate::routing_dismiss::set_dismissed(conn, row.id)?;
            } else {
                set_label(conn, row.id, &folder, LabelOrigin::Rule, None)?;
            }
        }
    }
    Ok(())
}

/// Label one event by hand; `always` also creates and applies a hard rule.
pub fn label_event(conn: &Connection, id: i64, req: &LabelRequest) -> Result<RoutedEvent> {
    let folder = req.folder.trim();
    if folder.is_empty() || !project_keys(conn)?.iter().any(|k| k == folder) {
        anyhow::bail!("Project no longer exists");
    }
    let row = fetch_event(conn, id)?.ok_or_else(|| anyhow::anyhow!("event {id} not found"))?;

    // Resolve the rule pattern before writing anything — a failure here must
    // leave the event untouched, not the label committed with the rule missing.
    let rule = req
        .always
        .map(|kind| rule_pattern(&row, kind).map(|pattern| (kind, pattern)))
        .transpose()?;

    set_label(conn, id, folder, LabelOrigin::Fix, None)?;

    if let Some((kind, pattern)) = rule {
        upsert_rule(conn, kind, &pattern, folder)?;
        apply_rule_to_existing(conn, kind, &pattern, id)?;
    }

    fetch_event(conn, id)?
        .map(to_routed)
        .ok_or_else(|| anyhow::anyhow!("event {id} vanished after labelling"))
}

pub fn list_rules(conn: &Connection) -> Result<Vec<Rule>> {
    let mut stmt = conn
        .prepare("SELECT id, kind, pattern, folder, created_at FROM routing_rules ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, kind, pattern, folder, created_at) = row?;
        let Some(kind) = RuleKind::parse(&kind) else {
            tracing::warn!(rule_id = id, kind = %kind, "unknown routing_rules.kind; skipping");
            continue;
        };
        out.push(Rule {
            id,
            kind,
            pattern,
            folder,
            created_at,
        });
    }
    Ok(out)
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn
        .execute("DELETE FROM routing_rules WHERE id = ?1", params![id])
        .context("delete_rule")?;
    Ok(n > 0)
}

/// Every browser/Slack event for `day`, as the review UI sees it (dismissed excluded).
pub fn routed_for_day(conn: &Connection, day: NaiveDate) -> Result<Vec<RoutedEvent>> {
    Ok(events_in_window(conn, day, false)?
        .into_iter()
        .map(to_routed)
        .filter(|e| e.label_origin != Some(LabelOrigin::Dismissed))
        .collect())
}

// Tests live in routing_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "routing_test.rs"]
mod tests;
