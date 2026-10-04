//! Tests for T006 — task board query (spec 012, B1, B2, B3).

use super::*;
use crate::db;
use rusqlite::params;

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn ticket(conn: &Connection, key: &str, summary: &str, category: Option<&str>, external: i64) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at, updated)
         VALUES (?1, ?2, 'In Progress', ?3, ?4, ?5, '2026-09-29T10:00:00.000+0000')",
        params![
            key,
            summary,
            category,
            external,
            format!("2026-09-30T08:0{external}:00Z")
        ],
    )
    .unwrap();
}

/// A one-hour block for `issue` on `day`.
fn worked(conn: &Connection, issue: &str, day: &str) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, ?3, ?4, 3600)",
        params![
            day,
            issue,
            format!("{day}T09:00:00+00:00"),
            format!("{day}T10:00:00+00:00")
        ],
    )
    .unwrap();
}

fn board(conn: &Connection) -> TasksResponse {
    tasks(
        conn,
        date("2026-09-28"),
        date("2026-09-30"),
        Some("https://x.atlassian.net"),
    )
    .unwrap()
}

fn keys(response: &TasksResponse) -> Vec<&str> {
    response.tasks.iter().map(|t| t.key.as_str()).collect()
}

#[test]
fn assigned_open_ticket_without_lines_has_zero_hours() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", Some("indeterminate"), 0);
    let response = board(&conn);
    assert_eq!(response.tasks.len(), 1);
    let row = &response.tasks[0];
    assert_eq!(row.summary, "Fix login");
    assert_eq!(row.status.as_deref(), Some("In Progress"));
    assert_eq!(row.status_category, Some(StatusCategory::Indeterminate));
    assert_eq!(
        row.url.as_deref(),
        Some("https://x.atlassian.net/browse/APRO-1")
    );
    assert!(row.assigned);
    assert_eq!((row.week_seconds, row.today_seconds), (0, 0));
    assert_eq!(row.last_worked_day, None);
}

#[test]
fn done_and_external_tickets_without_lines_are_hidden_but_null_category_stays() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Closed", Some("done"), 0);
    ticket(&conn, "APRO-2", "Picked", Some("new"), 1);
    ticket(&conn, "APRO-3", "Legacy row", None, 0);
    assert_eq!(keys(&board(&conn)), vec!["APRO-3"]);
}

#[test]
fn worked_ticket_outside_the_assigned_set_is_listed_unassigned() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Closed", Some("done"), 0);
    ticket(&conn, "APRO-2", "Picked", Some("new"), 1);
    worked(&conn, "APRO-1", "2026-09-29");
    worked(&conn, "APRO-2", "2026-09-30");
    worked(&conn, "APRO-9", "2026-09-30");
    let response = board(&conn);
    assert!(response.tasks.iter().all(|t| !t.assigned));
    assert_eq!(keys(&response), vec!["APRO-1", "APRO-2", "APRO-9"]);
}

#[test]
fn uncached_ticket_summary_falls_back_to_its_key() {
    let conn = db::open_memory().unwrap();
    worked(&conn, "APRO-9", "2026-09-30");
    let row = &board(&conn).tasks[0];
    assert_eq!(row.summary, "APRO-9");
    assert_eq!(row.status, None);
    assert_eq!(row.status_category, None);
}

#[test]
fn week_is_monday_to_sunday_and_today_is_one_day() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "A", Some("new"), 0);
    for day in [
        "2026-09-27",
        "2026-09-28",
        "2026-09-30",
        "2026-10-04",
        "2026-10-05",
    ] {
        worked(&conn, "APRO-1", day);
    }
    let row = &board(&conn).tasks[0];
    assert_eq!(row.week_seconds, 3 * 3600);
    assert_eq!(row.today_seconds, 3600);
    assert_eq!(row.last_worked_day.as_deref(), Some("2026-10-05"));
}

#[test]
fn hours_override_counts_instead_of_union() {
    let conn = db::open_memory().unwrap();
    worked(&conn, "APRO-1", "2026-09-30");
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, hours_override_seconds, updated_at)
         VALUES ('2026-09-30', 'APRO-1', 7200, 'now')",
        [],
    )
    .unwrap();
    assert_eq!(board(&conn).tasks[0].week_seconds, 7200);
}

#[test]
fn personal_blocks_do_not_make_a_line() {
    let conn = db::open_memory().unwrap();
    worked(&conn, "APRO-1", "2026-09-30");
    conn.execute("UPDATE blocks SET is_personal = 1", [])
        .unwrap();
    assert!(board(&conn).tasks.is_empty());
}

