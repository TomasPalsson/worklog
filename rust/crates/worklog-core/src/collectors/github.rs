//! GitHub collector — commits + PRs authored by the user in a date range.
//!
//! Ports the Python `github.collect` function. Uses the REST search API
//! (`/search/commits` + `/search/issues`) so we avoid per-repo enumeration.
//! Jira keys embedded in commit messages / PR titles are extracted and
//! attached as `jira_issue` on the event so the estimator has a strong
//! signal to start from.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use regex::Regex;
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::Deserialize;
use tracing::debug;

use crate::http::{self, RequestBuilderExt};
use crate::models::Event;
use crate::repo;

use super::CollectReport;

const GH_API: &str = "https://api.github.com";

/// Credentials + identity for GitHub.
#[derive(Debug, Clone)]
pub struct GitHubAuth {
    pub token: String,
    pub user: String,
    /// Override the GH API base — used by tests with httpmock.
    pub base: String,
}

impl GitHubAuth {
    pub fn from_secrets() -> Result<Self> {
        use crate::secrets;
        Ok(Self {
            token: secrets::require("github_token")?,
            user: secrets::require("github_user")?,
            base: GH_API.to_owned(),
        })
    }
}

/// Collect commits + PRs between `since` (inclusive) and `until`
/// (exclusive). Both dates are UTC.
pub fn collect(
    conn: &Connection,
    auth: &GitHubAuth,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    collect_with(conn, auth, since, until, &http::client()?)
}

