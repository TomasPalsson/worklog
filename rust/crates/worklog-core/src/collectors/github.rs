//! GitHub collector — commits + PRs authored by the user in a date range.
//!
//! Ports the Python `github.collect` function. Uses the REST search API
//! (`/search/commits` + `/search/issues`) so we avoid per-repo enumeration.
//! Jira keys embedded in commit messages / PR titles are extracted and
//! attached as `jira_issue` on the event so the estimator has a strong
//! signal to start from.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use regex::Regex;
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::Deserialize;
use tracing::debug;

use crate::clues_contract::RawRecord;
use crate::http::{self, RequestBuilderExt};
use crate::local_clone;
use crate::models::Event;
use crate::repo;
use crate::scrub;

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
        if is_personal_owner(&c.repository.full_name, &auth.user) {
            continue;
        }
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
        let folder = local_clone::folder_for_repo(&c.repository.full_name);
        let is_local = folder
            .as_deref()
            .is_some_and(|f| local_clone::sha_is_local(Path::new(f), &c.sha));
        let raw_json = serde_json::to_string(&RawRecord::Commit {
            sha: c.sha.clone(),
            body: scrub::scrub_secrets(commit_body(&c.commit.message)),
            local_folder: if is_local { folder.clone() } else { None },
        })
        .ok();
        let ev = Event {
            id: None,
            source: "github_commit".into(),
            source_id: c.sha.clone(),
            started_at: ts,
            ended_at: None,
            duration_seconds: None,
            title,
            details: Some(c.commit.message),
            repo: Some(c.repository.full_name),
            project_path: if is_local { folder } else { None },
            jira_issue,
            session_id: None,
            tempo_worklog_id: None,
            raw_json,
        };
        repo::upsert_event(conn, &ev)?;
        mark_elsewhere(conn, &ev.source, &ev.source_id, !is_local)?;
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
        if is_personal_owner(&repo_name, &auth.user) {
            continue;
        }
        let combined = format!("{} {}", p.title, p.body.as_deref().unwrap_or(""));
        let jira_issue = jira_re.find(&combined).map(|m| m.as_str().to_owned());
        // Search API PRs carry no sha: a clone existing is enough to call it local.
        let folder = local_clone::folder_for_repo(&repo_name);
        let is_local = folder.is_some();
        let raw_json = serde_json::to_string(&RawRecord::Commit {
            sha: String::new(),
            body: scrub::scrub_secrets(p.body.as_deref().unwrap_or("")),
            local_folder: if is_local { folder.clone() } else { None },
        })
        .ok();
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
            project_path: folder,
            jira_issue,
            session_id: None,
            tempo_worklog_id: None,
            raw_json,
        };
        repo::upsert_event(conn, &ev)?;
        mark_elsewhere(conn, &ev.source, &ev.source_id, !is_local)?;
        report.events_written += 1;
    }

    // FR-02/D-06: keep the personal-row deletion re-runnable. If
    // `run_upgrade_006` ran before `github_user` was configured, a
    // personal-owner row could otherwise survive forever — every
    // collect has auth.user in hand, so re-run it here too.
    crate::upgrade_006::delete_personal_rows(conn, Some(&auth.user))?;

    Ok(report)
}

/// The commit message with its title line (and the blank line after it)
/// removed, trimmed - the body stored in `RawRecord::Commit` (D-03).
fn commit_body(message: &str) -> &str {
    message.split_once('\n').map_or("", |(_, rest)| rest).trim()
}

/// D-06: repos owned by the configured personal GitHub account are never tracked.
pub(crate) fn is_personal_owner(repo_full_name: &str, user: &str) -> bool {
    repo_full_name
        .split('/')
        .next()
        .is_some_and(|owner| owner.eq_ignore_ascii_case(user))
}

/// FR-04: a re-collect that later finds the sha locally must clear this.
/// FR-06: an owner-moved row (`elsewhere = 2`) is never touched — it must
/// never flip back to "done elsewhere" or lose its manual placement.
fn mark_elsewhere(conn: &Connection, source: &str, source_id: &str, elsewhere: bool) -> Result<()> {
    conn.execute(
        "UPDATE events SET elsewhere = ?1
          WHERE source = ?2 AND source_id = ?3 AND elsewhere != 2",
        params![elsewhere as i64, source, source_id],
    )?;
    Ok(())
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

// Tests live in github_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "github_test.rs"]
mod tests;
