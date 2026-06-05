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
use crate::models::{JiraProject, JiraTicket};
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
    /// Atlassian's numeric id. Required by Tempo Cloud v4 `/worklogs`.
    id: Option<String>,
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
            issue_id: issue.id,
        };
        repo::upsert_ticket(conn, &ticket)?;
        report.tickets_written += 1;
    }
    Ok(report)
}

/// Cap on `search_tickets` results. The picker only ever surfaces a small
/// number alongside the assigned-to-me set, so a tight cap keeps the
/// daemon responsive and avoids paging concerns.
pub const SEARCH_DEFAULT_LIMIT: u32 = 20;
const SEARCH_MAX_LIMIT: u32 = 50;

/// Live JQL search across all tickets the user has Jira access to.
/// Returns results WITHOUT persisting them — the daemon decides whether
/// to record a pick via `repo::upsert_external_ticket`. JQL string
/// literals are quoted with double-quotes; any `"` or `\` in `q` is
/// escaped so a user typing `foo"` can't break out of the literal.
pub fn search_tickets(auth: &JiraAuth, q: &str, limit: u32) -> Result<Vec<JiraTicket>> {
    search_tickets_with(auth, q, limit, &http::client()?)
}

pub fn search_tickets_with(
    auth: &JiraAuth,
    q: &str,
    limit: u32,
    client: &Client,
) -> Result<Vec<JiraTicket>> {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, SEARCH_MAX_LIMIT);
    let escaped = escape_jql_literal(trimmed);
    // `text ~` does a tokenised match on summary/description; `key = "X"`
    // covers the case where the user types an exact key (which `text ~`
    // wouldn't match because keys aren't tokenised text). Done / Closed
    // tickets stay in the results — search is for picking, not
    // estimating, and people sometimes log time against a just-closed
    // ticket.
    let jql = format!(r#"(text ~ "{escaped}" OR key = "{escaped}") ORDER BY updated DESC"#);
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let body: SearchResponse = client
        .get(&url)
        .basic_auth(&auth.email, Some(&auth.token))
        .query(&[
            ("jql", jql.as_str()),
            ("maxResults", &limit.to_string()),
            ("fields", "summary,status,updated,project"),
        ])
        .json_ok()
        .with_context(|| format!("jira search at {url}"))?;

    debug!(
        q = trimmed,
        issues = body.issues.len(),
        "jira live search returned"
    );

    let tickets = body
        .issues
        .into_iter()
        .map(|issue| {
            let project_key = issue.key.split_once('-').map(|(p, _)| p.to_owned());
            JiraTicket {
                key: issue.key,
                summary: issue.fields.summary.unwrap_or_default(),
                status: issue.fields.status.and_then(|s| s.name),
                project_key,
                updated: issue.fields.updated,
                issue_id: issue.id,
            }
        })
        .collect();
    Ok(tickets)
}

/// Escape a user-supplied substring for embedding inside a JQL
/// double-quoted string literal. JQL uses backslash for escaping and
/// only the quote + the backslash itself are dangerous inside `"..."`.
fn escape_jql_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '"' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

// ───────────────────────── projects ─────────────────────────

#[derive(Debug, Deserialize)]
struct ProjectSearchResponse {
    #[serde(default)]
    values: Vec<ProjectValue>,
}

#[derive(Debug, Deserialize)]
struct ProjectValue {
    id: Option<String>,
    key: String,
    name: Option<String>,
}

/// List the Jira projects the user can see, for the create-ticket picker.
/// Ordered by name so the dropdown is scannable.
pub fn list_projects(auth: &JiraAuth) -> Result<Vec<JiraProject>> {
    list_projects_with(auth, &http::client()?)
}

pub fn list_projects_with(auth: &JiraAuth, client: &Client) -> Result<Vec<JiraProject>> {
    let url = format!("{}/rest/api/3/project/search", auth.base_url);
    let body: ProjectSearchResponse = client
        .get(&url)
        .basic_auth(&auth.email, Some(&auth.token))
        .query(&[("maxResults", "100"), ("orderBy", "name")])
        .json_ok()
        .with_context(|| format!("jira project search at {url}"))?;
    Ok(body
        .values
        .into_iter()
        .map(|p| JiraProject {
            key: p.key,
            name: p.name.unwrap_or_default(),
            id: p.id,
        })
        .collect())
}

// ───────────────────────── issue creation ─────────────────────────