pub fn collect_with(
    conn: &Connection,
    auth: &GitHubAuth,
    since: NaiveDate,
    until: NaiveDate,
    client: &Client,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "github",
        ..Default::default()
    };

    let jira_re = Regex::new(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b").unwrap();

    // --- commits -----------------------------------------------------------
    let commits_q = format!("author:{} author-date:{}..{}", auth.user, since, until);
    let commits: CommitSearch = client
        .get(format!("{}/search/commits", auth.base))
        .bearer_auth(&auth.token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .query(&[("q", commits_q.as_str()), ("per_page", "100")])
        .json_ok()
        .context("github commit search")?;
    debug!(total = commits.items.len(), "github commits");
    for c in commits.items {
        let ts = c.commit.author.date;
        let title = c
            .commit
            .message
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(200)
            .collect::<String>();
        let jira_issue = jira_re
            .find(&c.commit.message)
            .map(|m| m.as_str().to_owned());
        let ev = Event {
            id: None,
            source: "github_commit".into(),
            source_id: c.sha,
            started_at: ts,
            ended_at: None,
            duration_seconds: None,
            title,
            details: Some(c.commit.message),
            repo: Some(c.repository.full_name),
            project_path: None,
            jira_issue,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    // --- PRs ---------------------------------------------------------------
    let pr_q = format!("author:{} type:pr created:{}..{}", auth.user, since, until);
    let prs: IssueSearch = client
        .get(format!("{}/search/issues", auth.base))
        .bearer_auth(&auth.token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .query(&[("q", pr_q.as_str()), ("per_page", "100")])
        .json_ok()
        .context("github issue search")?;
    debug!(total = prs.items.len(), "github prs");
    for p in prs.items {
        let repo_name = p
            .repository_url
            .rsplit('/')
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        let combined = format!("{} {}", p.title, p.body.as_deref().unwrap_or(""));
        let jira_issue = jira_re.find(&combined).map(|m| m.as_str().to_owned());
        let ev = Event {
            id: None,
            source: "github_pr".into(),
            source_id: p.id.to_string(),
            started_at: p.created_at,
            ended_at: p.closed_at,
            duration_seconds: None,
            title: format!("PR #{}: {}", p.number, p.title),
            details: p.body.clone(),
            repo: Some(repo_name),
            project_path: None,
            jira_issue,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    Ok(report)
}

// ───────────────────────── JSON shapes ─────────────────────────

#[derive(Debug, Deserialize)]
struct CommitSearch {
    items: Vec<CommitItem>,
}

#[derive(Debug, Deserialize)]
struct CommitItem {
    sha: String,
    repository: Repo,
    commit: CommitPayload,
}

#[derive(Debug, Deserialize)]
struct Repo {
    full_name: String,
}

#[derive(Debug, Deserialize)]
struct CommitPayload {
    author: CommitAuthor,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CommitAuthor {
    date: String,
}

#[derive(Debug, Deserialize)]
struct IssueSearch {
    items: Vec<IssueItem>,
}

#[derive(Debug, Deserialize)]
struct IssueItem {
    id: i64,
    number: i64,
    title: String,
    body: Option<String>,
    created_at: String,
    closed_at: Option<String>,
    repository_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use httpmock::prelude::*;
    use serde_json::json;

    fn auth(base: String) -> GitHubAuth {
        GitHubAuth {
            token: "ghp_test".into(),
            user: "TomasPalsson".into(),
            base,
        }
    }

    #[test]
    fn collect_writes_commits_and_prs_with_jira_keys() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search/commits");
            then.status(200).json_body(json!({
                "items": [
                    {
                        "sha": "abc123",
                        "repository": { "full_name": "org/repo" },
                        "commit": {
                            "author": { "date": "2026-04-18T09:00:00Z" },
                            "message": "PROJ-42 fix login bug\n\nlonger description"
                        }
                    }
                ]
            }));
        });
        server.mock(|when, then| {
            when.method(GET).path("/search/issues");
            then.status(200).json_body(json!({
                "items": [
                    {
                        "id": 1001,
                        "number": 12,
                        "title": "Add dashboard for PROJ-100",
                        "body": null,
                        "created_at": "2026-04-18T10:00:00Z",
                        "closed_at": null,
                        "repository_url": "https://api.github.com/repos/org/repo"
                    }
                ]
            }));
        });

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        let report = collect_with(
            &conn,
            &auth(server.base_url()),
            since,
            until,
            &http::client().unwrap(),
        )
        .unwrap();

        assert_eq!(report.events_written, 2);
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 2);

        let commit = events.iter().find(|e| e.source == "github_commit").unwrap();
        assert_eq!(commit.source_id, "abc123");
        assert_eq!(commit.jira_issue.as_deref(), Some("PROJ-42"));
        assert_eq!(commit.repo.as_deref(), Some("org/repo"));

        let pr = events.iter().find(|e| e.source == "github_pr").unwrap();
        assert_eq!(pr.source_id, "1001");
        assert_eq!(pr.jira_issue.as_deref(), Some("PROJ-100"));
        assert_eq!(pr.repo.as_deref(), Some("org/repo"));
        assert!(pr.title.starts_with("PR #12:"));
    }

    #[test]
    fn collect_is_idempotent_by_source_id() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search/commits");
            then.status(200).json_body(json!({
                "items": [{
                    "sha": "deadbeef",
                    "repository": {"full_name":"o/r"},
                    "commit": {
                        "author": {"date": "2026-04-18T09:00:00Z"},
                        "message": "hello"
                    }
                }]
            }));
        });
        server.mock(|when, then| {
            when.method(GET).path("/search/issues");
            then.status(200).json_body(json!({"items": []}));
        });

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        collect_with(
            &conn,
            &auth(server.base_url()),
            since,
            until,
            &http::client().unwrap(),
        )
        .unwrap();
        collect_with(
            &conn,
            &auth(server.base_url()),
            since,
            until,
            &http::client().unwrap(),
        )
        .unwrap();
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(
            events.len(),
            1,
            "dedupe on (source, source_id) must prevent duplicates"
        );
    }

    #[test]
    fn collect_surfaces_http_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search/commits");
            then.status(403).body("rate limited");
        });
        let conn = open_memory().unwrap();
        let err = format!(
            "{:#}",
            collect_with(
                &conn,
                &auth(server.base_url()),
                NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
                NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
                &http::client().unwrap(),
            )
            .unwrap_err()
        );
        assert!(err.contains("HTTP 403"), "err = {err}");
    }
}
