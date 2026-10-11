//! Standup draft (FR-18, FR-19, FR-24): yesterday's and today's blocks,
//! merged PRs, open tickets and each Claude session's first prompt go to
//! the model, secrets removed first, and come back as three answers.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::clues_contract::RawRecord;
use crate::daily_helpers_contract::StandupDraft;
use crate::estimate::ModelInvoker;
use crate::line_text::LINE_TEXT_MODEL;
use crate::models::Block;
use crate::time_leaks_contract::{FocusRun, FocusSummary, FOCUS_MAX_GAP_SECONDS};
use crate::{repo, scrub, ticket_activity};

const PROMPT_CHARS: usize = 300;
const DESCRIPTION_CHARS: usize = 400;

const SYSTEM_PROMPT: &str = "You write a developer's morning standup for the team's Slack thread, \
in English. Sound like a person talking: casual, plain words, short. Never write ticket keys or \
numbers (like ABC-123); say what the work is about instead, using the ticket summary and \
description. No code, no secrets.\n\
Answer three questions, each as one or two short sentences. Start each answer with the verb, \
like notes: \"Investigating the Code Interpreter errors.\", \"Working on the Vitinn infra.\" \
Never open with \"Today I'm\", \"I'm\", \"Next up is\" or similar lead-ins.\n\
- today: what they are working on today.\n\
- next: real upcoming work only, meaning open tickets in a to-do or in-progress status that \
today's answer does not already cover. A ticket in Verification, review, QA or testing is \
waiting on someone else: it is not upcoming work, so leave it out. If nothing is lined up, say \
so plainly with a shrug emoji (for example \"Nothing lined up yet 🤷\"). That is a good answer; \
never pad it.\n\
- blockers: anything stopping them; if none, \"No blockers.\"\n\
If yesterday had no recorded work, say so plainly in the first answer and draw it from the open \
tickets only. Reply with JSON {\"today\": [..], \"next\": [..], \"blockers\": [..]}.";

const REWORD_INSTRUCTION: &str =
    "Rewrite the previous draft below with different wording. Keep the facts.";

pub fn draft(
    conn: &Connection,
    today: NaiveDate,
    model: &dyn ModelInvoker,
) -> Result<StandupDraft> {
    ask(model, &evidence(conn, today)?)
}

pub fn regenerate(
    conn: &Connection,
    today: NaiveDate,
    model: &dyn ModelInvoker,
    previous: &StandupDraft,
) -> Result<StandupDraft> {
    let message = format!(
        "{}\n{REWORD_INSTRUCTION}\n{}",
        evidence(conn, today)?,
        scrub::scrub_secrets(&previous.to_text())
    );
    ask(model, &message)
}

pub fn parse_draft(reply: &Value) -> Result<StandupDraft> {
    serde_json::from_value(reply.clone()).context("standup reply is not three answer lists")
}

fn ask(model: &dyn ModelInvoker, user_message: &str) -> Result<StandupDraft> {
    let answers = json!({ "type": "array", "items": { "type": "string" } });
    let schema = json!({
        "type": "object",
        "required": ["today", "next", "blockers"],
        "properties": { "today": answers, "next": answers, "blockers": answers }
    });
    parse_draft(&model.invoke(SYSTEM_PROMPT, user_message, &schema, LINE_TEXT_MODEL)?)
}

fn evidence(conn: &Connection, today: NaiveDate) -> Result<String> {
    let yesterday = today - Duration::days(1);
    let mut out = String::new();
    for (label, day) in [("Yesterday", yesterday), ("Today", today)] {
        section(
            &mut out,
            &format!("{label}'s blocks ({day})"),
            "none recorded",
            block_lines(conn, day)?,
        );
    }
    section(
        &mut out,
        "Merged PRs",
        "none",
        merged_prs(conn, yesterday, today)?,
    );
    section(&mut out, "Open tickets", "none", open_tickets(conn)?);
    section(
        &mut out,
        "First prompt of each Claude session",
        "none",
        first_prompts(conn, yesterday, today)?,
    );
    Ok(out)
}

fn section(out: &mut String, title: &str, empty: &str, lines: Vec<String>) {
    out.push_str(&format!("{title}:\n"));
    if lines.is_empty() {
        out.push_str(&format!("- {empty}\n"));
    }
    for line in lines {
        out.push_str(&format!("- {line}\n"));
    }
}

fn block_lines(conn: &Connection, day: NaiveDate) -> Result<Vec<String>> {
    Ok(repo::list_blocks_for_day(conn, &day.to_string())?
        .into_iter()
        .filter(|b| !b.is_personal && b.ignored_at.is_none())
        .map(|b| {
            let ticket = b.jira_issue.unwrap_or_else(|| "no ticket".into());
            let text = b.description.unwrap_or_default();
            scrub::scrub_secrets(&format!("{ticket}: {text}"))
        })
        .collect())
}

