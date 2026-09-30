//! Turns Firefox add-on heartbeats into `events` rows.
//!
//! Filters incognito tabs, the `PERSONAL_CONTAINER` container, and time
//! outside the configured work-hours window, then upserts one event per
//! heartbeat minute. See spec 003 T002.

use anyhow::{anyhow, Context, Result};
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveTime, SecondsFormat, TimeZone, Timelike, Utc,
    Weekday,
};
use rusqlite::{params, Connection};

use crate::models::Event;
use crate::purge::{meta_get, meta_set};
use crate::repo;
use crate::routing_contract::{
    Heartbeat, BROWSER_RECORDING_UNTIL_KEY, PERSONAL_CONTAINER, SOURCE_FIREFOX,
};

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

    /// End of the work day after the local day `now` falls on, as a UTC instant.
    pub fn auto_stop_at(&self, now: DateTime<Utc>, offset: FixedOffset) -> DateTime<Utc> {
        let next_day = now.with_timezone(&offset).date_naive() + Duration::days(1);
        offset
            .from_local_datetime(&next_day.and_time(self.end_time))
            .unwrap()
            .with_timezone(&Utc)
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
    recording_until: Option<DateTime<Utc>>,
) -> Result<IngestOutcome> {
    if hb.incognito {
        return Ok(IngestOutcome::Filtered("incognito"));
    }
    if hb.container.as_deref() == Some(PERSONAL_CONTAINER) {
        return Ok(IngestOutcome::Filtered("personal_container"));
    }
    let recording = recording_until.is_some_and(|until| hb.ts < until);
    if !recording && !hours.contains(hb.ts, offset) {
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

/// The stored recording-override end, or `None` when missing, unparseable
/// or already past.
pub fn recording_until(conn: &Connection, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
    let Some(raw) = meta_get(conn, BROWSER_RECORDING_UNTIL_KEY)? else {
        return Ok(None);
    };
    let until = match DateTime::parse_from_rfc3339(&raw) {
        Ok(parsed) => parsed.with_timezone(&Utc),
        Err(err) => {
            eprintln!("worklog: ignoring unparseable {BROWSER_RECORDING_UNTIL_KEY} {raw:?}: {err}");
            return Ok(None);
        }
    };
    Ok((until > now).then_some(until))
}

/// Store the recording-override end; `None` deletes it.
pub fn set_recording(conn: &Connection, until: Option<DateTime<Utc>>) -> Result<()> {
    match until {
        Some(until) => meta_set(
            conn,
            BROWSER_RECORDING_UNTIL_KEY,
            &until.to_rfc3339_opts(SecondsFormat::Secs, true),
        ),
        None => {
            conn.execute(
                "DELETE FROM meta WHERE key = ?1",
                params![BROWSER_RECORDING_UNTIL_KEY],
            )
            .context("clearing recording override")?;
            Ok(())
        }
    }
}

/// Firefox events started on the local day `now` falls on (one per minute).
pub fn minutes_today(conn: &Connection, now: DateTime<Utc>, offset: FixedOffset) -> Result<i64> {
    let midnight = now
        .with_timezone(&offset)
        .date_naive()
        .and_time(NaiveTime::MIN);
    let start = offset
        .from_local_datetime(&midnight)
        .unwrap()
        .with_timezone(&Utc);
    let end = start + Duration::days(1);
    conn.query_row(
        "SELECT COUNT(*) FROM events WHERE source = ?1 AND started_at >= ?2 AND started_at < ?3",
        params![
            SOURCE_FIREFOX,
            start.to_rfc3339_opts(SecondsFormat::Secs, true),
            end.to_rfc3339_opts(SecondsFormat::Secs, true)
        ],
        |r| r.get(0),
    )
    .context("counting today's browser minutes")
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
        let outcome = ingest_heartbeat(&conn, &hb(ts), &hours, utc(), None).unwrap();
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
        let id1 = match ingest_heartbeat(&conn, &hb(first), &hours, utc(), None).unwrap() {
            IngestOutcome::Stored(id) => id,
            IngestOutcome::Filtered(reason) => panic!("expected stored, got filtered: {reason}"),
        };
        let id2 = match ingest_heartbeat(&conn, &hb(second), &hours, utc(), None).unwrap() {
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
        let outcome = ingest_heartbeat(&conn, &heartbeat, &hours, utc(), None).unwrap();
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
        let outcome = ingest_heartbeat(&conn, &heartbeat, &hours, utc(), None).unwrap();
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
    fn ingest_preserves_routing_label_on_second_heartbeat_same_minute() {
        // A second heartbeat in the same minute bucket upserts the same
        // row (dedupe key: source + minute-truncated source_id). Routing
        // (T005) may have already labelled that row in between — the
        // re-upsert must not wipe project_path back to NULL.
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let first = Utc.with_ymd_and_hms(2026, 4, 14, 10, 30, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 14, 10, 30, 45).unwrap();
        let id = match ingest_heartbeat(&conn, &hb(first), &hours, utc(), None).unwrap() {
            IngestOutcome::Stored(id) => id,
            IngestOutcome::Filtered(reason) => panic!("expected stored, got filtered: {reason}"),
        };
        conn.execute(
            "UPDATE events SET project_path = '/Work/aws-cert', label_origin = 'rule' WHERE id = ?1",
            params![id],
        )
        .unwrap();

        ingest_heartbeat(&conn, &hb(second), &hours, utc(), None).unwrap();

        let (project_path, label_origin): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT project_path, label_origin FROM events WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(project_path.as_deref(), Some("/Work/aws-cert"));
        assert_eq!(label_origin.as_deref(), Some("rule"));
    }

    #[test]
    fn ingest_filters_outside_work_hours() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let ts = Utc.with_ymd_and_hms(2026, 4, 14, 18, 30, 0).unwrap();
        let outcome = ingest_heartbeat(&conn, &hb(ts), &hours, utc(), None).unwrap();
        assert!(matches!(
            outcome,
            IngestOutcome::Filtered("outside_work_hours")
        ));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    fn saturday_noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap()
    }

    #[test]
    fn recording_override_bypasses_work_hours_only_before_its_end() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 4, 18, 13, 0, 0).unwrap();
        let inside = ingest_heartbeat(&conn, &hb(saturday_noon()), &hours, utc(), Some(until));
        assert!(matches!(inside.unwrap(), IngestOutcome::Stored(_)));
        let at_end = ingest_heartbeat(&conn, &hb(until), &hours, utc(), Some(until)).unwrap();
        assert!(matches!(
            at_end,
            IngestOutcome::Filtered("outside_work_hours")
        ));
    }

    #[test]
    fn recording_override_still_filters_incognito() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 4, 19, 17, 0, 0).unwrap();
        let mut heartbeat = hb(saturday_noon());
        heartbeat.incognito = true;
        let outcome = ingest_heartbeat(&conn, &heartbeat, &hours, utc(), Some(until)).unwrap();
        assert!(matches!(outcome, IngestOutcome::Filtered("incognito")));
    }

    #[test]
    fn auto_stop_is_next_local_day_at_work_end() {
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        // Sat 23:30 local (+02:00) is still Sat, so the stop is Sun 17:00 local = 15:00Z.
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 21, 30, 0).unwrap();
        assert_eq!(
            hours.auto_stop_at(now, plus_two),
            Utc.with_ymd_and_hms(2026, 4, 19, 15, 0, 0).unwrap()
        );
        // Same instant read in UTC is Sat 21:30, stop Sun 17:00Z.
        assert_eq!(
            hours.auto_stop_at(now, utc()),
            Utc.with_ymd_and_hms(2026, 4, 19, 17, 0, 0).unwrap()
        );
    }

    #[test]
    fn recording_until_round_trips_and_expires() {
        let conn = db::open_memory().unwrap();
        let now = saturday_noon();
        assert_eq!(recording_until(&conn, now).unwrap(), None);
        let until = now + Duration::hours(2);
        set_recording(&conn, Some(until)).unwrap();
        assert_eq!(recording_until(&conn, now).unwrap(), Some(until));
        assert_eq!(recording_until(&conn, until).unwrap(), None);
        set_recording(&conn, None).unwrap();
        assert_eq!(recording_until(&conn, now).unwrap(), None);
    }

    #[test]
    fn recording_until_ignores_garbage() {
        let conn = db::open_memory().unwrap();
        meta_set(&conn, BROWSER_RECORDING_UNTIL_KEY, "not a date").unwrap();
        assert_eq!(recording_until(&conn, saturday_noon()).unwrap(), None);
    }

    #[test]
    fn minutes_today_counts_only_the_local_day() {
        let conn = db::open_memory().unwrap();
        let hours = WorkHours::parse(DEFAULT_WORK_HOURS).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 4, 30, 0, 0, 0).unwrap();
        let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
        // Local Sat 2026-04-18 spans 2026-04-17T22:00Z .. 2026-04-18T22:00Z.
        for (day, hour, minute) in [
            (17, 21, 59),
            (17, 22, 0),
            (17, 22, 1),
            (18, 21, 59),
            (18, 22, 0),
        ] {
            let ts = Utc.with_ymd_and_hms(2026, 4, day, hour, minute, 0).unwrap();
            ingest_heartbeat(&conn, &hb(ts), &hours, utc(), Some(until)).unwrap();
        }
        assert_eq!(minutes_today(&conn, saturday_noon(), plus_two).unwrap(), 3);
    }
}
