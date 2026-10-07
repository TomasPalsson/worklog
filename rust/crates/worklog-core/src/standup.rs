//! Standup draft (FR-18, FR-19, FR-24): yesterday's and today's blocks,
//! merged PRs, open tickets and each Claude session's first prompt go to
//! the model, secrets removed first, and come back as three answers.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::clues_contract::RawRecord;
use crate::daily_helpers_contract::StandupDraft;
use crate::estimate::ModelInvoker;
use crate::line_text::LINE_TEXT_MODEL;
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