/// Yesterday's focus line for the local draft; `None` when there is nothing to show.
pub fn yesterday_focus(conn: &Connection, today: NaiveDate) -> Result<Option<FocusSummary>> {
    let day = (today - Duration::days(1)).to_string();
    let summary = focus_summary(&day, &repo::list_blocks_for_day(conn, &day)?);
    Ok((summary.switches > 0 || summary.longest.is_some()).then_some(summary))
}

/// Like `yesterday_focus`, but a read failure is logged and shows no focus, so
/// it never fails the draft.
pub fn yesterday_focus_or_none(conn: &Connection, today: NaiveDate) -> Option<FocusSummary> {
    yesterday_focus(conn, today).unwrap_or_else(|e| {
        tracing::warn!("yesterday's focus unavailable: {e:#}");
        None
    })
}

/// The block's ticket, with an empty string counted as unassigned.
fn ticket(b: &Block) -> Option<&str> {
    b.jira_issue.as_deref().filter(|t| !t.is_empty())
}

pub fn focus_summary(day: &str, blocks: &[Block]) -> FocusSummary {
    let mut counted: Vec<&Block> = blocks
        .iter()
        .filter(|b| !b.is_personal && b.ignored_at.is_none())
        .collect();
    counted.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    let switches = counted
        .windows(2)
        .filter(|w| ticket(w[0]) != ticket(w[1]))
        .count() as u32;
    let mut longest: Option<FocusRun> = None;
    let mut run: Option<FocusRun> = None;
    for b in counted {
        let extends = run.as_ref().is_some_and(|r| {
            Some(&r.jira_issue) == b.jira_issue.as_ref()
                && gap_seconds(&r.ended_at, &b.started_at) <= FOCUS_MAX_GAP_SECONDS
        });
        if extends {
            let r = run.as_mut().expect("extends implies a run");
            if gap_seconds(&r.ended_at, &b.ended_at) > 0 {
                r.ended_at = b.ended_at.clone();
            }
            r.seconds += b.duration_seconds;
        } else {
            run = b
                .jira_issue
                .clone()
                .filter(|t| !t.is_empty())
                .map(|jira_issue| FocusRun {
                    jira_issue,
                    started_at: b.started_at.clone(),
                    ended_at: b.ended_at.clone(),
                    seconds: b.duration_seconds,
                });
        }
        if let Some(r) = run
            .as_ref()
            .filter(|r| longest.as_ref().is_none_or(|l| r.seconds > l.seconds))
        {
            longest = Some(r.clone());
        }
    }
    FocusSummary {
        day: day.to_string(),
        switches,
        longest,
    }
}

/// Seconds from `from` to `to`; an unparseable stamp is an infinite gap.
fn gap_seconds(from: &str, to: &str) -> i64 {
    match (
        DateTime::parse_from_rfc3339(from),
        DateTime::parse_from_rfc3339(to),
    ) {
        (Ok(a), Ok(b)) => (b - a).num_seconds(),
        _ => i64::MAX,
    }
}

fn merged_prs(conn: &Connection, yesterday: NaiveDate, today: NaiveDate) -> Result<Vec<String>> {
    let days = [yesterday.to_string(), today.to_string()];
    let mut stmt = conn.prepare(
        "SELECT title, raw_json FROM events WHERE source = 'github_pr' ORDER BY started_at",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                crate::raw_json::decode_raw_json(r, 1)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(title, raw)| match serde_json::from_str(&raw?).ok()? {
            RawRecord::Commit {
                merged_at: Some(at),
                ..
            } if days.iter().any(|d| at.starts_with(d)) => Some(scrub::scrub_secrets(&title)),
            _ => None,
        })
        .collect())
}

fn open_tickets(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT key, summary, status, description FROM jira_tickets
          WHERE external = 0 AND COALESCE(status_category, '') != 'done'
          ORDER BY updated DESC NULLS LAST, key",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, _, status, _)| !ticket_activity::is_dead_status(status.as_deref()))
        .map(|(key, summary, status, description)| {
            let line = format!("{key} {summary} [{}]", status.unwrap_or_default());
            let Some(description) = description else {
                return scrub::scrub_secrets(&line);
            };
            // One line so it stays inside its bullet; scrub before the cut, as for prompts.
            let flat = description.split_whitespace().collect::<Vec<_>>().join(" ");
            let about: String = scrub::scrub_secrets(&flat)
                .chars()
                .take(DESCRIPTION_CHARS)
                .collect();
            format!("{}: {about}", scrub::scrub_secrets(&line))
        })
        .collect())
}

fn first_prompts(conn: &Connection, yesterday: NaiveDate, today: NaiveDate) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for day in [yesterday, today] {
        for event in repo::load_day_events(conn, &day.to_string())? {
            let (Some(session), Some(details)) = (event.session_id, event.details) else {
                continue;
            };
            if event.source == "claude" && seen.insert(session) {
                // Scrub before the cut so a token is never sliced into a prefix the patterns miss.
                let clean = scrub::scrub_secrets(&details);
                out.push(clean.chars().take(PROMPT_CHARS).collect());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "standup_test.rs"]
mod tests;
