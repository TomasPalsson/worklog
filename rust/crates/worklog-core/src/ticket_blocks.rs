//! "Work logged" on a ticket (blocks in the My Tasks panel): the ticket's
//! blocks per local day over a window, with the day's Tempo line.

use crate::billing::{block_interval, union_seconds};
use crate::models::Block;
use crate::repo;
use crate::tempo_hub_contract::{TicketBlocks, TicketDay, TodayTotals};
use crate::tempo_lines;
use crate::week_closeout::closeout_day;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

pub fn ticket_blocks(
    conn: &Connection,
    key: &str,
    today: NaiveDate,
    days: u32,
) -> Result<TicketBlocks> {
    let mut out = Vec::new();
    for offset in 0..days {
        let day = (today - Duration::days(offset.into())).to_string();
        let blocks: Vec<_> = repo::list_blocks_for_day(conn, &day)?
            .into_iter()
            .filter(|b| {
                b.jira_issue.as_deref() == Some(key) && !b.is_personal && b.ignored_at.is_none()
            })
            .collect();
        if blocks.is_empty() {
            continue;
        }
        let line = tempo_lines::lines_for_day(conn, &day)?
            .into_iter()
            .find(|line| line.jira_issue == key);
        let tracked_seconds = line.as_ref().map_or(0, |_| {
            union_seconds(blocks.iter().map(block_interval).collect())
        });
        let hours_set_by_hand = line
            .as_ref()
            .is_some_and(|l| l.hours_override_seconds.is_some());
        let (line_seconds, line_text) = line.map_or((0, String::new()), |line| {
            let text = line
                .text
                .filter(|t| !t.is_empty())
                .unwrap_or(line.fallback_text);
            (line.effective_seconds, text)
        });
        out.push(TicketDay {
            in_tempo_seconds: in_tempo_seconds(conn, key, &day)?,
            day,
            line_seconds,
            line_text,
            tracked_seconds,
            hours_set_by_hand,
            blocks,
        });
    }
    Ok(TicketBlocks {
        key: key.to_string(),
        from: (today - Duration::days(days.saturating_sub(1).into())).to_string(),
        to: today.to_string(),
        days: out,
        in_tempo_total_seconds: in_tempo_total(conn, key)?,
        pulled_at: conn.query_row(
            "SELECT MAX(pulled_at) FROM tempo_remote_worklogs",
            [],
            |r| r.get(0),
        )?,
        today: today_totals(conn, key, &today.to_string())?,
    })
}

/// Owner-pulled seconds on the ticket over all pulled days; None without an issue id or any pull.
fn in_tempo_total(conn: &Connection, key: &str) -> Result<Option<i64>> {
    let Some(issue_id) = issue_id(conn, key)? else {
        return Ok(None);
    };
    Ok(conn.query_row(
        "SELECT CASE WHEN EXISTS (SELECT 1 FROM tempo_remote_worklogs)
                     THEN COALESCE(SUM(seconds), 0) END
           FROM tempo_remote_worklogs WHERE CAST(issue_id AS TEXT) = ?1",
        [issue_id],
        |r| r.get(0),
    )?)
}

fn today_totals(conn: &Connection, key: &str, day: &str) -> Result<TodayTotals> {
    let blocks: Vec<_> = repo::list_blocks_for_day(conn, day)?
        .into_iter()
        .filter(|b| !b.is_personal && b.ignored_at.is_none())
        .collect();
    let worked = |keep: &dyn Fn(&Block) -> bool| {
        union_seconds(
            blocks
                .iter()
                .filter(|b| keep(b))
                .map(block_interval)
                .collect(),
        )
    };
    Ok(TodayTotals {
        day: day.to_string(),
        worked_seconds: worked(&|_| true),
        in_tempo_seconds: day_pulled(conn, day)?
            .then(|| closeout_day(conn, day))
            .transpose()?
            .map(|d| d.tempo_seconds),
        ticket_worked_seconds: worked(&|b| b.jira_issue.as_deref() == Some(key)),
        ticket_in_tempo_seconds: in_tempo_seconds(conn, key, day)?,
    })
}

fn day_pulled(conn: &Connection, day: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tempo_remote_worklogs WHERE day = ?1)
             OR EXISTS (SELECT 1 FROM tempo_required_days WHERE day = ?1)",
        [day],
        |r| r.get(0),
    )?)
}

fn issue_id(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT issue_id FROM jira_tickets WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
}

/// Tempo's seconds for the ticket on `day` at the last pull; None if never pulled.
fn in_tempo_seconds(conn: &Connection, key: &str, day: &str) -> Result<Option<i64>> {
    let pulled = day_pulled(conn, day)?;
    let issue_id = issue_id(conn, key)?;
    let (true, Some(issue_id)) = (pulled, issue_id) else {
        return Ok(None);
    };
    Ok(Some(conn.query_row(
        "SELECT COALESCE(SUM(seconds), 0) FROM tempo_remote_worklogs
         WHERE day = ?1 AND CAST(issue_id AS TEXT) = ?2",
        params![day, issue_id],
        |r| r.get(0),
    )?))
}

#[path = "ticket_blocks_test.rs"]
#[cfg(test)]
mod tests;
