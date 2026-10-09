use crate::estimate_progress_contract::{
    PersonHours, RawWorklog, TicketProgress, PROGRESS_STALE_SECS,
};
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

const EPOCH: &str = "1970-01-01T00:00:00+00:00";

pub fn store_ticket(
    conn: &Connection,
    key: &str,
    estimate: Option<i64>,
    logs: &[RawWorklog],
    now: DateTime<Utc>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM ticket_progress_worklogs WHERE key = ?1", [key])?;
    for l in logs {
        tx.execute(
            "INSERT OR REPLACE INTO ticket_progress_worklogs
                 (worklog_id, key, account_id, name, day, seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                l.worklog_id,
                key,
                l.account_id,
                l.name,
                l.day.to_string(),
                l.seconds
            ],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO ticket_progress (key, estimate_seconds, logged_seconds, pulled_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            key,
            estimate,
            logs.iter().map(|l| l.seconds).sum::<i64>(),
            now.to_rfc3339()
        ],
    )?;
    Ok(tx.commit()?)
}

pub fn stale_keys(conn: &Connection, keys: &[String], now: DateTime<Utc>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for key in keys {
        let pulled: Option<String> = conn
            .query_row(
                "SELECT pulled_at FROM ticket_progress WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        let stale = match pulled.and_then(|p| DateTime::parse_from_rfc3339(&p).ok()) {
            Some(p) => (now - p.with_timezone(&Utc)).num_seconds() > PROGRESS_STALE_SECS,
            None => true,
        };
        if stale {
            out.push(key.clone());
        }
    }
    Ok(out)
}

pub fn mark_stale(conn: &Connection, keys: &[String]) -> Result<()> {
    for key in keys {
        conn.execute(
            "UPDATE ticket_progress SET pulled_at = ?2 WHERE key = ?1",
            params![key, EPOCH],
        )?;
    }
    Ok(())
}

pub fn day_ticket_keys(conn: &Connection, day: NaiveDate) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT jira_issue FROM blocks
         WHERE day = ?1 AND is_personal = 0 AND jira_issue IS NOT NULL AND jira_issue != ''
         ORDER BY jira_issue",
    )?;
    let rows = stmt.query_map([day.to_string()], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn view(conn: &Connection, keys: &[String], me: Option<&str>) -> Result<Vec<TicketProgress>> {
    keys.iter().map(|key| ticket_view(conn, key, me)).collect()
}

fn ticket_view(conn: &Connection, key: &str, me: Option<&str>) -> Result<TicketProgress> {
    let head: Option<(Option<i64>, String)> = conn
        .query_row(
            "SELECT estimate_seconds, pulled_at FROM ticket_progress WHERE key = ?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let mut stmt = conn.prepare(
        "SELECT account_id, name, day, seconds FROM ticket_progress_worklogs WHERE key = ?1",
    )?;
    let rows = stmt.query_map([key], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut by_person: BTreeMap<String, (String, BTreeMap<NaiveDate, i64>)> = BTreeMap::new();
    for row in rows {
        let (account, name, day, seconds) = row?;
        let entry = by_person.entry(account).or_insert((name, BTreeMap::new()));
        *entry.1.entry(day.parse()?).or_insert(0) += seconds;
    }
    let mut people: Vec<PersonHours> = by_person
        .into_iter()
        .map(|(account_id, (name, days))| PersonHours {
            is_you: me == Some(account_id.as_str()),
            seconds: days.values().sum(),
            by_day: days.into_iter().collect(),
            account_id,
            name,
        })
        .collect();
    people.sort_by(|a, b| b.is_you.cmp(&a.is_you).then(b.seconds.cmp(&a.seconds)));
    Ok(TicketProgress {
        key: key.to_string(),
        estimate_seconds: head.as_ref().and_then(|h| h.0),
        logged_seconds: people.iter().map(|p| p.seconds).sum(),
        people,
        pulled_at: head.map(|h| h.1),
        error: None,
    })
}

#[cfg(test)]
#[path = "ticket_progress_test.rs"]
mod tests;