/// Everything needed to open a new Jira issue. The account is the point
/// of the whole feature: `account_field_id` names the Tempo "Account"
/// custom field on the issue, and `account_value` is the account that
/// field is set to — that's what maps the ticket's worklogs to a
/// billable customer.
#[derive(Debug, Clone)]
pub struct NewIssue {
    pub project_key: String,
    pub summary: String,
    pub issue_type: String,
    pub description: Option<String>,
    /// e.g. `customfield_10100`. `None` → no account is set on the issue.
    pub account_field_id: Option<String>,
    /// The account id/key to write to `account_field_id`.
    pub account_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreatedIssue {
    id: String,
    key: String,
}

/// Build the Atlassian Document Format wrapper Jira Cloud v3 requires for
/// the `description` field — a plain string is rejected with a 400.
fn adf_doc(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "doc",
        "version": 1,
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": text }],
        }],
    })
}

/// Tempo's account custom field is backed by the numeric account id, so a
/// digits-only value is sent as a JSON number; anything else (a key, a
/// pre-wrapped value) passes through as a string. If Jira rejects the
/// shape, `create_issue_with` surfaces the full error body so the user
/// can correct the field id/format in settings.
fn account_field_value(raw: &str) -> serde_json::Value {
    let t = raw.trim();
    match t.parse::<i64>() {
        Ok(n) => serde_json::json!(n),
        Err(_) => serde_json::json!(t),
    }
}

/// Create a Jira issue and return it shaped as a `JiraTicket` (with the
/// numeric `issue_id` Tempo needs already populated from the response).
pub fn create_issue(auth: &JiraAuth, issue: &NewIssue) -> Result<JiraTicket> {
    create_issue_with(auth, issue, &http::client()?)
}

