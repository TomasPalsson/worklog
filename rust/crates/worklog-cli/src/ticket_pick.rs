//! `worklog ticket pick` — Verdict chooses the session's Jira ticket from
//! the Owner's first prompt among their assigned tickets.

use std::io::Write;

use anyhow::Result;
use serde::Serialize;
use worklog_core::routing_contract::{Guess, RouteRule};
use worklog_core::tempo_hub_contract::TasksResponse;
use worklog_core::verdict::VerdictClassifier;

use crate::daemon_client as daemon;

/// What `ticket pick` reports: `picked` is already recorded, `likely` is Verdict's named choice.
#[derive(Debug, Default, PartialEq, Serialize)]
struct PickOutcome {
    picked: Option<String>,
    likely: Option<String>,
    confidence: Option<f64>,
}

/// Assigned tasks as `(key, summary)` plus `also` (summary = key unless already a task); deduped by key.
fn pick_candidates(tasks: Vec<(String, String)>, also: Option<&str>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (key, summary) in tasks
        .into_iter()
        .chain(also.map(|k| (k.to_owned(), k.to_owned())))
    {
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, summary));
        }
    }
    out
}

fn pick_outcome(
    candidates: &[(String, String)],
    guess: Option<Guess>,
    rule: RouteRule,
) -> PickOutcome {
    let keys: Vec<String> = candidates.iter().map(|(k, _)| k.clone()).collect();
    let Some(guess) = guess.filter(|g| keys.contains(&g.folder)) else {
        return PickOutcome::default();
    };
    let sure = worklog_core::routing::accepts(&guess, &keys, rule);
    PickOutcome {
        picked: sure.then(|| guess.folder.clone()),
        likely: Some(guess.folder),
        confidence: Some(guess.confidence),
    }
}

/// A ticket the Owner already gave this session always wins over a pick.
fn record_unless_set(db: &std::path::Path, session: &str, key: &str) -> Result<()> {
    let conn = worklog_core::db::open(db)?;
    if worklog_core::session_tickets::get(&conn, session)?.is_none() {
        worklog_core::session_tickets::set(&conn, session, key)?;
    }
    Ok(())
}

/// 5 s, under the mod's 8 s wait, so a late answer can't land after the Owner was asked.
fn verdict_classifier() -> VerdictClassifier {
    let url = std::env::var("WORKLOG_VERDICT_URL")
        .unwrap_or_else(|_| format!("http://{}", worklog_core::routing_contract::CLASSIFIER_ADDR));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    VerdictClassifier::with_client(client, url)
}

pub fn run<W: Write>(
    text: &str,
    session: &str,
    also: Option<&str>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    // Daemon down means no tasks, never a failure: the caller just asks the Owner.
    let tasks = daemon::get::<TasksResponse>("/tasks")
        .map(|r| {
            r.tasks
                .into_iter()
                .filter(|t| t.assigned)
                .map(|t| (t.key, t.summary))
                .collect()
        })
        .unwrap_or_default();
    let candidates = pick_candidates(tasks, also);
    let guess = if candidates.len() < 2 {
        None
    } else {
        verdict_classifier().pick_ticket(text, &candidates)?
    };
    let outcome = pick_outcome(
        &candidates,
        guess,
        worklog_core::daemon::configured_route_rule(),
    );
    if let Some(key) = &outcome.picked {
        record_unless_set(&worklog_core::paths::Paths::resolve()?.db, session, key)?;
    }
    if json {
        return Ok(writeln!(out, "{}", serde_json::to_string(&outcome)?)?);
    }
    let short: String = session.chars().take(8).collect();
    match (&outcome.picked, &outcome.likely, outcome.confidence) {
        (Some(key), _, Some(c)) => writeln!(out, "Session {short} is on {key} (Verdict {c:.2})")?,
        (None, Some(key), _) => writeln!(out, "No sure pick. Likeliest: {key}")?,
        _ => writeln!(out, "No sure pick.")?,
    }
    Ok(())
}

#[cfg(test)]
#[path = "ticket_pick_test.rs"]
mod tests;
