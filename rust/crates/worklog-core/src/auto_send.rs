//! Auto-send to Tempo and the Review list (spec 017 FR-26..FR-35). At
//! [`AUTO_SEND_HOUR`] Owner-local the day's ready lines go to Tempo once;
//! lines Tempo rejected or could not be reached for show up in Review with
//! the reason, and sent lines wait there until the Owner confirms them.

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::clues_contract::LineTextOrigin;
use crate::collectors::tempo::SyncResult;
use crate::purge::{meta_get, meta_set};
use crate::tempo_line_contract::{TempoLine, TempoLineKey, TicketOrigin};
use crate::tempo_lines;
use crate::verdict_contract::{
    DecisionKind, DecisionSource, ReviewLine, ReviewStatus, AUTO_SEND_HOUR, AUTO_SEND_KEY,
};
use crate::verdict_decisions::latest_for;

/// `meta` key holding `<day>|<issue>,<issue>`: the lines the day's 17:00 run
/// committed to. Retries use it, so a line that turns ready later is not sent.
const PLAN_KEY: &str = "auto_send_plan";
const UNREACHABLE: &str = "Tempo could not be reached";

/// Sends one line to Tempo; `Err` means Tempo could not be reached.
pub type SendLine<'a> = dyn FnMut(&Connection, &TempoLineKey) -> Result<Vec<SyncResult>> + 'a;

/// The Settings switch; anything but an exact `on` is off (A7).
pub fn enabled() -> bool {
    crate::envfile::read(AUTO_SEND_KEY).as_deref() == Some("on")
}

fn block_ready(conn: &Connection, block: &crate::models::Block, issue: &str) -> Result<bool> {
    Ok(match block.ticket_origin {
        Some(TicketOrigin::Manual | TicketOrigin::Event) => true,
        Some(TicketOrigin::Auto) => latest_for(conn, DecisionKind::Ticket, &block.id.to_string())?
            .is_some_and(|d| {
                d.source == DecisionSource::Verdict && d.chosen.as_deref() == Some(issue)
            }),
        None => false,
    })
}

