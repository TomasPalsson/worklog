//! My Tasks board (spec 012): the cached assigned-open tickets plus any
//! ticket with a ticket line this week, with that week's hours.

use crate::tempo_hub_contract::{StatusCategory, TaskRow, TasksResponse};
use crate::tempo_lines;
use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use std::collections::{BTreeSet, HashMap};

struct CachedTicket {
    summary: String,
    status: Option<String>,
    status_category: Option<StatusCategory>,
    assigned: bool,
    issue_type: Option<String>,
    priority: Option<String>,
    due_date: Option<String>,
    labels: Vec<String>,
    parent_summary: Option<String>,
    updated: Option<String>,
}

#[derive(Default)]
struct Worked {
    week_seconds: i64,
    today_seconds: i64,
}

const DAYS: usize = 7;

pub fn tasks(
    conn: &Connection,
    monday: NaiveDate,
    today: NaiveDate,
    jira_base_url: Option<&str>,
) -> Result<TasksResponse> {
    let cached = cached_tickets(conn)?;
    let last_worked = last_worked_days(conn)?;
    let mut worked: HashMap<String, Worked> = HashMap::new();
    let mut day_seconds: HashMap<String, [i64; DAYS]> = HashMap::new();
    for offset in 0..DAYS {
        let day = (monday + Duration::days(offset as i64)).to_string();
        for line in tempo_lines::lines_for_day(conn, &day)? {
            day_seconds.entry(line.jira_issue.clone()).or_default()[offset] +=
                line.effective_seconds;
            let entry = worked.entry(line.jira_issue).or_default();
            entry.week_seconds += line.effective_seconds;
            if day == today.to_string() {
                entry.today_seconds += line.effective_seconds;
            }
        }
    }

    let keys: BTreeSet<&String> = cached
        .iter()
        .filter(|(_, ticket)| ticket.assigned)
        .map(|(key, _)| key)
        .chain(worked.keys())
        .filter(|key| !cached.get(*key).is_some_and(is_backlog))
        .collect();
    let base = jira_base_url.map(|url| url.trim_end_matches('/'));
    let mut rows: Vec<TaskRow> = keys
        .into_iter()
        .map(|key| {
            let ticket = cached.get(key);
            let hours = worked.get(key);
            TaskRow {
                key: key.clone(),
                summary: ticket.map_or_else(|| key.clone(), |t| t.summary.clone()),
                status: ticket.and_then(|t| t.status.clone()),
                status_category: ticket.and_then(|t| t.status_category),
                url: base.map(|base| format!("{base}/browse/{key}")),
                assigned: ticket.is_some_and(|t| t.assigned),
                week_seconds: hours.map_or(0, |h| h.week_seconds),
                today_seconds: hours.map_or(0, |h| h.today_seconds),
                last_worked_day: last_worked.get(key).cloned(),
                issue_type: ticket.and_then(|t| t.issue_type.clone()),
                priority: ticket.and_then(|t| t.priority.clone()),
                due_date: ticket.and_then(|t| t.due_date.clone()),
                labels: ticket.map(|t| t.labels.clone()).unwrap_or_default(),
                parent_summary: ticket.and_then(|t| t.parent_summary.clone()),
                updated: ticket.and_then(|t| t.updated.clone()),
                day_seconds: day_seconds
                    .get(key)
                    .map_or_else(|| vec![0; DAYS], |d| d.to_vec()),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.assigned
            .cmp(&a.assigned)
            .then(b.week_seconds.cmp(&a.week_seconds))
            .then_with(|| a.key.cmp(&b.key))
    });

    let last_fetched = conn
        .query_row("SELECT MAX(fetched_at) FROM jira_tickets", [], |r| r.get(0))
        .context("max jira_tickets.fetched_at")?;
    Ok(TasksResponse {
        monday: monday.to_string(),
        today: today.to_string(),
        tasks: rows,
        last_fetched,
    })
}

/// Parked in the Backlog: off the board even with hours this week (they stay on the day/week pages).
fn is_backlog(ticket: &CachedTicket) -> bool {
    ticket
        .status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("backlog"))
}

fn cached_tickets(conn: &Connection) -> Result<HashMap<String, CachedTicket>> {
    let mut statement = conn.prepare(
        "SELECT key, summary, status, status_category,
                external = 0 AND COALESCE(status_category, '') != 'done',
                issue_type, priority, due_date, labels, parent_summary, updated
           FROM jira_tickets",
    )?;
    let rows = statement.query_map([], |r| {
        let category: Option<String> = r.get(3)?;
        Ok((
            r.get::<_, String>(0)?,
            CachedTicket {
                summary: r.get(1)?,
                status: r.get(2)?,
                status_category: category.as_deref().and_then(StatusCategory::parse),
                assigned: r.get(4)?,
                issue_type: r.get(5)?,
                priority: r.get(6)?,
                due_date: r.get(7)?,
                labels: r
                    .get::<_, Option<String>>(8)?
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default(),
                parent_summary: r.get(9)?,
                updated: r.get(10)?,
            },
        ))
    })?;
    rows.collect::<rusqlite::Result<_>>()
        .context("reading jira_tickets")
}

fn last_worked_days(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut statement = conn.prepare(
        "SELECT jira_issue, MAX(day) FROM blocks
          WHERE is_personal = 0 AND COALESCE(jira_issue, '') != ''
          GROUP BY jira_issue",
    )?;
    let rows = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect::<rusqlite::Result<_>>()
        .context("reading last worked days")
}

#[path = "task_board_test.rs"]
#[cfg(test)]
mod tests;
