use super::*;
use crate::collectors::github::GitHubAuth;
use crate::db::open_memory;
use crate::models::Event;
use crate::{http, repo};
use chrono::TimeZone;
use httpmock::prelude::*;
use rusqlite::params;
use serde_json::json;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 12, 0, 0).unwrap()
}

fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn ticket(conn: &Connection, key: &str, status: &str) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, status_category, external, fetched_at)
         VALUES (?1, ?2, ?3, 'indeterminate', 0, '2026-10-06T08:00:00Z')",
        params![key, format!("summary {key}"), status],
    )
    .unwrap();
}

fn event_raw(
    conn: &Connection,
    id: &str,
    source: &str,
    key: &str,
    at: DateTime<Utc>,
    raw: Option<String>,
) {
    repo::upsert_event(
        conn,
        &Event {
            id: None,
            source: source.into(),
            source_id: id.into(),
            started_at: iso(at),
            ended_at: None,
            duration_seconds: None,
            title: format!("PR #7: {key} change"),
            details: None,
            repo: Some("acme/app".into()),
            project_path: None,
            jira_issue: Some(key.into()),
            session_id: None,
            tempo_worklog_id: None,
            raw_json: raw,
        },
    )
    .unwrap();
}

fn event(conn: &Connection, id: &str, key: &str, at: DateTime<Utc>) {
    event_raw(conn, id, "claude_session", key, at, None);
}

fn pr(conn: &Connection, id: &str, key: &str, merged_at: Option<&str>) {
    let raw = json!({"kind": "commit", "sha": "", "body": "", "local_folder": null, "merged_at": merged_at});
    event_raw(conn, id, "github_pr", key, now(), Some(raw.to_string()));
}

fn block(conn: &Connection, key: &str, ended: DateTime<Utc>) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, ?3, ?3, 0)",
        params![iso(ended)[..10].to_string(), key, iso(ended)],
    )
    .unwrap();
}

fn kinds(conn: &Connection) -> Vec<NudgeKind> {
    current(conn, now())
        .unwrap()
        .iter()
        .map(|n| n.kind)
        .collect()
}

fn auth(base: String) -> GitHubAuth {
    GitHubAuth {
        token: "t".into(),
        user: "TomasPalsson".into(),
        base,
    }
}

const SEARCH: &str = r#"{"items": [{"number": 12, "title": "Add dashboard",
    "html_url": "https://github.com/acme/app/pull/12",
    "repository_url": "https://api.github.com/repos/acme/app"}]}"#;

#[test]
fn nothing_needing_the_owner_gives_no_nudges() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-1", "In Progress");
    event(&conn, "e1", "GENAI-1", now());
    assert_eq!(current(&conn, now()).unwrap(), vec![]);
}

#[test]
fn review_search_parses_one_nudge_with_url() {
    let n = parse_review_search(SEARCH);
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].kind, NudgeKind::ReviewRequested);
    assert_eq!(
        n[0].url.as_deref(),
        Some("https://github.com/acme/app/pull/12")
    );
    assert!(n[0].text.contains("acme/app#12") && n[0].text.contains("Add dashboard"));
}

#[test]
fn review_search_empty_or_malformed_gives_zero_nudges() {
    assert!(parse_review_search(r#"{"items": []}"#).is_empty());
    // swallow-vs-propagate: a parse error must not take the whole footer down
    assert!(parse_review_search("not json").is_empty());
    assert!(parse_review_search(r#"{"message": "Bad credentials"}"#).is_empty());
}

#[test]
fn cached_reviews_show_until_just_past_ten_minutes() {
    let conn = open_memory().unwrap();
    let cached = parse_review_search(SEARCH);
    // `<` instead of `<=`: exactly 600 s old is still fresh
    store_reviews(
        &conn,
        now() - Duration::seconds(NUDGE_CACHE_SECONDS),
        &cached,
    )
    .unwrap();
    assert_eq!(kinds(&conn), vec![NudgeKind::ReviewRequested]);
    assert!(!reviews_stale(&conn, now()).unwrap());
    // a cache that never expires: 601 s old is dropped
    store_reviews(
        &conn,
        now() - Duration::seconds(NUDGE_CACHE_SECONDS + 1),
        &cached,
    )
    .unwrap();
    assert_eq!(kinds(&conn), vec![]);
    assert!(reviews_stale(&conn, now()).unwrap());
}

#[test]
fn missing_review_cache_is_stale() {
    assert!(reviews_stale(&open_memory().unwrap(), now()).unwrap());
}

#[test]
fn refresh_asks_github_for_open_prs_requesting_the_owner() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET)
            .path("/search/issues")
            .query_param("q", "is:pr is:open review-requested:TomasPalsson");
        then.status(200).body(SEARCH);
    });
    let conn = open_memory().unwrap();
    refresh_reviews(
        &conn,
        now(),
        &http::client().unwrap(),
        &auth(server.base_url()),
    );
    m.assert();
    assert_eq!(kinds(&conn), vec![NudgeKind::ReviewRequested]);
}