/// A day's lines the 17:00 run may send (FR-28): unsent, not failed, with
/// hours and text, every block's ticket Owner-set / in its events / Verdict-
/// picked, and the text either the Owner's own or a passed check.
pub fn ready_lines(conn: &Connection, day: &str) -> Result<Vec<TempoLine>> {
    let mut ready = Vec::new();
    for line in tempo_lines::lines_for_day(conn, day)? {
        let (check, sent, failed): (Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT check_status, auto_sent_at, send_error FROM tempo_line_texts
                  WHERE day = ?1 AND jira_issue = ?2",
                params![line.day, line.jira_issue],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .unwrap_or_default();
        let text_ok = line.text.is_some()
            && (line.text_origin == Some(LineTextOrigin::Manual)
                || check.as_deref() == Some("passed"));
        if !text_ok || sent.is_some() || failed.is_some() || line.effective_seconds == 0 {
            continue;
        }
        let key = TempoLineKey {
            day: line.day.clone(),
            jira_issue: line.jira_issue.clone(),
        };
        let blocks = tempo_lines::blocks_for_ticket(conn, &key)?;
        let unsent = blocks
            .iter()
            .any(|b| b.dirty || b.tempo_worklog_id.as_deref().unwrap_or("").is_empty());
        let mut every_block = true;
        for block in &blocks {
            every_block &= block_ready(conn, block, &line.jira_issue)?;
        }
        if unsent && every_block {
            ready.push(line);
        }
    }
    Ok(ready)
}

fn pending(conn: &Connection, day: &str, issue: &str) -> Result<bool> {
    let open = conn
        .query_row(
            "SELECT auto_sent_at IS NULL AND send_error IS NULL FROM tempo_line_texts
              WHERE day = ?1 AND jira_issue = ?2",
            params![day, issue],
            |r| r.get(0),
        )
        .optional()?;
    Ok(open.unwrap_or(false))
}

fn set_outcome(conn: &Connection, key: &TempoLineKey, error: Option<&str>) -> Result<()> {
    let (sent, error) = match error {
        None => (Some(Utc::now().to_rfc3339()), None),
        Some(e) => (None, Some(e)),
    };
    conn.execute(
        "UPDATE tempo_line_texts SET auto_sent_at = ?3, send_error = ?4
          WHERE day = ?1 AND jira_issue = ?2",
        params![key.day, key.jira_issue, sent, error],
    )
    .context("recording auto-send outcome")?;
    Ok(())
}

fn plan(conn: &Connection) -> Result<Option<(String, Vec<String>)>> {
    Ok(meta_get(conn, PLAN_KEY)?.and_then(|raw| {
        let (day, issues) = raw.split_once('|')?;
        let issues = issues.split(',').filter(|i| !i.is_empty());
        Some((day.to_string(), issues.map(str::to_string).collect()))
    }))
}

/// One 15-minute tick. Plans the day's run at the first tick from 17:00, then
/// sends each planned line through `send`; an unreachable Tempo leaves the
/// rest for the next tick. Lines still unsent when the day is over are marked
/// Not sent (FR-27b).
pub fn run_if_due(
    conn: &Connection,
    local: DateTime<FixedOffset>,
    enabled: bool,
    send: &mut SendLine,
) -> Result<()> {
    let today = local.date_naive().to_string();
    let mut current = plan(conn)?;
    if let Some((day, issues)) = current.take_if(|(day, _)| *day != today) {
        for issue in issues {
            if pending(conn, &day, &issue)? {
                let key = TempoLineKey {
                    day: day.clone(),
                    jira_issue: issue,
                };
                set_outcome(conn, &key, Some(UNREACHABLE))?;
            }
        }
        meta_set(conn, PLAN_KEY, "")?;
    }
    if !enabled || local.hour() < AUTO_SEND_HOUR {
        return Ok(());
    }
    let issues = match current {
        Some((_, issues)) => issues,
        None => {
            let issues: Vec<String> = ready_lines(conn, &today)?
                .into_iter()
                .map(|l| l.jira_issue)
                .collect();
            meta_set(conn, PLAN_KEY, &format!("{today}|{}", issues.join(",")))?;
            issues
        }
    };
    for issue in issues {
        if !pending(conn, &today, &issue)? {
            continue;
        }
        let key = TempoLineKey {
            day: today.clone(),
            jira_issue: issue,
        };
        let results = match send(conn, &key) {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!("auto-send {}: {e:#}", key.jira_issue);
                return Ok(());
            }
        };
        let rejected = results.iter().find(|r| r.status == "error");
        let reason = rejected.map(|r| r.reason.as_deref().unwrap_or("Tempo rejected the line"));
        set_outcome(conn, &key, reason)?;
    }
    Ok(())
}

/// Earlier days' lines that went out (or failed) and are not confirmed yet,
/// newest day first (FR-29, FR-32).
pub fn review_lines(conn: &Connection, today: &str) -> Result<Vec<ReviewLine>> {
    let mut stmt = conn.prepare(
        "SELECT day, jira_issue, send_error FROM tempo_line_texts
          WHERE day < ?1 AND confirmed_at IS NULL
            AND (auto_sent_at IS NOT NULL OR send_error IS NOT NULL)
          ORDER BY day DESC, jira_issue",
    )?;
    let rows = stmt
        .query_map([today], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut lines = Vec::new();
    for (day, jira_issue, error) in rows {
        let key = TempoLineKey {
            day: day.clone(),
            jira_issue: jira_issue.clone(),
        };
        let Some(line) = tempo_lines::line_for(conn, &key)? else {
            continue;
        };
        lines.push(ReviewLine {
            day,
            jira_issue,
            seconds: line.effective_seconds,
            text: line.text.unwrap_or(line.fallback_text),
            status: match error {
                Some(error) => ReviewStatus::NotSent { error },
                None => ReviewStatus::Sent,
            },
        });
    }
    Ok(lines)
}

/// Confirms the sent lines of `day` (one ticket, or all). Failed lines stay
/// in Review; Tempo and the blocks are not touched. Returns how many.
pub fn confirm(conn: &Connection, day: &str, jira_issue: Option<&str>) -> Result<usize> {
    conn.execute(
        "UPDATE tempo_line_texts SET confirmed_at = ?3
          WHERE day = ?1 AND (?2 IS NULL OR jira_issue = ?2)
            AND auto_sent_at IS NOT NULL AND send_error IS NULL AND confirmed_at IS NULL",
        params![day, jira_issue, Utc::now().to_rfc3339()],
    )
    .context("confirming review lines")
}

#[path = "auto_send_test.rs"]
#[cfg(test)]
mod tests;
