//! Tests for T008 — AI ticket update draft (spec 012, B7, FR-08).

use super::*;
use crate::db;
use crate::estimate::FixedInvoker;
use crate::tempo_hub_contract::StatusCategory;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn ticket(conn: &Connection, key: &str, summary: &str, status: Option<&str>) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, fetched_at)
         VALUES (?1, ?2, ?3, '2026-09-30T08:00:00Z')",
        params![key, summary, status],
    )
    .unwrap();
}

fn block(conn: &Connection, issue: &str, day: &str, description: &str) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?3, ?4, 3600, ?5)",
        params![
            day,
            issue,
            format!("{day}T09:00:00+00:00"),
            format!("{day}T10:00:00+00:00"),
            description
        ],
    )
    .unwrap();
}

fn stored_text(conn: &Connection, issue: &str, day: &str, text: &str) {
    crate::tempo_lines::set_text(
        conn,
        &crate::tempo_line_contract::SetTempoLineText {
            day: day.to_string(),
            jira_issue: issue.to_string(),
            text: text.to_string(),
        },
    )
    .unwrap()
    .unwrap();
}

fn transition(id: &str, name: &str) -> Transition {
    Transition {
        id: id.to_string(),
        name: name.to_string(),
        to_status: format!("{name} status"),
        to_category: Some(StatusCategory::Done),
    }
}

fn prep() -> DraftPrep {
    DraftPrep {
        key: "APRO-1".to_string(),
        summary: "Fix login".to_string(),
        status: Some("In Progress".to_string()),
        recent_lines: vec![("2026-09-30".to_string(), "Fixed the bug".to_string())],
    }
}

fn transitions() -> Vec<Transition> {
    vec![transition("11", "Start"), transition("31", "Done")]
}

#[test]
fn prepare_prefers_stored_text_then_fallback_newest_first() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", Some("In Progress"));
    block(&conn, "APRO-1", "2026-09-29", "Alpha");
    block(&conn, "APRO-1", "2026-09-30", "Beta");
    stored_text(&conn, "APRO-1", "2026-09-30", "Hand written");
    block(&conn, "APRO-2", "2026-09-30", "Other ticket");

    let prep = prepare_draft(&conn, "APRO-1", date("2026-09-30")).unwrap();
    assert_eq!(prep.key, "APRO-1");
    assert_eq!(prep.summary, "Fix login");
    assert_eq!(prep.status.as_deref(), Some("In Progress"));
    assert_eq!(
        prep.recent_lines,
        vec![
            ("2026-09-30".to_string(), "Hand written".to_string()),
            ("2026-09-29".to_string(), "Alpha".to_string()),
        ]
    );
}

#[test]
fn prepare_keeps_only_the_last_fourteen_days() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", None);
    block(&conn, "APRO-1", "2026-09-17", "Too old");
    block(&conn, "APRO-1", "2026-09-18", "Oldest kept");

    let prep = prepare_draft(&conn, "APRO-1", date("2026-10-01")).unwrap();
    let days: Vec<&str> = prep.recent_lines.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(days, vec!["2026-09-18"]);
}

#[test]
fn prepare_yields_one_line_per_day_within_the_cap() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", None);
    for offset in 1..=14 {
        block(
            &conn,
            "APRO-1",
            &format!("2026-09-{:02}", 14 + offset),
            "Work",
        );
    }
    let prep = prepare_draft(&conn, "APRO-1", date("2026-09-28")).unwrap();
    assert!(prep.recent_lines.len() <= 20);
    assert_eq!(prep.recent_lines.len(), 14);
}

#[test]
fn prepare_unknown_ticket_is_an_error() {
    let conn = db::open_memory().unwrap();
    assert!(prepare_draft(&conn, "NOPE-1", date("2026-09-30")).is_err());
}

#[test]
fn prompt_holds_exactly_the_allowed_fields() {
    let prompt = build_prompt(&prep(), &transitions());
    let value: Value = serde_json::from_str(&prompt).unwrap();
    let fields: BTreeSet<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        fields,
        BTreeSet::from(["key", "recent_lines", "status", "summary", "transitions"])
    );
    assert_eq!(value["key"], "APRO-1");
    assert_eq!(value["summary"], "Fix login");
    assert_eq!(value["status"], "In Progress");
    assert_eq!(value["transitions"], json!(["Start", "Done"]));
    assert_eq!(
        value["recent_lines"],
        json!([{"day": "2026-09-30", "text": "Fixed the bug"}])
    );
    assert!(!prompt.contains("Done status"));
    assert!(!prompt.contains("\"31\""));
}

#[test]
fn draft_returns_comment_and_known_transition() {
    let invoker = FixedInvoker(json!({"comment": "Fixed it.", "suggested_transition_id": "31"}));
    let draft = draft_with(&invoker, &prep(), &transitions(), "m").unwrap();
    assert_eq!(draft.comment, "Fixed it.");
    assert_eq!(draft.suggested_transition_id.as_deref(), Some("31"));
    assert_eq!(draft.transitions, transitions());
}

#[test]
fn draft_resolves_a_transition_name_to_its_id() {
    let invoker = FixedInvoker(json!({"comment": "Fixed it.", "suggested_transition_id": "Done"}));
    let draft = draft_with(&invoker, &prep(), &transitions(), "m").unwrap();
    assert_eq!(draft.suggested_transition_id.as_deref(), Some("31"));
}

#[test]
fn draft_drops_a_foreign_transition_id() {
    let invoker = FixedInvoker(json!({"comment": "Fixed it.", "suggested_transition_id": "999"}));
    let draft = draft_with(&invoker, &prep(), &transitions(), "m").unwrap();
    assert_eq!(draft.comment, "Fixed it.");
    assert_eq!(draft.suggested_transition_id, None);
}

#[test]
fn draft_accepts_null_transition() {
    let invoker = FixedInvoker(json!({"comment": "Fixed it.", "suggested_transition_id": null}));
    let draft = draft_with(&invoker, &prep(), &transitions(), "m").unwrap();
    assert_eq!(draft.suggested_transition_id, None);
}

#[test]
fn draft_rejects_empty_or_missing_comment() {
    for reply in [
        json!({"comment": "   ", "suggested_transition_id": "31"}),
        json!({"suggested_transition_id": "31"}),
    ] {
        let err = draft_with(&FixedInvoker(reply), &prep(), &transitions(), "m").unwrap_err();
        assert!(err.to_string().contains("comment"), "{err}");
    }
}

#[test]
fn draft_truncates_an_overlong_comment_to_the_limit() {
    let long = "é".repeat(COMMENT_MAX_CHARS + 10);
    let invoker = FixedInvoker(json!({"comment": long, "suggested_transition_id": null}));
    let draft = draft_with(&invoker, &prep(), &transitions(), "m").unwrap();
    assert_eq!(draft.comment.chars().count(), COMMENT_MAX_CHARS);
}

#[test]
fn draft_propagates_invoker_failure() {
    struct Failing;
    impl ModelInvoker for Failing {
        fn invoke(&self, _: &str, _: &str, _: &Value, _: &str) -> Result<Value> {
            Err(anyhow::anyhow!("model down"))
        }
    }
    let err = draft_with(&Failing, &prep(), &transitions(), "m").unwrap_err();
    assert!(err.to_string().contains("model down"), "{err:#}");
}
