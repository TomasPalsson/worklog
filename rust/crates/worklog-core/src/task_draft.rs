//! AI ticket update draft (spec 012, B7): a comment plus at most one
//! suggested transition, built from the ticket's recent line texts.

use crate::estimate::ModelInvoker;
use crate::tempo_hub_contract::{TicketDraft, Transition, COMMENT_MAX_CHARS};
use crate::tempo_lines;
use anyhow::{anyhow, Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde_json::{json, Value};

pub struct DraftPrep {
    pub key: String,
    pub summary: String,
    pub status: Option<String>,
    pub recent_lines: Vec<(String, String)>,
}

const RECENT_DAYS: i64 = 14;
const RECENT_LINE_LIMIT: usize = 20;

pub fn prepare_draft(conn: &Connection, key: &str, today: NaiveDate) -> Result<DraftPrep> {
    let (summary, status) = conn
        .query_row(
            "SELECT summary, status FROM jira_tickets WHERE key = ?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .with_context(|| format!("ticket {key} is not cached"))?;
    let mut recent_lines = Vec::new();
    for offset in 0..RECENT_DAYS {
        let day = (today - Duration::days(offset)).to_string();
        for line in tempo_lines::lines_for_day(conn, &day)? {
            if line.jira_issue == key {
                recent_lines.push((day.clone(), line.text.unwrap_or(line.fallback_text)));
            }
        }
    }
    recent_lines.truncate(RECENT_LINE_LIMIT);
    Ok(DraftPrep {
        key: key.to_string(),
        summary,
        status,
        recent_lines,
    })
}

pub fn draft_with(
    invoker: &dyn ModelInvoker,
    prep: &DraftPrep,
    transitions: &[Transition],
    model: &str,
) -> Result<TicketDraft> {
    let reply = invoker.invoke(
        SYSTEM_PROMPT,
        &build_prompt(prep, transitions),
        &draft_schema(),
        model,
    )?;
    let comment = reply
        .get("comment")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|comment| !comment.is_empty())
        .ok_or_else(|| anyhow!("draft reply has no comment"))?;
    let suggested_transition_id = reply
        .get("suggested_transition_id")
        .and_then(Value::as_str)
        .and_then(|chosen| {
            transitions
                .iter()
                .find(|transition| transition.id == chosen || transition.name == chosen)
        })
        .map(|transition| transition.id.clone());
    Ok(TicketDraft {
        comment: comment.chars().take(COMMENT_MAX_CHARS).collect(),
        suggested_transition_id,
        transitions: transitions.to_vec(),
    })
}

const SYSTEM_PROMPT: &str = "You draft a short Jira comment reporting progress on one ticket, \
based only on the JSON you receive: the ticket key, summary, status, the names of the \
available transitions, and recent work log lines (newest first). Write plain prose for a \
colleague, no headings, and do not invent work the lines do not show. If the work clearly \
completes the ticket or moves it to another state, set suggested_transition_id to the id of \
one listed transition (use its name exactly), otherwise null. Reply only with JSON \
{\"comment\": \"...\", \"suggested_transition_id\": \"...\" or null}.";

fn draft_schema() -> Value {
    json!({
        "type": "object",
        "required": ["comment"],
        "properties": {
            "comment": { "type": "string" },
            "suggested_transition_id": { "type": ["string", "null"] }
        }
    })
}

/// The model sees transition names only, so `draft_with` also resolves a
/// reply that names a transition to its id.
fn build_prompt(prep: &DraftPrep, transitions: &[Transition]) -> String {
    json!({
        "key": prep.key,
        "summary": prep.summary,
        "status": prep.status,
        "transitions": transitions.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        "recent_lines": prep
            .recent_lines
            .iter()
            .map(|(day, text)| json!({"day": day, "text": text}))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

#[path = "task_draft_test.rs"]
#[cfg(test)]
mod tests;
