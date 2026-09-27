//! Tests for session customer tagging (`session_customers`).

use super::*;
use crate::billing_registry::Customer;
use chrono::{TimeZone, Utc};

fn ev(session: &str, project: &str, title: Option<&str>, jira: Option<&str>) -> InferEvent {
    InferEvent {
        ts: Utc.with_ymd_and_hms(2026, 9, 23, 9, 0, 0).unwrap(),
        source: "claude_turn".into(),
        duration_seconds: None,
        jira_issue: jira.map(str::to_string),
        event_id: None,
        project_path: Some(project.to_string()),
        session_id: Some(session.to_string()),
        title: title.map(str::to_string),
        lane_tag: None,
    }
}

fn registry(customers: &[&str]) -> Registry {
    Registry {
        customers: customers
            .iter()
            .map(|n| Customer {
                id: None,
                name: (*n).to_string(),
                aliases: vec![],
            })
            .collect(),
        folders: vec![],
    }
}

const FOLDER: &str = "/Users/dev/Desktop/Work/vitinn-infra";

#[test]
fn two_customers_in_one_repo_tag_both() {
    let reg = registry(&["Acme", "Globex"]);
    let mut events = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("A", FOLDER, Some("more Acme work"), None),
        ev("B", FOLDER, Some("Globex migration"), None),
        ev("B", FOLDER, Some("Globex followup"), None),
    ];
    tag_sessions(&mut events, &reg);
    assert!(events[0..2]
        .iter()
        .all(|e| e.lane_tag.as_deref() == Some("Acme")));
    assert!(events[2..4]
        .iter()
        .all(|e| e.lane_tag.as_deref() == Some("Globex")));
    assert!(events
        .iter()
        .all(|e| e.project_path.as_deref() == Some(FOLDER)));
}

#[test]
fn one_customer_tags_nothing() {
    let reg = registry(&["Acme", "Globex"]);

    // Both sessions name the same customer.
    let mut same = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER, Some("Acme followup"), None),
    ];
    tag_sessions(&mut same, &reg);
    assert!(same.iter().all(|e| e.lane_tag.is_none()));

    // One session resolves, the other names nothing.
    let mut mixed = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER, Some("routine maintenance"), None),
    ];
    tag_sessions(&mut mixed, &reg);
    assert!(mixed.iter().all(|e| e.lane_tag.is_none()));
}

#[test]
fn unresolved_session_stays_untagged() {
    let reg = registry(&["Acme", "Globex"]);
    let mut events = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER, Some("Globex migration"), None),
        ev("C", FOLDER, Some("Acme and Globex sync"), None), // names both -> ambiguous
    ];
    tag_sessions(&mut events, &reg);
    assert_eq!(events[0].lane_tag.as_deref(), Some("Acme"));
    assert_eq!(events[1].lane_tag.as_deref(), Some("Globex"));
    assert_eq!(events[2].lane_tag, None);
}

#[test]
fn session_less_event_stays_untagged() {
    let reg = registry(&["Acme", "Globex"]);
    let mut events = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER, Some("Globex migration"), None),
    ];
    let ts = events[0].ts;
    events.push(InferEvent {
        ts,
        source: "shell".into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: Some(FOLDER.to_string()),
        session_id: None,
        title: None,
        lane_tag: None,
    });
    tag_sessions(&mut events, &reg);
    assert_eq!(
        events[2].lane_tag, None,
        "a session-less event stays untagged"
    );
    assert!(
        events
            .iter()
            .all(|e| e.project_path.as_deref() == Some(FOLDER)),
        "project_path must be untouched by tagging"
    );
}

#[test]
fn two_folders_one_customer_each_tags_nothing() {
    const FOLDER_B: &str = "/Users/dev/Desktop/Work/otherproj";
    let reg = registry(&["Acme", "Globex"]);
    let mut events = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER_B, Some("Globex migration"), None),
    ];
    tag_sessions(&mut events, &reg);
    assert!(events.iter().all(|e| e.lane_tag.is_none()));
}
