use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use crate::routing::{fetch_event, label_event};
use crate::routing_contract::{LabelRequest, RuleKind, SOURCE_FIREFOX, SOURCE_SLACK};

fn pin(conn: &Connection, folder: &str) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: None,
            verkefni: None,
            billable: true,
        },
    )
    .unwrap();
}

/// A `claude` work-activity event at `ts`, with `project_path` under the
/// real `$HOME/Desktop/Work/<key>` so `routing_context::context_key` matches it.
fn work_at(conn: &Connection, id: &str, ts: &str, key: &str) {
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let mut ev = Event::minimal("claude", id, ts, "x");
    ev.project_path = Some(format!("{home}/Desktop/Work/{key}"));
    repo::upsert_event(conn, &ev).unwrap();
}

fn firefox(conn: &Connection, id: &str, ts: &str) -> i64 {
    repo::upsert_event(conn, &Event::minimal(SOURCE_FIREFOX, id, ts, "page")).unwrap()
}

fn slack(conn: &Connection, id: &str, ts: &str) -> i64 {
    repo::upsert_event(conn, &Event::minimal(SOURCE_SLACK, id, ts, "#chan")).unwrap()
}

#[test]
fn absorbs_inside_a_work_stretch_with_the_dominant_key() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    work_at(&conn, "c1", "2026-04-20T08:57:00+00:00", "sjukra");
    work_at(&conn, "c2", "2026-04-20T09:03:00+00:00", "sjukra");
    let eid = slack(&conn, "s1", "2026-04-20T09:00:00+00:00");

    absorb_and_noise(&conn, day).unwrap();

    let row = fetch_event(&conn, eid).unwrap().unwrap();
    assert_eq!(row.label_origin.as_deref(), Some("context"));
    assert!(row.label_confidence.is_none());
    assert!(row.project_path.unwrap().ends_with("/sjukra"));
}

#[test]
fn edge_activity_on_one_side_only_is_not_absorbed() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    // Only activity BEFORE the event, none after — it sits at the edge of
    // the stretch, not inside it.
    work_at(&conn, "c1", "2026-04-20T08:56:00+00:00", "sjukra");
    let eid = slack(&conn, "s1", "2026-04-20T09:00:00+00:00");

    absorb_and_noise(&conn, day).unwrap();

    let row = fetch_event(&conn, eid).unwrap().unwrap();
    assert_eq!(row.label_origin.as_deref(), Some("noise"));
    assert!(row.project_path.is_none());
}

#[test]
fn a_tie_between_two_keys_is_noise() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    pin(&conn, "mms");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    work_at(&conn, "c1", "2026-04-20T08:58:00+00:00", "sjukra");
    work_at(&conn, "c2", "2026-04-20T09:02:00+00:00", "mms");
    let eid = firefox(&conn, "f1", "2026-04-20T09:00:00+00:00");

    absorb_and_noise(&conn, day).unwrap();

    let row = fetch_event(&conn, eid).unwrap().unwrap();
    assert_eq!(row.label_origin.as_deref(), Some("noise"));
}

#[test]
fn firefox_is_absorbed_only_inside_a_stretch() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    work_at(&conn, "c1", "2026-04-20T08:57:00+00:00", "sjukra");
    work_at(&conn, "c2", "2026-04-20T09:03:00+00:00", "sjukra");
    let inside = firefox(&conn, "f1", "2026-04-20T09:00:00+00:00");
    // Ten minutes clear of any activity — outside every stretch.
    let outside = firefox(&conn, "f2", "2026-04-20T09:20:00+00:00");

    absorb_and_noise(&conn, day).unwrap();

    let inside_row = fetch_event(&conn, inside).unwrap().unwrap();
    assert_eq!(inside_row.label_origin.as_deref(), Some("context"));
    let outside_row = fetch_event(&conn, outside).unwrap().unwrap();
    assert_eq!(outside_row.label_origin.as_deref(), Some("noise"));
}

#[test]
fn label_event_relabels_a_noise_event() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let eid = firefox(&conn, "f1", "2026-04-20T09:00:00+00:00");
    absorb_and_noise(&conn, day).unwrap();
    assert_eq!(
        fetch_event(&conn, eid)
            .unwrap()
            .unwrap()
            .label_origin
            .as_deref(),
        Some("noise")
    );

    let routed = label_event(
        &conn,
        eid,
        &LabelRequest {
            folder: "sjukra".into(),
            always: None,
        },
    )
    .unwrap();
    assert_eq!(routed.folder.as_deref(), Some("sjukra"));
    assert_eq!(
        routed.label_origin,
        Some(crate::routing_contract::LabelOrigin::Fix)
    );
}

#[test]
fn always_rule_retroactively_relabels_noise() {
    let conn = open_memory().unwrap();
    pin(&conn, "sjukra");
    let day = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
    let target = slack(&conn, "s1", "2026-04-20T09:00:00+00:00");
    let sibling = slack(&conn, "s2", "2026-04-20T09:05:00+00:00");
    absorb_and_noise(&conn, day).unwrap();
    assert_eq!(
        fetch_event(&conn, sibling)
            .unwrap()
            .unwrap()
            .label_origin
            .as_deref(),
        Some("noise")
    );

    label_event(
        &conn,
        target,
        &LabelRequest {
            folder: "sjukra".into(),
            always: Some(RuleKind::SlackChannel),
        },
    )
    .unwrap();

    let sibling_row = fetch_event(&conn, sibling).unwrap().unwrap();
    assert_eq!(
        sibling_row.label_origin.as_deref(),
        Some("rule"),
        "an always rule must retroactively reclaim a noise event"
    );
    assert!(sibling_row.project_path.unwrap().ends_with("/sjukra"));
}
