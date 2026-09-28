//! Tests for session customer tagging (`session_customers`).

use super::*;
use crate::billing_registry::Customer;
use crate::session_pins::SessionPin;
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
    tag_sessions(&mut events, &reg, &[]);
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
    tag_sessions(&mut same, &reg, &[]);
    assert!(same.iter().all(|e| e.lane_tag.is_none()));

    // One session resolves, the other names nothing.
    let mut mixed = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER, Some("routine maintenance"), None),
    ];
    tag_sessions(&mut mixed, &reg, &[]);
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
    tag_sessions(&mut events, &reg, &[]);
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
    tag_sessions(&mut events, &reg, &[]);
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

fn pin(session: &str, customer: &str, from_at: chrono::DateTime<Utc>) -> SessionPin {
    SessionPin {
        session_id: session.to_string(),
        customer: customer.to_string(),
        from_at,
        folder: FOLDER.to_string(),
        branch: None,
        source: "claude".to_string(),
    }
}

fn ev_at(session: &str, ts: chrono::DateTime<Utc>) -> InferEvent {
    InferEvent {
        ts,
        source: "claude_turn".into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: Some(FOLDER.to_string()),
        session_id: Some(session.to_string()),
        title: Some("routine maintenance".into()),
        lane_tag: None,
    }
}

#[test]
fn pin_beats_text_guess() {
    let reg = registry(&["Acme", "Globex"]);
    let switch_at = Utc.with_ymd_and_hms(2026, 9, 23, 9, 10, 0).unwrap();
    let mut events = vec![
        // Session A's own text says Acme, but it is pinned to Globex from
        // the start — the pin must win.
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("A", FOLDER, Some("more Acme work"), None),
        // Session B is never pinned; its text guess (Acme) stands.
        ev("B", FOLDER, Some("Acme followup"), None),
        // Session C switches mid-session: before `switch_at` it falls back
        // to its own text guess (Globex); from `switch_at` on the pin
        // (Acme) applies.
        ev_at("C", switch_at - chrono::Duration::minutes(5)),
        ev_at("C", switch_at),
        ev_at("C", switch_at + chrono::Duration::minutes(5)),
    ];
    events[3].title = Some("Globex prep".into());
    events[4].title = Some("Globex prep".into());
    events[5].title = Some("Globex prep".into());

    let pins = vec![
        pin(
            "A",
            "Globex",
            Utc.with_ymd_and_hms(2026, 9, 23, 0, 0, 0).unwrap(),
        ),
        pin("C", "Acme", switch_at),
    ];

    tag_sessions(&mut events, &reg, &pins);

    assert!(
        events[0..2]
            .iter()
            .all(|e| e.lane_tag.as_deref() == Some("Globex")),
        "the pin must beat session A's own text guess of Acme"
    );
    assert_eq!(events[2].lane_tag.as_deref(), Some("Acme"));
    assert_eq!(
        events[3].lane_tag.as_deref(),
        Some("Globex"),
        "before the pin's from_at, the text guess fills in"
    );
    assert_eq!(events[4].lane_tag.as_deref(), Some("Acme"));
    assert_eq!(events[5].lane_tag.as_deref(), Some("Acme"));
}

#[test]
fn two_folders_one_customer_each_tags_nothing() {
    const FOLDER_B: &str = "/Users/dev/Desktop/Work/otherproj";
    let reg = registry(&["Acme", "Globex"]);
    let mut events = vec![
        ev("A", FOLDER, Some("Acme onboarding"), None),
        ev("B", FOLDER_B, Some("Globex migration"), None),
    ];
    tag_sessions(&mut events, &reg, &[]);
    assert!(events.iter().all(|e| e.lane_tag.is_none()));
}

fn untitled_ev_at(session: &str, ts: chrono::DateTime<Utc>) -> InferEvent {
    InferEvent {
        ts,
        source: "claude_turn".into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: Some(FOLDER.to_string()),
        session_id: Some(session.to_string()),
        title: None,
        lane_tag: None,
    }
}

fn titled_ev_at(session: &str, ts: chrono::DateTime<Utc>, title: &str) -> InferEvent {
    InferEvent {
        title: Some(title.to_string()),
        ..untitled_ev_at(session, ts)
    }
}

// Finding A: a real session's SessionStart hook row and first prompt land
// before Claude gets around to running `worklog pin` — the setup-race
// reach-back (session_pins::SETUP_GRACE) must still resolve them to the
// pin once it lands soon enough, but must not reach back across a much
// later, genuine mid-session re-pin.
#[test]
fn setup_minutes_take_the_first_pin() {
    let reg = registry(&["Acme", "Globex"]);
    let t0 = Utc.with_ymd_and_hms(2026, 9, 23, 9, 0, 0).unwrap();
    let t0c = Utc.with_ymd_and_hms(2026, 9, 23, 11, 0, 0).unwrap();

    let mut events = vec![
        // Session A: no text guess of its own (SessionStart/first prompt
        // never name a customer) — the pin, 60s later, must reach back.
        untitled_ev_at("A", t0),
        untitled_ev_at("A", t0 + chrono::Duration::seconds(30)),
        untitled_ev_at("A", t0 + chrono::Duration::seconds(120)),
        // Session B: names Acme in its own text — the second customer
        // this folder needs before lane tagging kicks in at all.
        ev("B", FOLDER, Some("Acme onboarding"), None),
        // Session C: names Acme early on; its pin (Globex) lands 30
        // minutes after the session's first event — too far for the
        // setup-race reach-back — so the early events keep their guess.
        titled_ev_at("C", t0c, "Acme prep"),
        titled_ev_at("C", t0c + chrono::Duration::minutes(5), "Acme prep"),
    ];

    let pins = vec![
        pin("A", "Globex", t0 + chrono::Duration::seconds(60)),
        pin("C", "Globex", t0c + chrono::Duration::minutes(30)),
    ];

    tag_sessions(&mut events, &reg, &pins);

    assert!(
        events[0..3]
            .iter()
            .all(|e| e.lane_tag.as_deref() == Some("Globex")),
        "setup-minute events with no text guess of their own take the pin \
         once it lands within SETUP_GRACE of the session's first event"
    );
    assert_eq!(events[3].lane_tag.as_deref(), Some("Acme"));
    assert!(
        events[4..6]
            .iter()
            .all(|e| e.lane_tag.as_deref() == Some("Acme")),
        "a pin 30 minutes after the session's first event is too far for \
         the setup-race reach-back — early events keep the text guess"
    );
}
