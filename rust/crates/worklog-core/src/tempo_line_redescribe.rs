//! Rewrite one ticket line's text from an Owner instruction (spec 027).
//! Split in prepare / write / commit so the caller holds no connection
//! across the model call. Writes only the line text (as manual) and marks the line's
//! synced blocks for re-sync.

use crate::estimate::ModelInvoker;
use crate::line_text::SYSTEM_PROMPT_IS;
use crate::tempo_line_contract::{TempoLine, TempoLineKey, REASON_TEXT_CHANGED};
use crate::{db, tempo_line_writer, tempo_lines};
use anyhow::{anyhow, bail, Context};
use chrono::Utc;
use rusqlite::{params, Connection};

pub const INSTRUCTION_RULE_IS: &str = "Eigandinn gaf fyrirmæli í reitnum owner_instruction. \
Endurskrifaðu núverandi texta (current_text) samkvæmt þeim og fylgdu öllum reglunum hér að ofan. \
Notaðu aðeins staðreyndir úr inntakinu; ekki bæta við upplýsingum sem vantar.";

#[derive(Debug)]
pub struct LineRedescribePrep {
    pub key: TempoLineKey,
    pub instruction: String,
    pub current_text: String,
    pub stored_text: Option<String>,
    pub user_msg: String,
}

pub fn prepare(
    conn: &Connection,
    key: &TempoLineKey,
    instruction: &str,
) -> anyhow::Result<Option<LineRedescribePrep>> {
    let Some(line) = tempo_lines::line_for(conn, key)? else {
        return Ok(None);
    };
    let instruction = instruction.trim().to_string();
    let current_text = line.text.clone().unwrap_or(line.fallback_text);
    let mut msg: serde_json::Value =
        serde_json::from_str(&tempo_line_writer::prepare(conn, key).map_err(|e| anyhow!(e))?)?;
    msg["owner_instruction"] = instruction.as_str().into();
    msg["current_text"] = current_text.as_str().into();
    Ok(Some(LineRedescribePrep {
        key: key.clone(),
        instruction,
        current_text,
        stored_text: line.text,
        user_msg: msg.to_string(),
    }))
}

pub fn write(
    prep: &LineRedescribePrep,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> Result<String, String> {
    let system = format!("{SYSTEM_PROMPT_IS}\n\n{INSTRUCTION_RULE_IS}");
    tempo_line_writer::write_with_system(&prep.user_msg, &prep.key, invoker, model, &system)
}

pub fn commit(
    conn: &Connection,
    prep: &LineRedescribePrep,
    text: &str,
) -> anyhow::Result<Option<TempoLine>> {
    let tx = db::write_tx(conn)?;
    if tempo_lines::line_for(&tx, &prep.key)?.is_none() {
        return Ok(None);
    }
    tempo_lines::ensure_row(&tx, &prep.key)?;
    let changed = tx
        .execute(
            "UPDATE tempo_line_texts
                SET text = ?3, text_origin = 'manual', source_hash = NULL, check_status = NULL,
                    updated_at = ?5
              WHERE day = ?1 AND jira_issue = ?2 AND text IS ?4",
            params![
                prep.key.day,
                prep.key.jira_issue,
                text,
                prep.stored_text,
                Utc::now().to_rfc3339()
            ],
        )
        .context("line redescribe commit")?;
    if changed == 0 {
        bail!(REASON_TEXT_CHANGED);
    }
    tempo_lines::finish_write(&tx, &prep.key)?;
    tx.commit().context("line redescribe commit")?;
    tempo_lines::line_for(conn, &prep.key)
}

#[path = "tempo_line_redescribe_test.rs"]
#[cfg(test)]
mod tests;
