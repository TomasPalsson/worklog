//! The recap after the 17:00 auto-send run (spec 018 FR-43..45): what went
//! out, what was held back and why, how much of the work-hours window the
//! blocks cover, and the longest stretches nothing explains.

use crate::auto_send::block_ready;
use crate::billing::{block_interval, union_seconds};
use crate::browser_ingest::WorkHours;
use crate::clues_contract::LineTextOrigin;
use crate::daily_helpers_contract::{
    GapAction, HeldBackLine, Recap, RecapGap, RecapLine, RECAP_MIN_GAP_SECONDS, RECAP_TOP_GAPS,
};
use crate::routing_contract::{DEFAULT_WORK_HOURS, WORK_HOURS_KEY};
use crate::tempo_line_contract::{TempoLine, TempoLineKey};
use crate::ticket_log::{self, LogTimeBody};
use crate::verdict_contract::LineCheck;
use crate::{envfile, tempo_lines, tempo_match, timeline, tz};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

fn work_hours() -> String {
    envfile::read(WORK_HOURS_KEY).unwrap_or_else(|| DEFAULT_WORK_HOURS.to_owned())
}

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap_or_default()
}

fn iso(secs: i64) -> String {
    ts(secs).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The day's work-hours window as UTC timestamps; `None` when `day` is not a
/// work day or the hours do not parse.
fn window(day: &str, work_hours: &str) -> Option<(i64, i64)> {
    let hours = WorkHours::parse(work_hours).ok()?;
    let (_, times) = work_hours.trim().split_once(' ')?;
    let (start, end) = times.split_once('-')?;
    let date: NaiveDate = day.parse().ok()?;
    let offset = tz::day_offset();
    let at = |t: &str| {
        let time = NaiveTime::parse_from_str(t, "%H:%M").ok()?;
        Some(
            date.and_time(time)
                .and_local_timezone(offset)
                .single()?
                .timestamp(),
        )
    };
    let (start, end) = (at(start)?, at(end)?);
    (start < end && hours.contains(ts(start), offset)).then_some((start, end))
}

fn clip((s, e): (i64, i64), (ws, we): (i64, i64)) -> Option<(i64, i64)> {
    let (s, e) = (s.max(ws), e.min(we));
    (s < e).then_some((s, e))
}

/// Why a line the run did not send was held back; `None` when it was not
/// the run's to send (already synced earlier, or no hours).
fn held_reason(
    conn: &Connection,
    line: &TempoLine,
    error: Option<String>,
) -> Result<Option<String>> {
    if error.is_some() {
        return Ok(error);
    }
    if tempo_match::is_already(conn, line) {
        return Ok(Some("already in Tempo".into()));
    }
    let key = TempoLineKey {
        day: line.day.clone(),
        jira_issue: line.jira_issue.clone(),
    };
    let blocks = tempo_lines::blocks_for_ticket(conn, &key)?;
    let unsent = blocks
        .iter()
        .any(|b| b.dirty || b.tempo_worklog_id.as_deref().unwrap_or("").is_empty());
    if !unsent || line.effective_seconds == 0 {
        return Ok(None);
    }
    if line.text.is_none() {
        return Ok(Some("no text".into()));
    }
    let own_text = line.text_origin == Some(LineTextOrigin::Manual);
    if !own_text && line.check_status != Some(LineCheck::Passed) {
        return Ok(Some("text needs a look".into()));
    }
    for block in &blocks {
        if !block_ready(conn, block, &line.jira_issue)? {
            return Ok(Some("ticket not confirmed".into()));
        }
    }
    Ok(Some("not sent".into()))
}

fn sent_and_held(conn: &Connection, day: &str) -> Result<(Vec<RecapLine>, Vec<HeldBackLine>)> {
    let (mut sent, mut held) = (Vec::new(), Vec::new());
    for line in tempo_lines::lines_for_day(conn, day)? {
        let (sent_at, error): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT auto_sent_at, send_error FROM tempo_line_texts
                  WHERE day = ?1 AND jira_issue = ?2",
                params![line.day, line.jira_issue],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or_default();
        if sent_at.is_some() && error.is_none() {
            sent.push(RecapLine {
                jira_issue: line.jira_issue,
                seconds: line.effective_seconds,
            });
        } else if let Some(reason) = held_reason(conn, &line, error)? {
            held.push(HeldBackLine {
                jira_issue: line.jira_issue,
                reason,
            });
        }
    }
    Ok((sent, held))
}

fn breaks(conn: &Connection, day: &str) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare("SELECT started_at, ended_at FROM recap_breaks WHERE day = ?1")?;
    let rows = stmt
        .query_map([day], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .filter_map(|(s, e)| {
            let t = |x: &str| DateTime::parse_from_rfc3339(x).ok().map(|d| d.timestamp());
            Some((t(s)?, t(e)?))
        })
        .collect())
}

