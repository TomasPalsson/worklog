use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use crate::routing::label_event;
use crate::routing_contract::{LabelRequest, IGNORE_FOLDER};

fn slack_event(conn: &Connection, source_id: &str, started_at: &str, channel: &str) -> i64 {
    repo::upsert_event(
        conn,
        &Event::minimal(SOURCE_SLACK, source_id, started_at, channel),
    )
    .unwrap()
}

#[test]
fn dismiss_sets_origin_and_clears_folder_and_confidence() {
    let conn = open_memory().unwrap();
    let id = slack_event(&conn, "s1", "2026-04-20T09:00:00+00:00", "#random");

    let routed = dismiss_event(&conn, id, None).unwrap();
    assert_eq!(routed.id, id);
    assert_eq!(routed.folder, None);
    assert_eq!(routed.label_origin, Some(LabelOrigin::Dismissed));
    assert_eq!(routed.label_confidence, None);
}

#[test]
fn dismiss_unknown_event_errors() {
    let conn = open_memory().unwrap();
    let err = dismiss_event(&conn, 999_999, None).unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn dismiss_rejects_container_rule_kind() {
    let conn = open_memory().unwrap();
    let id = slack_event(&conn, "s1", "2026-04-20T09:00:00+00:00", "#random");
    let err = dismiss_event(&conn, id, Some(RuleKind::Container)).unwrap_err();
    assert!(err.to_string().contains("domain or slack_channel"));

    // Rejected before any write — the event must be untouched.
    let row = fetch_event(&conn, id).unwrap().unwrap();
    assert!(row.label_origin.is_none());
}

#[test]
fn dismiss_rejects_domain_rule_on_slack_event() {
    let conn = open_memory().unwrap();
    let id = slack_event(&conn, "s1", "2026-04-20T09:00:00+00:00", "#random");
    let err = dismiss_event(&conn, id, Some(RuleKind::Domain)).unwrap_err();
    assert!(err.to_string().contains("firefox"));
}

#[test]
fn dismiss_with_rule_kind_creates_ignore_rule() {
    let conn = open_memory().unwrap();
    let id = slack_event(&conn, "s1", "2026-04-20T09:00:00+00:00", "#random");

    dismiss_event(&conn, id, Some(RuleKind::SlackChannel)).unwrap();

    let (pattern, folder): (String, String) = conn
        .query_row(
            "SELECT pattern, folder FROM routing_rules WHERE kind = 'slack_channel'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pattern, "#random");
    assert_eq!(folder, IGNORE_FOLDER);
}

#[test]
fn dismiss_retroactively_dismisses_same_channel_events_but_skips_fix() {
    let conn = open_memory().unwrap();
    crate::billing_registry::upsert_folder(
        &conn,
        &crate::billing_registry::FolderMap {
            id: None,
            folder: "kept".into(),
            customer: None,
            verkefni: None,
            billable: true,
        },
    )
    .unwrap();

    let target = slack_event(&conn, "s1", "2026-04-20T09:00:00+00:00", "#random");
    let sibling = slack_event(&conn, "s2", "2026-04-20T09:05:00+00:00", "#random");
    let fixed = slack_event(&conn, "s3", "2026-04-20T09:10:00+00:00", "#random");
    label_event(
        &conn,
        fixed,
        &LabelRequest {
            folder: "kept".into(),
            always: None,
        },
    )
    .unwrap();

    dismiss_event(&conn, target, Some(RuleKind::SlackChannel)).unwrap();

    let sibling_row = fetch_event(&conn, sibling).unwrap().unwrap();
    assert_eq!(sibling_row.label_origin.as_deref(), Some("dismissed"));
    assert!(sibling_row.project_path.is_none());

    let fixed_row = fetch_event(&conn, fixed).unwrap().unwrap();
    assert_eq!(
        fixed_row.label_origin.as_deref(),
        Some("fix"),
        "a hand-fixed event must never be retroactively dismissed"
    );
    assert!(fixed_row.project_path.is_some());
}