#[test]
fn failed_fetch_leaves_no_review_nudges() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(401).body("{}");
    });
    let conn = open_memory().unwrap();
    refresh_reviews(
        &conn,
        now(),
        &http::client().unwrap(),
        &auth(server.base_url()),
    );
    assert_eq!(kinds(&conn), vec![]);
    assert!(reviews_stale(&conn, now()).unwrap());
}

#[test]
fn merged_pr_on_in_progress_ticket_is_a_nudge() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-5", "In Progress");
    pr(&conn, "p1", "GENAI-5", Some("2026-10-05T09:00:00Z"));
    let n = current(&conn, now()).unwrap();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].kind, NudgeKind::MergedNotDone);
    assert!(n[0].text.contains("GENAI-5"));
}

#[test]
fn unmerged_pr_or_other_status_is_no_merged_nudge() {
    let conn = open_memory().unwrap();
    // an open PR is not a merge
    ticket(&conn, "GENAI-5", "In Progress");
    pr(&conn, "p1", "GENAI-5", None);
    // no status filter would nudge here
    ticket(&conn, "GENAI-6", "In Review");
    pr(&conn, "p2", "GENAI-6", Some("2026-10-05T09:00:00Z"));
    assert_eq!(kinds(&conn), vec![]);
}

#[test]
fn stale_needs_more_than_fourteen_days_of_silence() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-8", "In Progress");
    let limit = Duration::days(STALE_TICKET_DAYS);
    // `>=` instead of `>`: exactly 14 days is not yet stale
    event(&conn, "e1", "GENAI-8", now() - limit);
    assert_eq!(kinds(&conn), vec![]);
    conn.execute("DELETE FROM events", []).unwrap();
    // just past the limit
    event(&conn, "e2", "GENAI-8", now() - limit - Duration::seconds(1));
    let n = current(&conn, now()).unwrap();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].kind, NudgeKind::Stale);
    assert!(n[0].text.contains("GENAI-8"));
}

#[test]
fn a_recent_block_keeps_a_ticket_with_old_events_alive() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-8", "In Progress");
    event(&conn, "e1", "GENAI-8", now() - Duration::days(30));
    block(&conn, "GENAI-8", now() - Duration::days(2));
    // looking only at events would flag it
    assert_eq!(kinds(&conn), vec![]);
}

#[test]
fn an_old_block_with_a_recent_event_is_not_stale() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-8", "In Progress");
    block(&conn, "GENAI-8", now() - Duration::days(30));
    event(&conn, "e1", "GENAI-8", now() - Duration::days(1));
    // looking only at blocks would flag it
    assert_eq!(kinds(&conn), vec![]);
}

#[test]
fn stale_ignores_other_statuses_and_prefix_keys() {
    let conn = open_memory().unwrap();
    // not In Progress
    ticket(&conn, "GENAI-8", "Done");
    event(&conn, "e1", "GENAI-8", now() - Duration::days(40));
    // LIKE 'GENAI-1%' would let GENAI-10's fresh event hide GENAI-1's silence
    ticket(&conn, "GENAI-10", "In Progress");
    event(&conn, "e2", "GENAI-10", now());
    ticket(&conn, "GENAI-1", "In Progress");
    event(&conn, "e3", "GENAI-1", now() - Duration::days(40));
    let n = current(&conn, now()).unwrap();
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].kind, NudgeKind::Stale);
    assert!(n[0].text.contains("GENAI-1 "), "{}", n[0].text);
}

#[test]
fn in_progress_ticket_with_no_activity_is_not_stale() {
    let conn = open_memory().unwrap();
    ticket(&conn, "GENAI-9", "In Progress");
    assert_eq!(kinds(&conn), vec![]);
}
