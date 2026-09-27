//! Billing-line texts: one generated Icelandic text per billing line
//! (spec 006, D-12), hand edits that generation never overwrites (FR-31),
//! and the previous-text fallback on writer failure (FR-35). Populated by
//! T020: `validate`, `generate_for_day`, `set_manual`, `text_for`.

use crate::billing;
use crate::clues_contract::{BillingLineKey, LineTextOrigin};
use crate::clues_send;
use crate::estimate::ModelInvoker;
use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;

#[path = "line_text_prompt.rs"]
mod prompt;
/// Icelandic system prompt for [`generate_for_day`]. Kept short, plain,
/// non-technical: the audience is a boss or customer, not a developer.
/// Split into its own module purely to keep this file under the repo's
/// size guard.
pub use prompt::SYSTEM_PROMPT_IS;

/// Invoice text reaches customers in Icelandic: a stronger model than the
/// per-block estimator's, for a handful of calls a day.
pub const LINE_TEXT_MODEL: &str = "claude-sonnet-5";

const MAX_CHARS: usize = 400;
const MIN_SENTENCES: usize = 2;
const MAX_SENTENCES: usize = 3;
const PATH_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".py", ".md", ".json", ".tsx", ".toml", ".yml", ".yaml", ".sql", ".sh",
];

/// Trims `text` and rejects it (with a short reason) unless it reads like
/// a hand-written, non-technical Icelandic sentence or two (B9, D-14).
pub fn validate(text: &str) -> std::result::Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty".to_string());
    }
    if trimmed.chars().count() > MAX_CHARS {
        return Err(format!("too long (max {MAX_CHARS} chars)"));
    }
    let sentences = count_sentences(trimmed);
    if !(MIN_SENTENCES..=MAX_SENTENCES).contains(&sentences) {
        return Err(format!(
            "need {MIN_SENTENCES}-{MAX_SENTENCES} sentences, got {sentences}"
        ));
    }
    if trimmed.chars().any(|c| c.is_ascii_digit()) {
        // A Jira/PR key always carries a trailing digit, so this alone
        // also catches every such token — no separate key regex needed.
        return Err("contains a number".to_string());
    }
    if trimmed.contains(['#', '{', '}']) {
        return Err("contains '#' or JSON braces".to_string());
    }
    if has_path_like_token(trimmed) {
        return Err("contains a path or file name".to_string());
    }
    if tool_name_re().is_match(trimmed) {
        return Err("contains a tool name".to_string());
    }
    if !has_icelandic_letter(trimmed) {
        return Err("not Icelandic".to_string());
    }
    Ok(trimmed.to_string())
}

/// Segments ending in `. ! ?` followed by whitespace/end each count as one
/// sentence; a trailing segment with no terminal punctuation counts too.
fn count_sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut seg_start = 0;
    for i in 0..chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?') {
            match chars.get(i + 1) {
                None => {
                    count += 1;
                    seg_start = i + 1;
                }
                Some(next) if next.is_whitespace() => {
                    count += 1;
                    seg_start = i + 1;
                }
                _ => {}
            }
        }
    }
    let trailing: String = chars[seg_start..].iter().collect();
    if !trailing.trim().is_empty() {
        count += 1;
    }
    count
}

fn has_path_like_token(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        word.contains('/')
            || word.contains('\\')
            || PATH_EXTENSIONS.iter().any(|ext| word.ends_with(ext))
    })
}

fn tool_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(Bash|Read|Edit|Write|Grep|Glob|MultiEdit|NotebookEdit|WebFetch|WebSearch|TodoWrite|Agent|Task)\b",
        )
        .unwrap()
    })
}

fn has_icelandic_letter(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            'á' | 'é'
                | 'í'
                | 'ó'
                | 'ú'
                | 'ý'
                | 'þ'
                | 'æ'
                | 'ö'
                | 'ð'
                | 'Á'
                | 'É'
                | 'Í'
                | 'Ó'
                | 'Ú'
                | 'Ý'
                | 'Þ'
                | 'Æ'
                | 'Ö'
                | 'Ð'
        )
    })
}