/// The recap for `day`; `None` when `day` has no work-hours window.
pub fn build(conn: &Connection, day: &str, work_hours: &str) -> Result<Option<Recap>> {
    let Some(win) = window(day, work_hours) else {
        return Ok(None);
    };
    let (sent, held_back) = sent_and_held(conn, day)?;
    let covered: Vec<(i64, i64)> = crate::repo::list_blocks_for_day(conn, day)?
        .iter()
        .filter(|b| b.ignored_at.is_none())
        .filter_map(|b| clip(block_interval(b), win))
        .collect();
    let coverage = union_seconds(covered.clone()) as f64 / (win.1 - win.0) as f64 * 100.0;

    // Window edges and recorded breaks count as occupied so gaps run between them.
    let mut occupied = covered;
    occupied.extend(breaks(conn, day)?.into_iter().filter_map(|b| clip(b, win)));
    occupied.extend([(win.0, win.0), (win.1, win.1)]);
    let occupied: Vec<_> = occupied.iter().map(|&(s, e)| (ts(s), ts(e))).collect();
    let mut gaps: Vec<(i64, i64)> =
        timeline::day_gaps(&occupied, Duration::seconds(RECAP_MIN_GAP_SECONDS))
            .into_iter()
            .map(|(s, e)| (s.timestamp(), e.timestamp()))
            .collect();
    gaps.sort_by_key(|&(s, e)| (-(e - s), s));
    gaps.truncate(RECAP_TOP_GAPS);

    Ok(Some(Recap {
        day: day.to_owned(),
        sent,
        held_back,
        coverage_percent: coverage.round() as u32,
        gaps: gaps
            .into_iter()
            .map(|(s, e)| RecapGap {
                started_at: iso(s),
                ended_at: iso(e),
                minutes: (e - s) / 60,
            })
            .collect(),
    }))
}

pub fn build_and_store(conn: &Connection, day: &str, work_hours: &str) -> Result<()> {
    let Some(recap) = build(conn, day, work_hours)? else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO recaps (day, json, built_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(day) DO UPDATE SET json = excluded.json, built_at = excluded.built_at",
        params![day, serde_json::to_string(&recap)?, Utc::now().to_rfc3339()],
    )
    .context("storing recap")?;
    Ok(())
}

/// The most recently built recap.
pub fn latest(conn: &Connection) -> Result<Option<Recap>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM recaps ORDER BY day DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
}

pub fn apply_gap(
    conn: &Connection,
    day: &str,
    gap_started_at: &str,
    action: GapAction,
) -> Result<()> {
    apply_gap_with(conn, day, gap_started_at, action, &work_hours())
}

/// Resolves the gap of `day`'s stored recap that starts at `gap_started_at`,
/// then rebuilds the recap. Only `PickTicket` adds billable time.
pub fn apply_gap_with(
    conn: &Connection,
    day: &str,
    gap_started_at: &str,
    action: GapAction,
    work_hours: &str,
) -> Result<()> {
    let stored: Option<String> = conn
        .query_row("SELECT json FROM recaps WHERE day = ?1", [day], |r| {
            r.get(0)
        })
        .optional()?;
    let recap: Recap = serde_json::from_str(&stored.ok_or_else(|| anyhow!("no recap for {day}"))?)?;
    let gap = recap
        .gaps
        .iter()
        .find(|g| g.started_at == gap_started_at)
        .ok_or_else(|| anyhow!("no gap starting {gap_started_at} in the recap for {day}"))?;
    let start = DateTime::parse_from_rfc3339(&gap.started_at)?.with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(&gap.ended_at)?.with_timezone(&Utc);
    match action {
        GapAction::Personal => {
            let at = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::AutoSi, false);
            conn.execute(
                "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, estimated_by, is_personal)
                 VALUES (?1, ?2, ?3, ?4, 'manual', 1)",
                params![day, at(start), at(end), (end - start).num_seconds()],
            )?;
        }
        GapAction::Break => {
            conn.execute(
                "INSERT OR REPLACE INTO recap_breaks (day, started_at, ended_at) VALUES (?1, ?2, ?3)",
                params![day, gap.started_at, gap.ended_at],
            )?;
        }
        GapAction::PickTicket { jira_issue } => {
            let body = LogTimeBody {
                day: day.to_owned(),
                start: start
                    .with_timezone(&tz::day_offset())
                    .format("%H:%M")
                    .to_string(),
                minutes: gap.minutes,
                description: "Time added from the 17:00 recap".to_owned(),
            };
            ticket_log::log_time(conn, &jira_issue, &body, day.parse()?)?;
        }
    }
    build_and_store(conn, day, work_hours)
}

#[path = "recap_test.rs"]
#[cfg(test)]
mod tests;
