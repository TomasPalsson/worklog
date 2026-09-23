//! Turns Firefox add-on heartbeats into `events` rows.
//!
//! Filters incognito tabs, the `PERSONAL_CONTAINER` container, and time
//! outside the configured work-hours window, then upserts one event per
//! heartbeat minute. See spec 003 T002.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Datelike, FixedOffset, NaiveTime, SecondsFormat, Timelike, Utc, Weekday};
use rusqlite::{params, Connection};

use crate::models::Event;
use crate::repo;
use crate::routing_contract::{Heartbeat, PERSONAL_CONTAINER, SOURCE_FIREFOX};

/// Editable work-hours window, e.g. `Mon-Fri 09:00-17:00`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkHours {
    start_day: Weekday,
    end_day: Weekday,
    start_time: NaiveTime,
    end_time: NaiveTime,
}

impl WorkHours {
    /// Parse `"<Day>-<Day> <HH:MM>-<HH:MM>"`, e.g. `DEFAULT_WORK_HOURS`.
    pub fn parse(s: &str) -> Result<WorkHours> {
        let (days, times) = s
            .trim()
            .split_once(' ')
            .ok_or_else(|| anyhow!("work hours {s:?}: expected \"<Day>-<Day> <HH:MM>-<HH:MM>\""))?;
        let (start_day_str, end_day_str) = days
            .split_once('-')
            .ok_or_else(|| anyhow!("work hours {s:?}: bad day range {days:?}"))?;
        let start_day: Weekday = start_day_str
            .parse()
            .map_err(|_| anyhow!("work hours {s:?}: unknown day {start_day_str:?}"))?;
        let end_day: Weekday = end_day_str
            .parse()
            .map_err(|_| anyhow!("work hours {s:?}: unknown day {end_day_str:?}"))?;
        let (start_time_str, end_time_str) = times
            .split_once('-')
            .ok_or_else(|| anyhow!("work hours {s:?}: bad time range {times:?}"))?;
        let start_time = NaiveTime::parse_from_str(start_time_str, "%H:%M")
            .with_context(|| format!("work hours {s:?}: bad start time {start_time_str:?}"))?;
        let end_time = NaiveTime::parse_from_str(end_time_str, "%H:%M")
            .with_context(|| format!("work hours {s:?}: bad end time {end_time_str:?}"))?;
        Ok(WorkHours {
            start_day,
            end_day,
            start_time,
            end_time,
        })
    }

    /// True when `ts`, read at the given local `offset`, falls on one of
    /// the configured days within `[start_time, end_time)`.
    pub fn contains(&self, ts: DateTime<Utc>, offset: FixedOffset) -> bool {
        let local = ts.with_timezone(&offset);
        let day = local.weekday();
        let time = local.time();
        day_in_range(self.start_day, self.end_day, day)
            && time >= self.start_time
            && time < self.end_time
    }
}

/// Inclusive day range, wrapping past Sunday (e.g. `Fri-Mon`) so an
/// on-call schedule crossing the week boundary still parses.
fn day_in_range(start: Weekday, end: Weekday, day: Weekday) -> bool {
    let s = start.num_days_from_monday();
    let e = end.num_days_from_monday();
    let d = day.num_days_from_monday();
    if s <= e {
        (s..=e).contains(&d)
    } else {
        d >= s || d <= e
    }
}