/// Result of one [`generate_for_day`] run.
pub struct LineTextReport {
    pub generated: Vec<BillingLineKey>,
    /// One entry per line that kept its previous (or absent) text (FR-35).
    pub not_generated: Vec<(BillingLineKey, String)>,
}

/// Generates a text for every distinct billing line of `day`, skipping
/// hand-edited lines (FR-31) and leaving any failing line's stored text
/// untouched (FR-35) — one line's failure never stops the others.
pub fn generate_for_day(
    conn: &Connection,
    day: &str,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> Result<LineTextReport> {
    let mut report = LineTextReport {
        generated: Vec::new(),
        not_generated: Vec::new(),
    };
    for key in distinct_keys(conn, day)? {
        match generate_line(conn, &key, invoker, model) {
            Ok(()) => report.generated.push(key),
            // A hand-edited line is skipped silently here — it's never
            // overwritten (FR-31), not a failure to report. Only
            // `generate_line`'s single-line caller (the daemon's
            // Regenerate route) needs that reason surfaced.
            Err(reason) if reason == "hand-edited" => {}
            Err(reason) => report.not_generated.push((key, reason)),
        }
    }
    Ok(report)
}

/// One line's worth of [`generate_for_day`]'s body, split out so the
/// daemon's single-line "Regenerate" route can reuse it directly. A
/// hand-edited line (FR-31) is left untouched and reported as
/// `Err("hand-edited")`; any other failure also leaves the stored text
/// untouched (FR-35).
pub fn generate_line(
    conn: &Connection,
    key: &BillingLineKey,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> std::result::Result<(), String> {
    if stored_origin(conn, key).map_err(|e| e.to_string())? == Some(LineTextOrigin::Manual) {
        return Err("hand-edited".to_string());
    }
    generate_one(conn, key, invoker, model)
}

/// [`generate_for_day`], resolving the model invoker the same way
/// [`crate::estimate::estimate_day`] does (env/secrets via
/// [`crate::estimate::build_invoker`]) — so a caller that just wants
/// "today's configured provider" doesn't have to construct one itself.
pub fn generate_with_default_provider(
    conn: &Connection,
    day: &str,
    model: &str,
) -> Result<LineTextReport> {
    match crate::estimate::build_invoker() {
        Ok(invoker) => generate_for_day(conn, day, invoker.as_ref(), model),
        // FR-35: a misconfigured provider must surface as a report naming
        // every line the day would have touched, not an Err the caller
        // swallows into an empty (silently-looking-fine) report.
        Err(e) => report_for_invoker_error(conn, day, &e.to_string()),
    }
}

/// Every distinct billing line of `day`, reported as `not_generated` with
/// `reason` — used when the model invoker itself couldn't be built. A
/// hand-edited (manual-origin) line is skipped (FR-31): generation would
/// have skipped it too, so it's not "not generated".
fn report_for_invoker_error(conn: &Connection, day: &str, reason: &str) -> Result<LineTextReport> {
    let mut not_generated = Vec::new();
    for key in distinct_keys(conn, day)? {
        if stored_origin(conn, &key)? == Some(LineTextOrigin::Manual) {
            continue;
        }
        not_generated.push((key, reason.to_string()));
    }
    Ok(LineTextReport {
        generated: Vec::new(),
        not_generated,
    })
}

/// [`generate_line`]'s sibling, resolving the model invoker the same way
/// [`generate_with_default_provider`] does. Used by the daemon's
/// single-line "Regenerate" route.
pub fn generate_line_with_default_provider(
    conn: &Connection,
    key: &BillingLineKey,
    model: &str,
) -> std::result::Result<(), String> {
    let invoker = crate::estimate::build_invoker().map_err(|e| e.to_string())?;
    generate_line(conn, key, invoker.as_ref(), model)
}

fn distinct_keys(conn: &Connection, day: &str) -> Result<Vec<BillingLineKey>> {
    let rows = billing::rows_for_day(conn, day)?;
    let mut seen = HashSet::new();
    let mut keys = Vec::new();
    for row in rows {
        let key = BillingLineKey {
            day: day.to_string(),
            folder: row.folder.clone(),
            customer: row.customer.clone().unwrap_or_default(),
        };
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn stored_origin(conn: &Connection, key: &BillingLineKey) -> Result<Option<LineTextOrigin>> {
    let origin: Option<String> = conn
        .query_row(
            "SELECT origin FROM billing_line_texts WHERE day = ?1 AND folder = ?2 AND customer = ?3",
            params![key.day, key.folder, key.customer],
            |r| r.get(0),
        )
        .optional()
        .context("stored_origin")?;
    Ok(origin.map(|o| {
        if o == "manual" {
            LineTextOrigin::Manual
        } else {
            LineTextOrigin::Generated
        }
    }))
}

fn generate_one(
    conn: &Connection,
    key: &BillingLineKey,
    invoker: &dyn ModelInvoker,
    model: &str,
) -> std::result::Result<(), String> {
    let input = clues_send::build_line_input(conn, key).map_err(|e| e.to_string())?;
    let user = serde_json::to_string(&input).map_err(|e| e.to_string())?;
    let reply = invoker
        .invoke(SYSTEM_PROMPT_IS, &user, &line_text_schema(), model)
        .map_err(|e| e.to_string())?;
    let text = reply
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "reply missing `text` field".to_string())?;
    let validated = validate(&unwrap_nested_text(text))?;
    upsert_generated(conn, key, &validated).map_err(|e| e.to_string())
}

/// `claude -p` sometimes puts the whole `{"text": "..."}` reply inside the
/// schema's `text` field again; take the inner text when it does.
fn unwrap_nested_text(text: &str) -> String {
    serde_json::from_str::<Value>(text.trim())
        .ok()
        .and_then(|v| v.get("text").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| text.to_owned())
}

fn line_text_schema() -> Value {
    json!({
        "type": "object",
        "required": ["text"],
        "properties": { "text": { "type": "string" } }
    })
}

fn upsert_generated(conn: &Connection, key: &BillingLineKey, text: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO billing_line_texts (day, folder, customer, text, origin, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'generated', ?5)
         ON CONFLICT(day, folder, customer) DO UPDATE SET
             text = excluded.text,
             origin = excluded.origin,
             updated_at = excluded.updated_at",
        params![
            key.day,
            key.folder,
            key.customer,
            text,
            Utc::now().to_rfc3339()
        ],
    )
    .context("upsert_generated")?;
    Ok(())
}

/// Hands a billing line's text back to the owner's own words (FR-31).
/// A trimmed-empty `text` deletes the row instead, handing the line back
/// to the next `generate_for_day` run. Never validated — the owner's
/// words are final.
pub fn set_manual(conn: &Connection, key: &BillingLineKey, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        conn.execute(
            "DELETE FROM billing_line_texts WHERE day = ?1 AND folder = ?2 AND customer = ?3",
            params![key.day, key.folder, key.customer],
        )
        .context("set_manual delete")?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO billing_line_texts (day, folder, customer, text, origin, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'manual', ?5)
         ON CONFLICT(day, folder, customer) DO UPDATE SET
             text = excluded.text,
             origin = excluded.origin,
             updated_at = excluded.updated_at",
        params![
            key.day,
            key.folder,
            key.customer,
            text,
            Utc::now().to_rfc3339()
        ],
    )
    .context("set_manual upsert")?;
    Ok(())
}

/// The stored text and its origin for one billing line, if any. A
/// re-keyed line (customer changed) simply has no row here — the caller
/// falls back to whatever it shows for a line with no text yet.
pub fn text_for(
    conn: &Connection,
    key: &BillingLineKey,
) -> Result<Option<(String, LineTextOrigin)>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT text, origin FROM billing_line_texts WHERE day = ?1 AND folder = ?2 AND customer = ?3",
            params![key.day, key.folder, key.customer],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .context("text_for")?;
    Ok(row.map(|(text, origin)| {
        let origin = if origin == "manual" {
            LineTextOrigin::Manual
        } else {
            LineTextOrigin::Generated
        };
        (text, origin)
    }))
}

#[path = "line_text_test.rs"]
#[cfg(test)]
mod tests;
