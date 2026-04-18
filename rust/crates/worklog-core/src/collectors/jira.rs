//! Jira collector — caches the user's open tickets.
//!
//! We only port `fetch_open_tickets` for Stage 2. The richer "Jira activity
//! as events" collector in Python is niche — 99% of the useful ticket data
//! comes through the estimator + GitHub/gcal correlation — so we defer it.
//!
//! Uses the Atlassian Cloud REST v3 search endpoint with basic auth.
//! `statusCategory != Done` filters out Done/Closed/Resolved tickets.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::Deserialize;
use tracing::debug;

use crate::http::{self, RequestBuilderExt};
use crate::models::JiraTicket;
use crate::repo;

use super::CollectReport;

const JQL: &str = "assignee = currentUser() AND statusCategory != Done";
const MAX_RESULTS: u32 = 200;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Issue {
    key: String,
    fields: Fields,
}

#[derive(Debug, Deserialize)]
struct Fields {
    summary: Option<String>,
    status: Option<Status>,
    updated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Status {
    name: Option<String>,
}

/// Credentials captured from the secrets layer. Bundled into a struct so
/// tests can construct them without touching the secret store at all.
#[derive(Debug, Clone)]
pub struct JiraAuth {
    pub base_url: String,
    pub email: String,
    pub token: String,
}

impl JiraAuth {
    /// Load from `secrets::get` (keychain with `.env` fallback).
    pub fn from_secrets() -> Result<Self> {
        use crate::secrets;
        Ok(Self {
            base_url: secrets::require("jira_base_url")?
                .trim_end_matches('/')
                .to_owned(),
            email: secrets::require("jira_email")?,
            token: secrets::require("jira_api_token")?,
        })
    }
}

/// Refresh the `jira_tickets` cache in-place.
pub fn fetch_open_tickets(conn: &Connection, auth: &JiraAuth) -> Result<CollectReport> {
    fetch_open_tickets_with(conn, auth, &http::client()?)
}

/// Test seam — collectors tests inject a mock server by overriding the
/// HTTP client's base URL here. The caller decides whether to reuse the
/// shared client or construct a new one for the call.
pub fn fetch_open_tickets_with(
    conn: &Connection,
    auth: &JiraAuth,
    client: &Client,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "jira",
        ..Default::default()
    };

    // Atlassian retired `/rest/api/3/search` on 2026-04 — new endpoint
    // is `/search/jql` with the same response shape for basic queries.
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let body: SearchResponse = client
        .get(&url)
        .basic_auth(&auth.email, Some(&auth.token))
        .query(&[
            ("jql", JQL),
            ("maxResults", &MAX_RESULTS.to_string()),
            ("fields", "summary,status,updated,project"),
        ])
        .json_ok()
        .with_context(|| format!("jira search at {url}"))?;

    debug!(issues = body.issues.len(), "jira search returned");

    for issue in body.issues {
        let project_key = issue.key.split_once('-').map(|(p, _)| p.to_owned());
        let ticket = JiraTicket {
            key: issue.key,
            summary: issue.fields.summary.unwrap_or_default(),
            status: issue.fields.status.and_then(|s| s.name),
            project_key,
            updated: issue.fields.updated,
        };
        repo::upsert_ticket(conn, &ticket)?;
        report.tickets_written += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use httpmock::prelude::*;
    use serde_json::json;

    #[test]
    fn fetch_open_tickets_upserts_every_issue() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param("jql", JQL);
            then.status(200).json_body(json!({
                "issues": [
                    {
                        "key": "PROJ-1",
                        "fields": {
                            "summary": "fix the thing",
                            "status": { "name": "In Progress" },
                            "updated": "2026-04-17T09:00:00.000+0000"
                        }
                    },
                    {
                        "key": "OTHER-42",
                        "fields": {
                            "summary": "ship the hat",
                            "status": { "name": "To Do" },
                            "updated": "2026-04-15T09:00:00.000+0000"
                        }
                    }
                ]
            }));
        });

        let conn = open_memory().unwrap();
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "tomas@p5.is".into(),
            token: "tok".into(),
        };
        let report = fetch_open_tickets_with(&conn, &auth, &http::client().unwrap()).unwrap();

        mock.assert();
        assert_eq!(report.tickets_written, 2);

        let all = repo::list_tickets(&conn).unwrap();
        let keys: Vec<String> = all.iter().map(|t| t.key.clone()).collect();
        assert!(keys.contains(&"PROJ-1".to_string()));
        assert!(keys.contains(&"OTHER-42".to_string()));
        let proj = all.iter().find(|t| t.key == "PROJ-1").unwrap();
        assert_eq!(proj.summary, "fix the thing");
        assert_eq!(proj.status.as_deref(), Some("In Progress"));
        assert_eq!(proj.project_key.as_deref(), Some("PROJ"));
    }

    #[test]
    fn fetch_open_tickets_propagates_http_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/search/jql");
            then.status(401).body("unauthorized");
        });
        let conn = open_memory().unwrap();
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "bad".into(),
        };
        let err = format!(
            "{:#}",
            fetch_open_tickets_with(&conn, &auth, &http::client().unwrap()).unwrap_err()
        );
        assert!(err.contains("HTTP 401"), "err = {err}");
    }

    #[test]
    fn fetch_open_tickets_upsert_updates_summary_in_place() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/search/jql");
            then.status(200).json_body(json!({
                "issues": [
                    {
                        "key": "PROJ-1",
                        "fields": {
                            "summary": "v2 summary",
                            "status": { "name": "Done" },
                            "updated": "2026-04-18T10:00:00.000+0000"
                        }
                    }
                ]
            }));
        });
        let conn = open_memory().unwrap();
        // Seed with v1 summary.
        repo::upsert_ticket(
            &conn,
            &JiraTicket {
                key: "PROJ-1".into(),
                summary: "v1".into(),
                status: Some("To Do".into()),
                project_key: Some("PROJ".into()),
                updated: Some("old".into()),
            },
        )
        .unwrap();

        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        fetch_open_tickets_with(&conn, &auth, &http::client().unwrap()).unwrap();

        let all = repo::list_tickets(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].summary, "v2 summary");
        assert_eq!(all[0].status.as_deref(), Some("Done"));
    }
}