/// Result of one ingest attempt.
pub enum IngestOutcome {
    Stored(i64),
    Filtered(&'static str),
}

/// Filter, then upsert one heartbeat as an `events` row.
pub fn ingest_heartbeat(
    conn: &Connection,
    hb: &Heartbeat,
    hours: &WorkHours,
    offset: FixedOffset,
) -> Result<IngestOutcome> {
    if hb.incognito {
        return Ok(IngestOutcome::Filtered("incognito"));
    }
    if hb.container.as_deref() == Some(PERSONAL_CONTAINER) {
        return Ok(IngestOutcome::Filtered("personal_container"));
    }
    if !hours.contains(hb.ts, offset) {
        return Ok(IngestOutcome::Filtered("outside_work_hours"));
    }

    let minute = hb
        .ts
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(hb.ts);
    let source_id = minute.to_rfc3339_opts(SecondsFormat::Secs, true);
    let started_at = hb.ts.to_rfc3339_opts(SecondsFormat::Secs, true);

    let mut event = Event::minimal(SOURCE_FIREFOX, source_id, started_at, hb.title.clone());
    event.details = Some(hb.url.clone());
    let id = repo::upsert_event(conn, &event)?;
    conn.execute(
        "UPDATE events SET container = ?1 WHERE id = ?2",
        params![hb.container, id],
    )
    .context("updating events.container")?;
    Ok(IngestOutcome::Stored(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::routing_contract::DEFAULT_WORK_HOURS;
    use chrono::TimeZone;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn hb(ts: DateTime<Utc>) -> Heartbeat {
        Heartbeat {
            ts,
            url: "https://aws.tomasari.is/cert".into(),
            title: "AWS cert study".into(),
            container: Some("Default".into()),
            incognito: false,
        }
    }

    #[test]
    fn parses_default_work_hours() {
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        assert_eq!(hours.start_day, Weekday::Mon);
        assert_eq!(hours.end_day, Weekday::Fri);
        assert_eq!(hours.start_time, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(hours.end_time, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
    }

    #[test]
    fn rejects_malformed_work_hours() {
        assert!(WorkHours::parse("garbage").is_err());
        assert!(WorkHours::parse("Mon-Fri 09:00").is_err());
        assert!(WorkHours::parse("Mon-Nope 09:00-17:00").is_err());
    }

    #[test]
    fn contains_excludes_before_and_after_the_window() {
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        // Tuesday 08:59 and 17:01 UTC — one minute either side of the window.
        let before = Utc.with_ymd_and_hms(2026, 4, 14, 8, 59, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 4, 14, 17, 1, 0).unwrap();
        assert!(!hours.contains(before, utc()));
        assert!(!hours.contains(after, utc()));
    }

    #[test]
    fn contains_includes_inside_the_window() {
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let inside = Utc.with_ymd_and_hms(2026, 4, 14, 9, 0, 0).unwrap();
        assert!(hours.contains(inside, utc()));
    }

    #[test]
    fn contains_excludes_weekend() {
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        // Saturday 2026-04-18, 10:00 — inside the daily window, wrong day.
        let saturday = Utc.with_ymd_and_hms(2026, 4, 18, 10, 0, 0).unwrap();
        assert!(!hours.contains(saturday, utc()));
    }

    #[test]
    fn ingest_stores_heartbeat_with_container_and_details() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let ts = Utc.with_ymd_and_hms(2026, 4, 14, 10, 30, 12).unwrap();
        let outcome = ingest_heartbeat(&conn, &hb(ts), &hours, utc()).unwrap();
        let id = match outcome {
            IngestOutcome::Stored(id) => id,
            IngestOutcome::Filtered(reason) => panic!("expected stored, got filtered: {reason}"),
        };
        let (source, title, details, container): (String, String, Option<String>, Option<String>) =
            conn.query_row(
                "SELECT source, title, details, container FROM events WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(source, SOURCE_FIREFOX);
        assert_eq!(title, "AWS cert study");
        assert_eq!(details.as_deref(), Some("https://aws.tomasari.is/cert"));
        assert_eq!(container.as_deref(), Some("Default"));
    }

    #[test]
    fn ingest_dedupes_two_heartbeats_in_the_same_minute() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let first = Utc.with_ymd_and_hms(2026, 4, 14, 10, 30, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 14, 10, 30, 45).unwrap();
        let id1 = match ingest_heartbeat(&conn, &hb(first), &hours, utc()).unwrap() {
            IngestOutcome::Stored(id) => id,
            IngestOutcome::Filtered(reason) => panic!("expected stored, got filtered: {reason}"),
        };
        let id2 = match ingest_heartbeat(&conn, &hb(second), &hours, utc()).unwrap() {
            IngestOutcome::Stored(id) => id,
            IngestOutcome::Filtered(reason) => panic!("expected stored, got filtered: {reason}"),
        };
        assert_eq!(id1, id2);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ingest_filters_incognito() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let ts = Utc.with_ymd_and_hms(2026, 4, 14, 10, 0, 0).unwrap();
        let mut heartbeat = hb(ts);
        heartbeat.incognito = true;
        let outcome = ingest_heartbeat(&conn, &heartbeat, &hours, utc()).unwrap();
        assert!(matches!(outcome, IngestOutcome::Filtered("incognito")));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn ingest_filters_personal_container() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let ts = Utc.with_ymd_and_hms(2026, 4, 14, 10, 0, 0).unwrap();
        let mut heartbeat = hb(ts);
        heartbeat.container = Some(PERSONAL_CONTAINER.to_string());
        let outcome = ingest_heartbeat(&conn, &heartbeat, &hours, utc()).unwrap();
        assert!(matches!(
            outcome,
            IngestOutcome::Filtered("personal_container")
        ));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn ingest_filters_outside_work_hours() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let ts = Utc.with_ymd_and_hms(2026, 4, 14, 18, 30, 0).unwrap();
        let outcome = ingest_heartbeat(&conn, &hb(ts), &hours, utc()).unwrap();
        assert!(matches!(
            outcome,
            IngestOutcome::Filtered("outside_work_hours")
        ));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