pub fn create_issue_with(auth: &JiraAuth, issue: &NewIssue, client: &Client) -> Result<JiraTicket> {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "project".into(),
        serde_json::json!({ "key": issue.project_key }),
    );
    fields.insert("summary".into(), serde_json::json!(issue.summary));
    fields.insert(
        "issuetype".into(),
        serde_json::json!({ "name": issue.issue_type }),
    );
    if let Some(desc) = issue
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        fields.insert("description".into(), adf_doc(desc));
    }
    if let (Some(field), Some(val)) = (
        issue
            .account_field_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        issue
            .account_value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    ) {
        fields.insert(field.to_owned(), account_field_value(val));
    }
    let body = serde_json::json!({ "fields": serde_json::Value::Object(fields) });

    let url = format!("{}/rest/api/3/issue", auth.base_url);
    // Manual send (not `json_ok`) so Jira's validation error body — which
    // names the offending field, crucial for the account custom field —
    // reaches the user verbatim instead of a bare status code.
    let resp = client
        .post(&url)
        .basic_auth(&auth.email, Some(&auth.token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .with_context(|| format!("jira create issue at {url}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("jira create issue: HTTP {} — {text}", status.as_u16());
    }
    let created: CreatedIssue = serde_json::from_str(&text)
        .with_context(|| format!("decode jira create-issue response: {text}"))?;
    debug!(key = %created.key, "created jira issue");
    let project_key = created.key.split_once('-').map(|(p, _)| p.to_owned());
    Ok(JiraTicket {
        key: created.key,
        summary: issue.summary.clone(),
        status: None,
        project_key,
        updated: None,
        issue_id: Some(created.id),
    })
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
                issue_id: None,
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

    #[test]
    fn search_tickets_returns_matches_without_persisting() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param_exists("jql")
                .query_param("maxResults", "20");
            then.status(200).json_body(json!({
                "issues": [
                    {
                        "key": "OTHER-7",
                        "id": "99001",
                        "fields": {
                            "summary": "deploy the thing",
                            "status": { "name": "In Review" },
                            "updated": "2026-04-18T10:00:00.000+0000"
                        }
                    }
                ]
            }));
        });

        let conn = open_memory().unwrap();
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        let results = search_tickets_with(&auth, "deploy", 20, &http::client().unwrap()).unwrap();
        mock.assert();
        assert_eq!(results.len(), 1);
        let t = &results[0];
        assert_eq!(t.key, "OTHER-7");
        assert_eq!(t.summary, "deploy the thing");
        assert_eq!(t.status.as_deref(), Some("In Review"));
        assert_eq!(t.project_key.as_deref(), Some("OTHER"));
        assert_eq!(t.issue_id.as_deref(), Some("99001"));
        // The whole point of the live-search path: results are returned
        // ephemerally and the cache stays empty until the user picks one.
        assert!(repo::list_tickets(&conn).unwrap().is_empty());
    }

    #[test]
    fn search_tickets_short_circuits_on_empty_query() {
        // No mock — the function must return without making an HTTP call.
        let auth = JiraAuth {
            base_url: "http://nope.invalid".into(),
            email: "x".into(),
            token: "y".into(),
        };
        let results = search_tickets_with(&auth, "  ", 20, &http::client().unwrap()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_tickets_escapes_quotes_in_jql_literal() {
        // Use a custom predicate to assert the outgoing JQL contains the
        // backslash-escaped quotes. A raw `"` would terminate the JQL
        // literal early and never appear in the wire format.
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).matches(|req| {
                req.path == "/rest/api/3/search/jql"
                    && req
                        .query_params
                        .iter()
                        .flatten()
                        .any(|(k, v)| k == "jql" && v.contains(r#"odd \"phrase\""#))
            });
            then.status(200).json_body(json!({ "issues": [] }));
        });
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        let _ =
            search_tickets_with(&auth, r#"odd "phrase""#, 20, &http::client().unwrap()).unwrap();
        mock.assert();
    }

    #[test]
    fn search_tickets_caps_limit() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param("maxResults", "50");
            then.status(200).json_body(json!({ "issues": [] }));
        });
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        // Caller asked for 9999 — we clamp to SEARCH_MAX_LIMIT (50).
        let _ = search_tickets_with(&auth, "anything", 9999, &http::client().unwrap()).unwrap();
        mock.assert();
    }

    #[test]
    fn escape_jql_literal_escapes_quotes_and_backslashes() {
        assert_eq!(escape_jql_literal(r#"plain"#), "plain");
        assert_eq!(escape_jql_literal(r#"with "quote""#), r#"with \"quote\""#);
        assert_eq!(escape_jql_literal(r#"back\slash"#), r#"back\\slash"#);
    }

    #[test]
    fn list_projects_maps_values() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/project/search");
            then.status(200).json_body(json!({
                "values": [
                    { "id": "10001", "key": "PROJ", "name": "Project One" },
                    { "id": "10002", "key": "OPS",  "name": "Operations" }
                ]
            }));
        });
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        let projects = list_projects_with(&auth, &http::client().unwrap()).unwrap();
        mock.assert();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].key, "PROJ");
        assert_eq!(projects[0].name, "Project One");
        assert_eq!(projects[0].id.as_deref(), Some("10001"));
    }

    #[test]
    fn create_issue_sends_account_field_as_number_and_returns_ticket() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rest/api/3/issue")
                .json_body_partial(
                    r#"{"fields": {
                        "project": {"key": "PROJ"},
                        "summary": "Investigate billing drift",
                        "issuetype": {"name": "Task"},
                        "customfield_10100": 42
                    }}"#,
                );
            then.status(201)
                .json_body(json!({ "id": "99123", "key": "PROJ-77" }));
        });
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        let issue = NewIssue {
            project_key: "PROJ".into(),
            summary: "Investigate billing drift".into(),
            issue_type: "Task".into(),
            description: Some("look into the variance".into()),
            account_field_id: Some("customfield_10100".into()),
            account_value: Some("42".into()),
        };
        let ticket = create_issue_with(&auth, &issue, &http::client().unwrap()).unwrap();
        mock.assert();
        assert_eq!(ticket.key, "PROJ-77");
        assert_eq!(ticket.issue_id.as_deref(), Some("99123"));
        assert_eq!(ticket.project_key.as_deref(), Some("PROJ"));
        assert_eq!(ticket.summary, "Investigate billing drift");
    }

    #[test]
    fn create_issue_surfaces_jira_error_body() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/rest/api/3/issue");
            then.status(400)
                .body(r#"{"errors":{"customfield_10100":"Account is required"}}"#);
        });
        let auth = JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "y".into(),
        };
        let issue = NewIssue {
            project_key: "PROJ".into(),
            summary: "no account".into(),
            issue_type: "Task".into(),
            description: None,
            account_field_id: None,
            account_value: None,
        };
        let err = format!(
            "{:#}",
            create_issue_with(&auth, &issue, &http::client().unwrap()).unwrap_err()
        );
        assert!(err.contains("HTTP 400"), "err = {err}");
        assert!(err.contains("Account is required"), "err = {err}");
    }

    #[test]
    fn account_field_value_is_number_for_digits_string_otherwise() {
        assert_eq!(account_field_value("42"), json!(42));
        assert_eq!(account_field_value(" 7 "), json!(7));
        assert_eq!(account_field_value("ACME-1"), json!("ACME-1"));
    }
}
