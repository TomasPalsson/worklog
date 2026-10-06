//! Pre-send checklist and post-send read-back.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::{Connection, OptionalExtension};

use crate::billing::{block_interval, union_seconds};
use crate::daily_helpers_contract::{PreflightCheck, PreflightRow};
use crate::models::Block;
use crate::repo;
use crate::tempo_line_contract::TempoLine;
use crate::tempo_lines;
use crate::tempo_match;

fn green(check: PreflightCheck, detail: String) -> PreflightRow {
    PreflightRow {
        check,
        ok: true,
        detail,
        target: None,
    }
}

fn red(check: PreflightCheck, target: String, detail: String) -> PreflightRow {
    PreflightRow {
        check,
        ok: false,
        detail,
        target: Some(target),
    }
}

/// The red rows of one check, or a single green row when there are none.
fn settle(check: PreflightCheck, reds: Vec<PreflightRow>, ok_detail: &str) -> Vec<PreflightRow> {
    if reds.is_empty() {
        vec![green(check, ok_detail.to_string())]
    } else {
        reds
    }
}

pub fn check(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Result<Vec<PreflightRow>> {
    let (mut ticketed, mut double, mut hours, mut text) = (vec![], vec![], vec![], vec![]);
    let mut day = from;
    while day <= to {
        let day_text = day.to_string();
        let blocks: Vec<_> = repo::list_blocks_for_day(conn, &day_text)?
            .into_iter()
            .filter(|b| !b.is_personal)
            .collect();

        let mut per_ticket: BTreeMap<&str, Vec<(i64, i64)>> = BTreeMap::new();
        for b in &blocks {
            match b.jira_issue.as_deref().filter(|t| !t.is_empty()) {
                Some(ticket) => per_ticket
                    .entry(ticket)
                    .or_default()
                    .push(block_interval(b)),
                None => ticketed.push(red(
                    PreflightCheck::Ticketed,
                    b.id.to_string(),
                    format!("Block {} on {day_text} has no ticket", b.id),
                )),
            }
        }
        let separate: i64 = per_ticket.values().cloned().map(union_seconds).sum();
        let together = union_seconds(per_ticket.into_values().flatten().collect());
        if separate > together {
            double.push(red(
                PreflightCheck::NoDoubleCount,
                day_text.clone(),
                format!(
                    "{day_text}: {}s sits on more than one ticket",
                    separate - together
                ),
            ));
        }

        let lines = tempo_lines::lines_for_day(conn, &day_text)?;
        text.extend(textless_lines(&day_text, &blocks, &lines));
        hours.extend(day_hours(conn, &day_text, &lines)?);
        day += Duration::days(1);
    }
    let mut rows = settle(
        PreflightCheck::Ticketed,
        ticketed,
        "Every work block has a ticket",
    );
    rows.extend(settle(
        PreflightCheck::NoDoubleCount,
        double,
        "No time is counted twice",
    ));
    rows.extend(settle(
        PreflightCheck::DayHours,
        hours,
        "Every day meets its required hours",
    ));
    rows.extend(settle(
        PreflightCheck::LineText,
        text,
        "Every line has text",
    ));
    Ok(rows)
}

/// A line has text when the Owner stored some, or a block under it says
/// what was done; the generated "Work on X" fallback does not count.
fn textless_lines(day: &str, blocks: &[Block], lines: &[TempoLine]) -> Vec<PreflightRow> {
    lines
        .iter()
        .filter(|line| {
            let stored = line.text.as_deref().is_some_and(|t| !t.trim().is_empty());
            !stored
                && !blocks.iter().any(|b| {
                    b.jira_issue.as_deref() == Some(line.jira_issue.as_str())
                        && b.description
                            .as_deref()
                            .is_some_and(|d| !d.trim().is_empty())
                })
        })
        .map(|line| {
            red(
                PreflightCheck::LineText,
                format!("{day} {}", line.jira_issue),
                format!("Line {day} {} has no text", line.jira_issue),
            )
        })
        .collect()
}

/// Already-in-Tempo lines are held there as outside entries, so they are not ours to count.
fn to_send_seconds(conn: &Connection, lines: &[TempoLine]) -> i64 {
    lines
        .iter()
        .filter(|l| !tempo_match::is_already(conn, l))
        .map(|l| l.effective_seconds)
        .sum()
}

fn day_hours(conn: &Connection, day: &str, lines: &[TempoLine]) -> Result<Option<PreflightRow>> {
    let required: Option<i64> = conn
        .query_row(
            "SELECT required_seconds FROM tempo_required_days WHERE day = ?1",
            [day],
            |r| r.get(0),
        )
        .optional()
        .context("required seconds")?;
    let outside: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(seconds), 0) FROM tempo_remote_worklogs
              WHERE day = ?1 AND owner = 'outside'",
            [day],
            |r| r.get(0),
        )
        .context("outside seconds")?;
    let logged = outside + to_send_seconds(conn, lines);
    Ok(required.filter(|r| logged < *r).map(|required| {
        red(
            PreflightCheck::DayHours,
            day.to_string(),
            format!("{day}: {logged}s logged, Tempo requires {required}s"),
        )
    }))
}

pub fn read_back(conn: &Connection, day: NaiveDate) -> Result<PreflightRow> {
    let day_text = day.to_string();
    let sent = to_send_seconds(conn, &tempo_lines::lines_for_day(conn, &day_text)?);
    let in_tempo: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(seconds), 0) FROM tempo_remote_worklogs
              WHERE day = ?1 AND owner = 'worklog'",
            [&day_text],
            |r| r.get(0),
        )
        .context("read-back seconds")?;
    Ok(if sent == in_tempo {
        green(
            PreflightCheck::ReadBack,
            format!("{day_text}: Tempo matches ({sent}s)"),
        )
    } else {
        red(
            PreflightCheck::ReadBack,
            day_text.clone(),
            format!("{day_text}: sent {sent}s, Tempo has {in_tempo}s"),
        )
    })
}

#[cfg(test)]
#[path = "preflight_test.rs"]
mod tests;