#[test]
fn order_is_assigned_then_week_hours_then_key() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-5", "e", Some("new"), 0);
    ticket(&conn, "APRO-3", "c", Some("new"), 0);
    ticket(&conn, "APRO-4", "d", Some("new"), 0);
    worked(&conn, "APRO-4", "2026-09-29");
    worked(&conn, "APRO-4", "2026-09-30");
    worked(&conn, "APRO-3", "2026-09-30");
    worked(&conn, "APRO-1", "2026-09-30");
    worked(&conn, "APRO-1", "2026-09-29");
    worked(&conn, "APRO-1", "2026-09-28");
    worked(&conn, "APRO-2", "2026-09-30");
    assert_eq!(
        keys(&board(&conn)),
        vec!["APRO-4", "APRO-3", "APRO-5", "APRO-1", "APRO-2"]
    );
}

#[test]
fn url_is_none_without_a_base_url_and_trims_a_trailing_slash() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "A", Some("new"), 0);
    let monday = date("2026-09-28");
    let bare = tasks(&conn, monday, monday, None).unwrap();
    assert_eq!(bare.tasks[0].url, None);
    let slashed = tasks(&conn, monday, monday, Some("https://x.atlassian.net/")).unwrap();
    assert_eq!(
        slashed.tasks[0].url.as_deref(),
        Some("https://x.atlassian.net/browse/APRO-1")
    );
}

#[test]
fn last_fetched_is_the_newest_cache_time_and_none_when_empty() {
    let conn = db::open_memory().unwrap();
    assert_eq!(board(&conn).last_fetched, None);
    ticket(&conn, "APRO-1", "A", Some("new"), 0);
    ticket(&conn, "APRO-2", "B", Some("new"), 1);
    assert_eq!(
        board(&conn).last_fetched.as_deref(),
        Some("2026-09-30T08:01:00Z")
    );
}

#[test]
fn details_round_trip_and_uncached_defaults() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", Some("indeterminate"), 0);
    conn.execute(
        "UPDATE jira_tickets SET issue_type='Bug', priority='High', due_date='2026-10-04',
            labels='[\"a\",\"b\"]', parent_summary='Epic', updated='2026-09-29T10:00:00.000+0000'
          WHERE key='APRO-1'",
        [],
    )
    .unwrap();
    ticket(&conn, "APRO-2", "Bad labels", Some("new"), 0);
    conn.execute(
        "UPDATE jira_tickets SET labels='not json' WHERE key='APRO-2'",
        [],
    )
    .unwrap();
    worked(&conn, "NOCACHE-1", "2026-09-29");
    let response = board(&conn);
    let row = |k: &str| response.tasks.iter().find(|t| t.key == k).unwrap();
    let one = row("APRO-1");
    assert_eq!(one.issue_type.as_deref(), Some("Bug"));
    assert_eq!(one.priority.as_deref(), Some("High"));
    assert_eq!(one.due_date.as_deref(), Some("2026-10-04"));
    assert_eq!(one.labels, vec!["a", "b"]);
    assert_eq!(one.parent_summary.as_deref(), Some("Epic"));
    assert_eq!(one.updated.as_deref(), Some("2026-09-29T10:00:00.000+0000"));
    assert!(row("APRO-2").labels.is_empty());
    let bare = row("NOCACHE-1");
    assert_eq!(bare.issue_type, None);
    assert!(bare.labels.is_empty());
    assert_eq!(bare.updated, None);
    assert_eq!(bare.day_seconds.len(), 7);
}

#[test]
fn day_seconds_sums_each_day_monday_first() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Fix login", Some("indeterminate"), 0);
    worked(&conn, "APRO-1", "2026-09-28");
    worked(&conn, "APRO-1", "2026-09-30");
    let response = board(&conn);
    assert_eq!(
        response.tasks[0].day_seconds,
        vec![3600, 0, 3600, 0, 0, 0, 0]
    );
}

#[test]
fn backlog_tickets_are_hidden_even_when_worked_this_week() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Parked", Some("new"), 0);
    ticket(&conn, "APRO-2", "Parked, worked", Some("new"), 0);
    ticket(&conn, "APRO-3", "Still to do", Some("new"), 0);
    conn.execute(
        "UPDATE jira_tickets SET status = 'Backlog' WHERE key = 'APRO-1'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE jira_tickets SET status = 'BACKLOG' WHERE key = 'APRO-2'",
        [],
    )
    .unwrap();
    worked(&conn, "APRO-2", "2026-09-29");
    assert_eq!(keys(&board(&conn)), vec!["APRO-3"]);
}

