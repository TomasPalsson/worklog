//! Spec 014 Logged section: stored Tempo entries per day with a flag state.

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use crate::logged_contract::{day_state, LoggedDay, LoggedEntry, LoggedRange};
use crate::tempo_hub_contract::WorklogOwner;

pub fn logged_range(
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
) -> Result<LoggedRange> {
    let mut days = Vec::new();
    let mut day = from;
    while day <= to {
        days.push(logged_day(conn, day, today)?);
        day += Duration::days(1);
    }
    let (from_text, to_text) = (from.to_string(), to.to_string());
    let pulled_at = conn.query_row(
        "SELECT MAX(p) FROM (
            SELECT pulled_at AS p FROM tempo_remote_worklogs WHERE day BETWEEN ?1 AND ?2
            UNION ALL
            SELECT pulled_at FROM tempo_required_days WHERE day BETWEEN ?1 AND ?2)",
        params![from_text, to_text],
        |r| r.get(0),
    )?;
    Ok(LoggedRange {
        from: from_text,
        to: to_text,
        today: today.to_string(),
        days,
        pulled_at,
    })
}

fn logged_day(conn: &Connection, day: NaiveDate, today: NaiveDate) -> Result<LoggedDay> {
    let day_text = day.to_string();
    let mut stmt = conn.prepare(
        "SELECT tempo_worklog_id, issue_id, jira_issue, seconds, description, owner
         FROM tempo_remote_worklogs WHERE day = ?1 ORDER BY tempo_worklog_id",
    )?;
    let entries: Vec<LoggedEntry> = stmt
        .query_map(params![day_text], |r| {
            let owner: String = r.get(5)?;
            Ok(LoggedEntry {
                tempo_worklog_id: r.get(0)?,
                issue_id: r.get(1)?,
                jira_issue: r.get(2)?,
                seconds: r.get(3)?,
                description: r.get(4)?,
                owner: WorklogOwner::parse(&owner).unwrap_or(WorklogOwner::Outside),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let required_seconds: Option<i64> = conn
        .query_row(
            "SELECT required_seconds FROM tempo_required_days WHERE day = ?1",
            params![day_text],
            |r| r.get(0),
        )
        .optional()?;
    let dismissal_reason: Option<String> = conn
        .query_row(
            "SELECT reason FROM tempo_day_dismissals WHERE day = ?1",
            params![day_text],
            |r| r.get(0),
        )
        .optional()?;
    let logged_seconds = entries.iter().map(|e| e.seconds).sum();
    let state = day_state(
        day,
        today,
        logged_seconds,
        required_seconds,
        entries.len(),
        dismissal_reason.is_some(),
    );
    Ok(LoggedDay {
        day: day_text,
        logged_seconds,
        required_seconds,
        state,
        dismissal_reason,
        entries,
    })
}

pub fn dismiss(conn: &Connection, day: NaiveDate, reason: &str, now: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO tempo_day_dismissals (day, reason, dismissed_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(day) DO UPDATE SET
            reason = excluded.reason, dismissed_at = excluded.dismissed_at",
        params![day.to_string(), reason.trim(), now],
    )?;
    Ok(())
}

pub fn undismiss(conn: &Connection, day: NaiveDate) -> Result<()> {
    conn.execute(
        "DELETE FROM tempo_day_dismissals WHERE day = ?1",
        params![day.to_string()],
    )?;
    Ok(())
}

#[path = "logged_test.rs"]
#[cfg(test)]
mod tests;
