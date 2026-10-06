//! Pre-send check that a line is not already in Tempo as a hand entry
//! (spec 018 FR-06..10).

use crate::daily_helpers_contract::{MatchVerdict, ALREADY_IN_TEMPO, SAME_HOURS_TOLERANCE_SECONDS};
use crate::repo;
use crate::tempo_hub_contract::PulledWorklog;
use crate::tempo_line_contract::TempoLine;
use crate::updater::crypto::sha256_hex;
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

fn unchecked(reason: &str) -> MatchVerdict {
    MatchVerdict::Unchecked {
        reason: reason.to_string(),
    }
}

fn line_text(line: &TempoLine) -> &str {
    line.text.as_deref().unwrap_or(&line.fallback_text)
}

fn fingerprint(line: &TempoLine) -> String {
    sha256_hex(format!("{}\n{}", line.effective_seconds, line_text(line)).as_bytes())
}

fn owned_by_worklog(conn: &Connection, tempo_id: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM blocks WHERE tempo_worklog_id = ?1)",
        [tempo_id],
        |r| r.get(0),
    )
}

/// `matcher` is `verdict::match_texts`. Only entries worklog did not send,
/// on the line's issue and day and within the hours tolerance, are asked about.
pub fn check_line<F>(
    conn: &Connection,
    line: &TempoLine,
    existing: &[PulledWorklog],
    matcher: F,
) -> MatchVerdict
where
    F: Fn(&str, &[String]) -> Result<Vec<bool>>,
{
    let issue_id = match repo::get_ticket_issue_id(conn, &line.jira_issue) {
        Ok(Some(id)) => match id.parse::<i64>() {
            Ok(id) => id,
            Err(_) => return unchecked("ticket issue id is not a number"),
        },
        Ok(None) => return unchecked("ticket issue id unknown"),
        Err(e) => return unchecked(&format!("ticket lookup failed: {e}")),
    };
    let mut candidates = Vec::new();
    for w in existing {
        if w.day != line.day
            || w.issue_id != issue_id
            || (w.seconds - line.effective_seconds).abs() > SAME_HOURS_TOLERANCE_SECONDS
        {
            continue;
        }
        match owned_by_worklog(conn, &w.tempo_worklog_id) {
            Ok(false) => candidates.push(w),
            Ok(true) => {}
            Err(e) => return unchecked(&format!("block lookup failed: {e}")),
        }
    }
    if candidates.is_empty() {
        return MatchVerdict::Different;
    }
    let texts: Vec<String> = candidates.iter().map(|w| w.description.clone()).collect();
    match matcher(line_text(line), &texts) {
        Ok(answers) if answers.len() == texts.len() => {
            match answers.iter().position(|&same| same) {
                Some(i) => MatchVerdict::AlreadyInTempo {
                    tempo_worklog_id: candidates[i].tempo_worklog_id.clone(),
                },
                None => MatchVerdict::Different,
            }
        }
        Ok(_) => unchecked("Verdict gave no usable answer"),
        Err(e) => unchecked(&format!("Verdict unavailable: {e}")),
    }
}

pub fn mark_already(conn: &Connection, line: &TempoLine, tempo_id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, updated_at, match_status, match_tempo_id, match_basis)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(day, jira_issue) DO UPDATE SET
             match_status = excluded.match_status,
             match_tempo_id = excluded.match_tempo_id,
             match_basis = excluded.match_basis",
        params![
            line.day,
            line.jira_issue,
            Utc::now().to_rfc3339(),
            ALREADY_IN_TEMPO,
            tempo_id,
            fingerprint(line)
        ],
    )
    .context("mark_already")?;
    Ok(())
}

/// True only while the line's text and hours are what they were when it was
/// marked, so an Owner edit sends it to a re-check.
pub fn is_already(conn: &Connection, line: &TempoLine) -> bool {
    conn.query_row(
        "SELECT match_status IS ?3 AND match_basis IS ?4 FROM tempo_line_texts
         WHERE day = ?1 AND jira_issue = ?2",
        params![
            line.day,
            line.jira_issue,
            ALREADY_IN_TEMPO,
            fingerprint(line)
        ],
        |r| r.get(0),
    )
    .unwrap_or(false)
}

#[cfg(test)]
#[path = "tempo_match_test.rs"]
mod tests;
