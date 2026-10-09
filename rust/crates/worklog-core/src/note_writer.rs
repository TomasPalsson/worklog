//! Note blocks: save a rough note as a manual block, then let the AI
//! rewrite it into the description — never over a hand edit. Split in
//! prepare / invoke / commit so the caller holds no connection across
//! the model call.

use crate::block_service::MARK_DIRTY_IF_SYNCED;
use crate::change_log;
use crate::deild_contract::ChangeSource;
use crate::estimate::ModelInvoker;
use crate::models::Block;
use crate::note_block_contract::*;
use crate::repo;
use crate::tempo_hub_contract::HubError;
use crate::ticket_log::{self, LogTimeBody, MAX_DESCRIPTION_CHARS};
use anyhow::Result;
use chrono::{NaiveDate, NaiveTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

const SYSTEM_PROMPT: &str = "You write timesheet descriptions. From the Owner's rough note, \
the ticket and the minutes spent, write one to three sentences in the past tense describing the \
work, as it would appear on a timesheet. Use only facts in the note and ticket; do not invent \
details, names or outcomes. Reply with JSON: {\"description\": string}.";

#[derive(Debug)]
pub struct NotePrep {
    pub block_id: i64,
    pub note: String,
    pub jira_issue: String,
    pub ticket_summary: Option<String>,
    pub minutes: i64,
}

pub fn log_note_block(conn: &Connection, body: &NoteBlockBody, today: NaiveDate) -> Result<Block> {
    if let Ok(start) = NaiveTime::parse_from_str(&body.start, "%H:%M") {
        if i64::from(start.num_seconds_from_midnight() / 60) + body.minutes > 24 * 60 {
            return Err(HubError::InvalidInput("block ends after midnight".into()).into());
        }
    }
    let logged = LogTimeBody {
        day: body.day.clone(),
        start: body.start.clone(),
        minutes: body.minutes,
        description: body.note.clone(),
    };
    let block = ticket_log::log_time(conn, &body.jira_issue, &logged, today)?;
    conn.execute(
        "UPDATE blocks SET rough_note = ?1, description_origin = 'note' WHERE id = ?2",
        params![body.note.trim(), block.id],
    )?;
    repo::get_block(conn, block.id)?
        .ok_or_else(|| anyhow::anyhow!("note block {} vanished", block.id))
}

pub fn prepare_note(conn: &Connection, block_id: i64) -> std::result::Result<NotePrep, String> {
    let block = repo::get_block(conn, block_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| REASON_NOT_A_NOTE_BLOCK.to_string())?;
    let (Some(note), Some(jira_issue)) = (block.rough_note, block.jira_issue) else {
        return Err(REASON_NOT_A_NOTE_BLOCK.into());
    };
    let ticket_summary = conn
        .query_row(
            "SELECT summary FROM jira_tickets WHERE key = ?1",
            [&jira_issue],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(NotePrep {
        block_id,
        note,
        jira_issue,
        ticket_summary,
        minutes: block.duration_seconds / 60,
    })
}

pub fn invoke_note(
    prep: &NotePrep,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> std::result::Result<String, String> {
    let ticket = match &prep.ticket_summary {
        Some(summary) => format!("{} ({summary})", prep.jira_issue),
        None => prep.jira_issue.clone(),
    };
    let user = format!(
        "Ticket: {ticket}\nTime spent: {} minutes\nRough note: {}",
        prep.minutes, prep.note
    );
    let schema = json!({
        "type": "object",
        "properties": { "description": { "type": "string" } },
        "required": ["description"],
    });
    let reply = invoker
        .invoke(SYSTEM_PROMPT, &user, &schema, model)
        .map_err(|e| e.to_string())?;
    let text = reply
        .get("description")
        .and_then(Value::as_str)
        .ok_or("model reply has no description")?
        .trim();
    match text.chars().count() {
        0 => Err("model wrote an empty description".into()),
        n if n > MAX_DESCRIPTION_CHARS => Err(format!(
            "model wrote {n} characters; the limit is {MAX_DESCRIPTION_CHARS}"
        )),
        _ => Ok(text.to_string()),
    }
}

pub fn commit_note(
    conn: &Connection,
    block_id: i64,
    text: &str,
    force: bool,
) -> std::result::Result<(), String> {
    let changed = conn
        .execute(
            &format!(
                "UPDATE blocks
                    SET description = ?1, description_origin = 'ai',
                        dirty = {MARK_DIRTY_IF_SYNCED}
                  WHERE id = ?2 AND rough_note IS NOT NULL
                    AND (?3 OR description_origin IN ('note','ai'))"
            ),
            params![text, block_id, force],
        )
        .map_err(|e| e.to_string())?;
    let block = repo::get_block(conn, block_id).map_err(|e| e.to_string())?;
    match (changed, block) {
        (0, Some(b)) if b.rough_note.is_some() => Err(REASON_HAND_EDITED.into()),
        (0, _) => Err(REASON_NOT_A_NOTE_BLOCK.into()),
        (_, Some(b)) => {
            change_log::refresh_day_logged(conn, &b.day, ChangeSource::Claude);
            Ok(())
        }
        (_, None) => Err(REASON_NOT_A_NOTE_BLOCK.into()),
    }
}

#[path = "note_writer_test.rs"]
#[cfg(test)]
mod tests;
