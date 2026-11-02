//! Tickets the user logs time on but is not the assignee of — the internal
//! APRO- catch-alls and shared GENAI stories — must still land in the
//! `jira_tickets` cache with their summary.

use super::*;
use crate::db::open_memory;
use httpmock::prelude::*;
use rusqlite::params;
use serde_json::json;

fn block_on(conn: &Connection, key: &str) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES ('2026-11-02', ?1, '2026-11-02T09:00:00+00:00', '2026-11-02T09:30:00+00:00', 1800)",
        params![key],
    )
    .unwrap();
}

fn cached(conn: &Connection, key: &str) -> Option<(String, i64)> {
    conn.query_row(
        "SELECT summary, external FROM jira_tickets WHERE key = ?1",
        params![key],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

fn issue(key: &str, summary: &str) -> serde_json::Value {
    json!({
        "key": key,
        "id": "1",
        "fields": {
            "summary": summary,
            "status": { "name": "In Progress" },
            "updated": "2026-11-01T09:00:00.000+0000"
        }
    })
}

#[test]
fn collect_caches_unassigned_tickets_the_user_logs_time_on() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/search/jql")
            .query_param("jql", JQL);
        then.status(200).json_body(json!({ "issues": [] }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/search/jql")
            .query_param("jql", "project = APRO AND statusCategory != Done");
        then.status(200)
            .json_body(json!({ "issues": [issue("APRO-7", "[O] Samstilling")] }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/APRO-4");
        then.status(200)
            .json_body(issue("APRO-4", "[O] Innra support"));
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/GENAI-846");
        then.status(200).json_body(issue("GENAI-846", "REKSTUR"));
    });
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/GONE-9");
        then.status(404).body("Issue does not exist");
    });

    let conn = open_memory().unwrap();
    block_on(&conn, "APRO-4");
    block_on(&conn, "GENAI-846");
    block_on(&conn, "GONE-9");
    // Tempo sync leaves a summary-less stub for a key it resolved on the fly.
    repo::set_ticket_issue_id(&conn, "GENAI-846", "87472").unwrap();

    let auth = JiraAuth {
        base_url: server.base_url(),
        email: "x".into(),
        token: "y".into(),
    };
    fetch_open_tickets_with(&conn, &auth, &http::client().unwrap())
        .expect("a ticket Jira no longer has must not fail the collect");

    assert_eq!(cached(&conn, "APRO-7"), Some(("[O] Samstilling".into(), 1)));
    assert_eq!(
        cached(&conn, "APRO-4"),
        Some(("[O] Innra support".into(), 1))
    );
    assert_eq!(cached(&conn, "GENAI-846"), Some(("REKSTUR".into(), 1)));
}
