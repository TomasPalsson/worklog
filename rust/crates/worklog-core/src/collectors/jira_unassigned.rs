//! Tickets the user logs time on without being their assignee — the
//! internal `CATCH_ALL_PROJECT` tickets and shared stories. Cached as
//! external so the assigned set (and `mark_unreturned_done`) is unchanged.

use super::*;
use crate::jira_assist_contract::CATCH_ALL_PROJECT;
use tracing::warn;

const FIELDS: &str = "summary,status,updated";

pub(super) fn cache_unassigned_with(
    conn: &Connection,
    auth: &JiraAuth,
    client: &Client,
) -> Result<()> {
    let jql = format!("project = {CATCH_ALL_PROJECT} AND statusCategory != Done");
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let body: SearchResponse = client
        .get(&url)
        .basic_auth(&auth.email, Some(&auth.token))
        .query(&[
            ("jql", jql.as_str()),
            ("maxResults", &MAX_RESULTS.to_string()),
            ("fields", FIELDS),
        ])
        .json_ok()
        .with_context(|| format!("jira search at {url}"))?;
    for issue in body.issues {
        store(conn, issue)?;
    }

    // ponytail: a key Jira never resolves (another tenant, deleted) is retried every collect.
    for key in keys_missing_summary(conn)? {
        let url = format!("{}/rest/api/3/issue/{key}", auth.base_url);
        let fetched: Result<Issue> = client
            .get(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .query(&[("fields", FIELDS)])
            .json_ok()
            .with_context(|| format!("jira issue {key}"));
        match fetched {
            Ok(issue) => store(conn, issue)?,
            Err(e) => warn!(error = %format!("{e:#}"), "skipping uncached ticket"),
        }
    }
    Ok(())
}

fn store(conn: &Connection, issue: Issue) -> Result<()> {
    let ticket = JiraTicket {
        project_key: issue.key.split_once('-').map(|(p, _)| p.to_owned()),
        summary: issue.fields.summary.unwrap_or_default(),
        status: issue.fields.status.as_ref().and_then(|s| s.name.clone()),
        updated: issue.fields.updated,
        issue_id: issue.id,
        key: issue.key,
    };
    repo::upsert_external_ticket(conn, &ticket)?;
    if let Some(status) = &issue.fields.status {
        repo::set_ticket_status(
            conn,
            &ticket.key,
            ticket.status.as_deref().unwrap_or(""),
            status.category(),
        )?;
    }
    Ok(())
}

/// Ticket keys on blocks with no cached summary (absent, or a Tempo stub).
fn keys_missing_summary(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT b.jira_issue FROM blocks b
           LEFT JOIN jira_tickets t ON t.key = b.jira_issue
          WHERE COALESCE(b.jira_issue, '') != '' AND COALESCE(t.summary, '') = ''",
    )?;
    let keys = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(keys)
}
