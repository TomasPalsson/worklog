//! Spec 012 Tempo hub: stored read-back of the Owner's Tempo week.

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection};

use crate::tempo_hub_contract::{
    PullReport, PulledWorklog, RemoteWorklog, RequiredDay, WorklogOwner,
};

/// Replaces the week's pulled rows in one transaction. A worklog is the
/// tool's own iff a block carries its id in `tempo_worklog_id`.
pub fn store_week(
    conn: &Connection,
    monday: NaiveDate,
    worklogs: &[PulledWorklog],
    schedule: &[RequiredDay],
    pulled_at: &str,
) -> Result<PullReport> {
    let (from, to) = week_bounds(monday);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM tempo_remote_worklogs WHERE day BETWEEN ?1 AND ?2",
        params![from, to],
    )?;
    tx.execute(
        "DELETE FROM tempo_required_days WHERE day BETWEEN ?1 AND ?2",
        params![from, to],
    )?;
    let mut outside = 0;
    for w in worklogs {
        let owner: String = tx.query_row(
            "SELECT CASE WHEN EXISTS (SELECT 1 FROM blocks WHERE tempo_worklog_id = ?1)
                    THEN 'worklog' ELSE 'outside' END",
            params![w.tempo_worklog_id],
            |r| r.get(0),
        )?;
        outside += usize::from(owner == WorklogOwner::Outside.as_str());
        tx.execute(
            "INSERT INTO tempo_remote_worklogs
                (tempo_worklog_id, day, issue_id, jira_issue, seconds, description, owner, pulled_at)
             VALUES (?1, ?2, ?3,
                (SELECT key FROM jira_tickets WHERE issue_id = CAST(?3 AS TEXT)),
                ?4, ?5, ?6, ?7)
             ON CONFLICT(tempo_worklog_id) DO UPDATE SET
                day = excluded.day, issue_id = excluded.issue_id,
                jira_issue = excluded.jira_issue, seconds = excluded.seconds,
                description = excluded.description, owner = excluded.owner,
                pulled_at = excluded.pulled_at",
            params![
                w.tempo_worklog_id,
                w.day,
                w.issue_id,
                w.seconds,
                w.description,
                owner,
                pulled_at
            ],
        )
        .context("insert tempo_remote_worklogs")?;
    }
    for d in schedule {
        tx.execute(
            "INSERT INTO tempo_required_days (day, required_seconds, pulled_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(day) DO UPDATE SET
                required_seconds = excluded.required_seconds, pulled_at = excluded.pulled_at",
            params![d.day, d.required_seconds, pulled_at],
        )
        .context("insert tempo_required_days")?;
    }
    tx.commit()?;
    Ok(PullReport {
        monday: from,
        worklogs: worklogs.len(),
        outside,
        schedule_days: schedule.len(),
        pulled_at: pulled_at.to_string(),
    })
}

pub fn list_week(conn: &Connection, monday: NaiveDate) -> Result<Vec<RemoteWorklog>> {
    let (from, to) = week_bounds(monday);
    let mut stmt = conn.prepare(
        "SELECT tempo_worklog_id, day, issue_id, jira_issue, seconds, description, owner
         FROM tempo_remote_worklogs WHERE day BETWEEN ?1 AND ?2
         ORDER BY day, tempo_worklog_id",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        let owner: String = r.get(6)?;
        Ok(RemoteWorklog {
            tempo_worklog_id: r.get(0)?,
            day: r.get(1)?,
            issue_id: r.get(2)?,
            jira_issue: r.get(3)?,
            seconds: r.get(4)?,
            description: r.get(5)?,
            owner: WorklogOwner::parse(&owner).unwrap_or(WorklogOwner::Outside),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn week_bounds(monday: NaiveDate) -> (String, String) {
    (monday.to_string(), (monday + Duration::days(6)).to_string())
}

#[path = "tempo_remote_test.rs"]
#[cfg(test)]
mod tests;
