//! Regenerate one block's description from an Owner instruction (spec 026).
//! Split in prepare / invoke / commit so the caller holds no connection
//! across the model call. Writes the description (and `dirty` when synced)
//! and nothing else.

use crate::block_digest::eval_evidence;
use crate::block_service::MARK_DIRTY_IF_SYNCED;
use crate::change_log;
use crate::daily_helpers_contract::BlockChange;
use crate::deild_contract::ChangeSource;
use crate::estimate::ModelInvoker;
use crate::redescribe_contract::*;
use crate::undo;
use crate::{db, repo};
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

const MAX_DESCRIPTION_CHARS: usize = 500;

const SYSTEM_PROMPT: &str = "You rewrite one timesheet description. Follow the Owner's \
instruction, using the current description, the ticket, the minutes spent and the evidence. \
Write one to three sentences in the past tense. Use only facts present in those inputs; do not \
invent details, names or outcomes. Reply with JSON: {\"description\": string}.";

#[derive(Debug)]
pub struct RedescribePrep {
    pub block_id: i64,
    pub instruction: String,
    pub description: String,
    pub jira_issue: Option<String>,
    pub ticket_summary: Option<String>,
    pub minutes: i64,
    pub repos: Vec<String>,
    pub titles: Vec<String>,
}

pub fn prepare(conn: &Connection, block_id: i64, instruction: &str) -> Result<RedescribePrep> {
    let block =
        repo::get_block(conn, block_id)?.ok_or_else(|| anyhow!("block {block_id} not found"))?;
    if block.is_personal {
        bail!(REASON_PERSONAL);
    }
    if block.rough_note.is_some() {
        bail!(REASON_NOTE_BLOCK);
    }
    let description = block
        .description
        .filter(|d| !d.trim().is_empty())
        .ok_or_else(|| anyhow!(REASON_NOT_DESCRIBED))?;
    let ticket_summary = match &block.jira_issue {
        Some(key) => conn
            .query_row(
                "SELECT summary FROM jira_tickets WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?,
        None => None,
    };
    let (repos, titles) = eval_evidence(&repo::list_events_for_block(conn, block_id)?);
    Ok(RedescribePrep {
        block_id,
        instruction: instruction.trim().to_string(),
        description,
        jira_issue: block.jira_issue,
        ticket_summary,
        minutes: block.duration_seconds / 60,
        repos,
        titles,
    })
}

pub fn invoke<I: ModelInvoker>(prep: &RedescribePrep, invoker: &I, model: &str) -> Result<String> {
    let ticket = match (&prep.jira_issue, &prep.ticket_summary) {
        (Some(key), Some(summary)) => format!("{key} ({summary})"),
        (Some(key), None) => key.clone(),
        _ => "none".to_string(),
    };
    let user = format!(
        "Instruction: {}\nCurrent description: {}\nTicket: {ticket}\nTime spent: {} minutes\n\
         Repos: {}\nEvidence:\n{}",
        prep.instruction,
        prep.description,
        prep.minutes,
        prep.repos.join(", "),
        prep.titles.join("\n"),
    );
    let schema = json!({
        "type": "object",
        "properties": { "description": { "type": "string" } },
        "required": ["description"],
    });
    let reply = invoker.invoke(SYSTEM_PROMPT, &user, &schema, model)?;
    let text = reply
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("model reply has no description"))?
        .trim();
    if text.is_empty() {
        bail!("model wrote an empty description");
    }
    Ok(cut_at_word(text, MAX_DESCRIPTION_CHARS))
}

fn cut_at_word(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(limit).collect();
    match chars.next() {
        Some(next) if !next.is_whitespace() => match head.rfind(char::is_whitespace) {
            Some(at) => head[..at].trim_end().to_string(),
            None => head,
        },
        Some(_) => head.trim_end().to_string(),
        None => head,
    }
}

pub fn commit(
    conn: &Connection,
    prep: &RedescribePrep,
    description: &str,
) -> Result<RedescribeResult> {
    let tx = db::write_tx(conn)?;
    undo::record(&tx, BlockChange::Text, &[prep.block_id])?;
    let changed = tx
        .execute(
            &format!(
                "UPDATE blocks SET description = ?1, dirty = {MARK_DIRTY_IF_SYNCED} WHERE id = ?2"
            ),
            params![description, prep.block_id],
        )
        .context("redescribe commit")?;
    if changed == 0 {
        bail!("block {} not found", prep.block_id);
    }
    undo::seal(&tx, &[])?;
    tx.commit().context("redescribe commit")?;
    crate::ask::refresh(conn, &[prep.block_id]);
    if let Some(block) = repo::get_block(conn, prep.block_id)? {
        change_log::refresh_day_logged(conn, &block.day, ChangeSource::Claude);
    }
    Ok(RedescribeResult {
        block_id: prep.block_id,
        description: description.to_string(),
    })
}

pub fn redescribe_block_with<I: ModelInvoker>(
    conn: &Connection,
    block_id: i64,
    instruction: &str,
    invoker: &I,
    model: &str,
) -> Result<RedescribeResult> {
    let prep = prepare(conn, block_id, instruction)?;
    let description = invoke(&prep, invoker, model)?;
    commit(conn, &prep, &description)
}

#[path = "redescribe_test.rs"]
#[cfg(test)]
mod tests;
