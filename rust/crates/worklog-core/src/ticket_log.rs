//! "Log time" on a ticket: one hand-made block, locked to its ticket and
//! marked manual so estimate, auto-merge and re-inference leave it alone
//! (see `infer::persist_blocks` for the event-less guard).

use crate::change_log;
use crate::deild_contract::ChangeSource;
use crate::digest_contract::DAY_COMPRESSED;
use crate::models::Block;
use crate::tempo_hub_contract::HubError;
use crate::tempo_line_contract::TicketOrigin;
use crate::{block_digest, repo, tz};
use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate, NaiveTime, SecondsFormat};
use rusqlite::{params, Connection};
use serde::Deserialize;

pub const MAX_MINUTES: i64 = 720;
pub const MAX_DESCRIPTION_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct LogTimeBody {
    /// Local day, YYYY-MM-DD.
    pub day: String,
    /// Local start time, HH:MM.
    pub start: String,
    pub minutes: i64,
    pub description: String,
}

fn invalid(message: impl Into<String>) -> anyhow::Error {
    HubError::InvalidInput(message.into()).into()
}

pub fn log_time(
    conn: &Connection,
    key: &str,
    body: &LogTimeBody,
    today: NaiveDate,
) -> Result<Block> {
    let day: NaiveDate = body
        .day
        .parse()
        .map_err(|_| invalid(format!("`{}` is not a day (YYYY-MM-DD)", body.day)))?;
    if day > today {
        return Err(invalid(format!("{day} is in the future")));
    }
    let start = NaiveTime::parse_from_str(&body.start, "%H:%M")
        .map_err(|_| invalid(format!("`{}` is not a start time (HH:MM)", body.start)))?;
    if !(1..=MAX_MINUTES).contains(&body.minutes) {
        return Err(invalid(format!(
            "length must be 1 to {MAX_MINUTES} minutes"
        )));
    }
    let description = body.description.trim();
    match description.chars().count() {
        0 => return Err(invalid("description is empty")),
        n if n > MAX_DESCRIPTION_CHARS => {
            return Err(invalid(format!(
                "description is {n} characters; the limit is {MAX_DESCRIPTION_CHARS}"
            )))
        }
        _ => {}
    }
    let day_iso = day.to_string();
    if block_digest::day_is_compressed(conn, &day_iso)? {
        return Err(anyhow!("{DAY_COMPRESSED}: {day_iso}"));
    }
    let started = day
        .and_time(start)
        .and_local_timezone(tz::day_offset())
        .single()
        .ok_or_else(|| invalid("start time does not exist"))?
        .to_utc();
    let ended = started + Duration::minutes(body.minutes);
    let iso = |t: chrono::DateTime<chrono::Utc>| t.to_rfc3339_opts(SecondsFormat::AutoSi, false);
    conn.execute(
        "INSERT INTO blocks (
            day, jira_issue, started_at, ended_at, duration_seconds,
            description, estimated_by, ticket_origin, described_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', ?7, ?5)",
        params![
            day_iso,
            key,
            iso(started),
            iso(ended),
            body.minutes * 60,
            description,
            TicketOrigin::Manual.as_str(),
        ],
    )?;
    let id = conn.last_insert_rowid();
    change_log::refresh_day_logged(conn, &day_iso, ChangeSource::User);
    repo::get_block(conn, id)?.ok_or_else(|| anyhow!("logged block {id} vanished"))
}

#[path = "ticket_log_test.rs"]
#[cfg(test)]
mod tests;
