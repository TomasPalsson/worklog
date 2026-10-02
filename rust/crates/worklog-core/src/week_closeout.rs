//! Week close-out (spec 012): per day, what is logged, synced, in Tempo,
//! required and still untagged.

use crate::tempo_hub_contract::{CloseoutDay, WeekCloseout};
use crate::tempo_lines;
use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::{Connection, OptionalExtension};

pub fn week_closeout(conn: &Connection, monday: NaiveDate) -> Result<WeekCloseout> {
    let days = (0..7)
        .map(|offset| closeout_day(conn, &(monday + Duration::days(offset)).to_string()))
        .collect::<Result<Vec<_>>>()?;
    let first = monday.to_string();
    let last = (monday + Duration::days(6)).to_string();
    let pulled_at = conn
        .query_row(
            "SELECT MAX(pulled_at) FROM tempo_remote_worklogs WHERE day BETWEEN ?1 AND ?2",
            [&first, &last],
            |r| r.get(0),
        )
        .context("latest pulled_at")?;
    Ok(WeekCloseout {
        monday: first,
        days,
        pulled_at,
    })
}

pub(crate) fn closeout_day(conn: &Connection, day: &str) -> Result<CloseoutDay> {
    let mut logged_seconds = 0;
    let mut synced_seconds = 0;
    let mut pending_lines = 0;
    for line in tempo_lines::lines_for_day(conn, day)? {
        logged_seconds += line.effective_seconds;
        let unsettled: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks
                  WHERE day = ?1 AND jira_issue = ?2 AND is_personal = 0
                    AND (COALESCE(tempo_worklog_id, '') = '' OR dirty = 1)",
                [day, line.jira_issue.as_str()],
                |r| r.get(0),
            )
            .context("pending blocks")?;
        if unsettled == 0 {
            synced_seconds += line.effective_seconds;
        } else {
            pending_lines += 1;
        }
    }
    let (tempo_seconds, outside_seconds) = conn
        .query_row(
            "SELECT COALESCE(SUM(seconds), 0),
                    COALESCE(SUM(CASE WHEN owner = 'outside' THEN seconds END), 0)
               FROM tempo_remote_worklogs WHERE day = ?1",
            [day],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .context("pulled seconds")?;
    let required_seconds = conn
        .query_row(
            "SELECT required_seconds FROM tempo_required_days WHERE day = ?1",
            [day],
            |r| r.get(0),
        )
        .optional()
        .context("required seconds")?;
    let unticketed_seconds = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_seconds), 0) FROM blocks
              WHERE day = ?1 AND COALESCE(jira_issue, '') = ''
                AND is_personal = 0 AND ignored_at IS NULL",
            [day],
            |r| r.get(0),
        )
        .context("unticketed seconds")?;
    Ok(CloseoutDay {
        day: day.to_string(),
        logged_seconds,
        synced_seconds,
        tempo_seconds,
        outside_seconds,
        required_seconds,
        unticketed_seconds,
        pending_lines,
    })
}

#[path = "week_closeout_test.rs"]
#[cfg(test)]
mod tests;