fn set_ticket(conn: &Connection, key: &str, status: &str, updated: &str) {
    conn.execute(
        "UPDATE jira_tickets SET status = ?2, updated = ?3 WHERE key = ?1",
        params![key, status, updated],
    )
    .unwrap();
}

#[test]
fn cancelled_tickets_are_hidden_even_when_worked_this_week() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Cancelled, worked", Some("done"), 0);
    ticket(&conn, "APRO-2", "Cancelled, idle", Some("new"), 0);
    set_ticket(&conn, "APRO-1", "Cancel", "2026-09-29T10:00:00.000+0000");
    set_ticket(&conn, "APRO-2", "Cancelled", "2026-09-29T10:00:00.000+0000");
    worked(&conn, "APRO-1", "2026-09-29");
    assert!(keys(&board(&conn)).is_empty());
}

#[test]
fn stale_assigned_ticket_is_hidden_but_a_recent_one_stays() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Stale", Some("indeterminate"), 0);
    ticket(&conn, "APRO-2", "Fresh", Some("indeterminate"), 0);
    set_ticket(
        &conn,
        "APRO-1",
        "In Progress",
        "2026-08-16T10:00:00.000+0000",
    );
    set_ticket(
        &conn,
        "APRO-2",
        "In Progress",
        "2026-09-27T10:00:00.000+0000",
    );
    assert_eq!(keys(&board(&conn)), vec!["APRO-2"]);
}

#[test]
fn stale_ticket_worked_this_week_stays() {
    let conn = db::open_memory().unwrap();
    ticket(
        &conn,
        "APRO-1",
        "Stale but worked",
        Some("indeterminate"),
        0,
    );
    set_ticket(
        &conn,
        "APRO-1",
        "In Progress",
        "2026-08-16T10:00:00.000+0000",
    );
    worked(&conn, "APRO-1", "2026-09-29");
    assert_eq!(keys(&board(&conn)), vec!["APRO-1"]);
}

#[test]
fn done_ticket_worked_this_week_lands_in_done() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "APRO-1", "Finished", Some("done"), 0);
    set_ticket(
        &conn,
        "APRO-1",
        "Verification",
        "2026-09-29T10:00:00.000+0000",
    );
    worked(&conn, "APRO-1", "2026-09-29");
    let response = board(&conn);
    assert_eq!(keys(&response), vec!["APRO-1"]);
    assert_eq!(
        response.tasks[0].status_category,
        Some(StatusCategory::Done)
    );
}

#[test]
fn merged_pr_fills_done_hint_only_on_that_card() {
    use crate::jira_assist_contract::HintReason;
    use crate::models::Event;
    use crate::repo;
    let conn = db::open_memory().unwrap();
    ticket(&conn, "GENAI-7", "Ship it", Some("indeterminate"), 0);
    ticket(&conn, "GENAI-8", "Other", Some("indeterminate"), 0);
    worked(&conn, "GENAI-7", "2026-09-29");
    worked(&conn, "GENAI-8", "2026-09-29");
    let raw = serde_json::json!({
        "kind": "commit", "sha": "", "body": "", "local_folder": null,
        "merged_at": "2026-09-30T11:00:00Z",
    });
    repo::upsert_event(
        &conn,
        &Event {
            id: None,
            source: "github_pr".into(),
            source_id: "1".into(),
            started_at: "2026-09-29T09:00:00Z".into(),
            ended_at: None,
            duration_seconds: None,
            title: "PR #42: GENAI-7 change".into(),
            details: None,
            repo: Some("acme/app".into()),
            project_path: None,
            jira_issue: Some("GENAI-7".into()),
            session_id: None,
            tempo_worklog_id: None,
            raw_json: Some(raw.to_string()),
        },
    )
    .unwrap();
    let response = board(&conn);
    let hinted = response.tasks.iter().find(|t| t.key == "GENAI-7").unwrap();
    let hint = hinted.done_hint.as_ref().unwrap();
    assert_eq!(hint.key, "GENAI-7");
    assert!(matches!(
        hint.reason,
        HintReason::PrMerged { number: 42, .. }
    ));
    let plain = response.tasks.iter().find(|t| t.key == "GENAI-8").unwrap();
    assert!(plain.done_hint.is_none());
}
