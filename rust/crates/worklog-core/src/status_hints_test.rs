//! Tests for T010 — done hints (spec 013, FR-16, B3, B11).

use super::*;
use crate::db;
use crate::jira_assist_contract::HintReason;
use crate::models::Event;
use crate::repo;
use rusqlite::params;

fn ticket(conn: &Connection, key: &str, category: Option<&str>) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at, updated)
         VALUES (?1, ?2, 'In Progress', ?3, 0, '2026-09-30T08:00:00Z', '2026-09-29T10:00:00.000+0000')",
        params![key, format!("summary {key}"), category],
    )
    .unwrap();
}

fn pr(conn: &Connection, id: &str, number: i64, key: &str, merged_at: Option<&str>) {
    let raw = serde_json::json!({
        "kind": "commit", "sha": "", "body": "", "local_folder": null, "merged_at": merged_at,
    });
    repo::upsert_event(
        conn,
        &Event {
            id: None,
            source: "github_pr".into(),
            source_id: id.into(),
            started_at: "2026-10-01T09:00:00Z".into(),
            ended_at: None,
            duration_seconds: None,
            title: format!("PR #{number}: {key} change"),
            details: None,
            repo: Some("acme/app".into()),
            project_path: None,
            jira_issue: Some(key.into()),
            session_id: None,
            tempo_worklog_id: None,
            raw_json: Some(raw.to_string()),
        },
    )
    .unwrap();
}

#[test]
fn hints_genai_ticket_with_merged_pr() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "GENAI-7", Some("indeterminate"));
    pr(&conn, "1", 42, "GENAI-7", Some("2026-10-02T11:00:00Z"));
    let hints = done_hints(&conn).unwrap();
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].key, "GENAI-7");
    assert_eq!(hints[0].summary, "summary GENAI-7");
    assert_eq!(hints[0].to_category, StatusCategory::Done);
    assert_eq!(
        hints[0].reason,
        HintReason::PrMerged {
            repo: "acme/app".into(),
            number: 42,
            merged_at: "2026-10-02T11:00:00Z".into()
        }
    );
}

#[test]
fn skips_unmerged_done_and_non_genai() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "GENAI-1", Some("indeterminate"));
    pr(&conn, "1", 1, "GENAI-1", None);
    ticket(&conn, "GENAI-2", Some("done"));
    pr(&conn, "2", 2, "GENAI-2", Some("2026-10-02T11:00:00Z"));
    ticket(&conn, "APRO-3", Some("indeterminate"));
    pr(&conn, "3", 3, "APRO-3", Some("2026-10-02T11:00:00Z"));
    assert!(done_hints(&conn).unwrap().is_empty());
}

#[test]
fn unknown_category_still_hints_and_latest_merge_wins() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "GENAI-4", None);
    pr(&conn, "1", 10, "GENAI-4", Some("2026-10-01T11:00:00Z"));
    pr(&conn, "2", 11, "GENAI-4", Some("2026-10-03T11:00:00Z"));
    let hints = done_hints(&conn).unwrap();
    assert_eq!(hints.len(), 1);
    assert!(matches!(
        &hints[0].reason,
        HintReason::PrMerged { number: 11, .. }
    ));
}
