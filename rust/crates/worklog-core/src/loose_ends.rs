//! Spec 021 FR-11: ticketed work in the current billing cycle that never
//! reached Tempo or an invoice.

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;

use crate::purge::{configured_cycle_start_day, cycle_start_on_or_before};
use crate::time_leaks_contract::{LooseEnd, LooseEndMissing, LooseEnds};

pub fn for_cycle(conn: &Connection, today: &str) -> Result<LooseEnds> {
    for_cycle_with(conn, today, configured_cycle_start_day())
}

fn for_cycle_with(conn: &Connection, today: &str, start_day: u32) -> Result<LooseEnds> {
    let today: NaiveDate = today
        .parse()
        .with_context(|| format!("parsing today {today:?}"))?;
    let start = cycle_start_on_or_before(today, start_day);
    // 32 days always lands inside the next cycle, whatever the month lengths.
    let end = cycle_start_on_or_before(start + Duration::days(32), start_day) - Duration::days(1);
    let (start, end) = (start.to_string(), end.to_string());
    let mut stmt = conn.prepare(
        "SELECT id, day, jira_issue, started_at, ended_at, duration_seconds, description,
                COALESCE(tempo_worklog_id, '') = '', COALESCE(exported_at, '') = ''
         FROM blocks
         WHERE day BETWEEN ?1 AND ?2
           AND COALESCE(jira_issue, '') <> ''
           AND is_personal = 0 AND ignored_at IS NULL
           AND (COALESCE(tempo_worklog_id, '') = '' OR COALESCE(exported_at, '') = '')
         ORDER BY started_at, id",
    )?;
    let items = stmt
        .query_map([&start, &end], |r| {
            let mut missing = Vec::new();
            if r.get::<_, bool>(7)? {
                missing.push(LooseEndMissing::Tempo);
            }
            if r.get::<_, bool>(8)? {
                missing.push(LooseEndMissing::Export);
            }
            Ok(LooseEnd {
                block_id: r.get(0)?,
                day: r.get(1)?,
                jira_issue: r.get(2)?,
                started_at: r.get(3)?,
                ended_at: r.get(4)?,
                seconds: r.get(5)?,
                description: r.get(6)?,
                missing,
            })
        })?
        .collect::<rusqlite::Result<_>>()
        .context("listing loose ends")?;
    Ok(LooseEnds {
        cycle_start: start,
        cycle_end: end,
        items,
    })
}

#[cfg(test)]
#[path = "loose_ends_test.rs"]
mod tests;
