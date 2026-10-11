use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::purge::meta_get;
use crate::repo;
use chrono::{Duration, TimeZone};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap()
}

fn seed(conn: &Connection, source: &str, id: &str, at: DateTime<Utc>) {
    let e = Event::minimal(source, id, at.to_rfc3339(), "t");
    repo::upsert_event(conn, &e).unwrap();
}

fn row<'a>(rows: &'a [SourceHealth], source: &str) -> &'a SourceHealth {
    rows.iter().find(|r| r.source == source).unwrap()
}

#[test]
fn empty_db_lists_no_sources() {
    let conn = open_memory().unwrap();
    assert!(all(&conn, now()).unwrap().is_empty());
}

#[test]
fn ok_run_leaves_last_ok_and_no_error() {
    let conn = open_memory().unwrap();
    record(&conn, "github", Ok(())).unwrap();
    assert!(meta_get(&conn, &collect_ok_key("github"))
        .unwrap()
        .is_some());
    let rows = all(&conn, now()).unwrap();
    assert!(row(&rows, "github").last_ok.is_some());
    assert_eq!(row(&rows, "github").last_err, None); // catches writing both keys
}

#[test]
fn err_run_leaves_time_and_text_and_no_ok() {
    let conn = open_memory().unwrap();
    record(&conn, "jira", Err("HTTP 401")).unwrap();
    let rows = all(&conn, now()).unwrap();
    let r = row(&rows, "jira");
    assert_eq!(r.last_ok, None);
    let err = r.last_err.as_deref().unwrap();
    assert!(err.ends_with(" HTTP 401"), "{err}"); // catches dropping the message
    let stamp = err.split(' ').next().unwrap();
    assert!(DateTime::parse_from_rfc3339(stamp).is_ok(), "{err}"); // catches a missing time
}

#[test]
fn later_run_overwrites_earlier_error() {
    let conn = open_memory().unwrap();
    record(&conn, "jira", Err("first")).unwrap();
    record(&conn, "jira", Err("second")).unwrap();
    let rows = all(&conn, now()).unwrap();
    assert!(row(&rows, "jira")
        .last_err
        .as_deref()
        .unwrap()
        .ends_with(" second"));
}

#[test]
fn prefix_sibling_sources_stay_separate() {
    let conn = open_memory().unwrap();
    record(&conn, "git", Ok(())).unwrap();
    record(&conn, "github", Err("boom")).unwrap();
    let rows = all(&conn, now()).unwrap();
    assert_eq!(rows.len(), 2); // catches a prefix match merging git and github
    assert_eq!(row(&rows, "git").last_err, None);
    assert_eq!(row(&rows, "github").last_ok, None);
}

#[test]
fn events_24h_includes_exactly_24h_and_excludes_just_past() {
    let conn = open_memory().unwrap();
    record(&conn, "github", Ok(())).unwrap();
    seed(&conn, "github", "at", now() - Duration::hours(24)); // catches > for >=
    seed(
        &conn,
        "github",
        "past",
        now() - Duration::hours(24) - Duration::seconds(1),
    ); // catches no cutoff
    seed(&conn, "github", "recent", now() - Duration::hours(1));
    seed(&conn, "jira", "other", now() - Duration::hours(1)); // catches counting all sources
    let rows = all(&conn, now()).unwrap();
    assert_eq!(row(&rows, "github").events_24h, 2);
}

#[test]
fn source_with_events_but_no_run_is_still_listed() {
    let conn = open_memory().unwrap();
    seed(&conn, "jira", "a", now() - Duration::hours(2));
    let rows = all(&conn, now()).unwrap();
    let r = row(&rows, "jira");
    assert_eq!(
        (r.last_ok.clone(), r.last_err.clone(), r.events_24h),
        (None, None, 1)
    );
}

#[test]
fn quiet_source_reports_zero_events() {
    let conn = open_memory().unwrap();
    record(&conn, "gcal", Ok(())).unwrap();
    seed(&conn, "gcal", "old", now() - Duration::days(3));
    assert_eq!(row(&all(&conn, now()).unwrap(), "gcal").events_24h, 0);
}
