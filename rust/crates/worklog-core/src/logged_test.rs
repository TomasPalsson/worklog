//! Tests for the Logged range reader and dismissals (spec 014).

use super::*;
use crate::db;
use crate::logged_contract::DayState;
use crate::tempo_hub_contract::{PulledWorklog, RequiredDay};
use crate::tempo_remote::store_range;

const PULLED_AT: &str = "2026-10-01T09:00:00Z";
const HOUR: i64 = 3600;

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn today() -> NaiveDate {
    date("2026-10-04")
}

fn pulled(id: &str, day: &str, seconds: i64) -> PulledWorklog {
    PulledWorklog {
        tempo_worklog_id: id.to_string(),
        day: day.to_string(),
        issue_id: 1,
        seconds,
        description: format!("work {id}"),
    }
}

fn required(day: &str, seconds: i64) -> RequiredDay {
    RequiredDay {
        day: day.to_string(),
        required_seconds: seconds,
    }
}

#[test]
fn empty_store_reads_not_fetched_with_no_pulled_at() {
    let conn = db::open_memory().unwrap();
    let got = logged_range(&conn, date("2026-09-29"), date("2026-10-01"), today()).unwrap();
    assert_eq!(got.days.len(), 3);
    assert!(got.days.iter().all(|d| d.state == DayState::NotFetched));
    assert_eq!(got.pulled_at, None);
    assert_eq!(got.today, "2026-10-04");
}

#[test]
fn past_weekend_with_zero_schedule_row_is_off() {
    let conn = db::open_memory().unwrap();
    let (from, to) = (date("2026-09-28"), date("2026-10-04"));
    store_range(
        &conn,
        from,
        to,
        &[],
        &[required("2026-10-03", 0)],
        PULLED_AT,
    )
    .unwrap();
    let got = logged_range(&conn, from, to, today()).unwrap();
    let saturday = got.days.iter().find(|d| d.day == "2026-10-03").unwrap();
    assert_eq!(saturday.state, DayState::Off);
    assert_eq!(saturday.required_seconds, Some(0));
    assert_eq!(got.days[0].state, DayState::NotFetched);
    assert_eq!(got.pulled_at.as_deref(), Some(PULLED_AT));
}

#[test]
fn under_day_dismisses_and_undismisses() {
    let conn = db::open_memory().unwrap();
    let (from, to) = (date("2026-09-29"), date("2026-09-30"));
    store_range(
        &conn,
        from,
        to,
        &[
            pulled("2", "2026-09-29", HOUR),
            pulled("1", "2026-09-29", HOUR),
        ],
        &[
            required("2026-09-29", 8 * HOUR),
            required("2026-09-30", 8 * HOUR),
        ],
        PULLED_AT,
    )
    .unwrap();

    let got = logged_range(&conn, from, to, today()).unwrap();
    assert_eq!(got.days[0].state, DayState::Under);
    assert_eq!(got.days[0].logged_seconds, 2 * HOUR);
    let ids: Vec<_> = got.days[0]
        .entries
        .iter()
        .map(|e| e.tempo_worklog_id.as_str())
        .collect();
    assert_eq!(ids, ["1", "2"]);
    assert_eq!(got.days[1].logged_seconds, 0);

    dismiss(&conn, from, "  on leave  ", PULLED_AT).unwrap();
    let got = logged_range(&conn, from, to, today()).unwrap();
    assert_eq!(got.days[0].state, DayState::Dismissed);
    assert_eq!(got.days[0].dismissal_reason.as_deref(), Some("on leave"));

    undismiss(&conn, from).unwrap();
    undismiss(&conn, from).unwrap();
    let got = logged_range(&conn, from, to, today()).unwrap();
    assert_eq!(got.days[0].state, DayState::Under);
    assert_eq!(got.days[0].dismissal_reason, None);
}
