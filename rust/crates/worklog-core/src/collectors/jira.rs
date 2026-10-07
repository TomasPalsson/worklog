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
use crate::jira_assist_contract::AllowedAccount;
use crate::models::{JiraProject, JiraTicket};
use crate::repo;
use crate::tempo_hub_contract::{
    Attachment, HubError, IssueLink, IssueRef, StatusCategory, TicketComment, TicketDetail,
    Transition,
};

use crate::ticket_text::markdown_to_adf;

use super::CollectReport;

const JQL: &str = "assignee = currentUser() AND statusCategory != Done";
const MAX_RESULTS: u32 = 200;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    issues: Vec<Issue>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
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
    issuetype: Option<Named>,
    priority: Option<Named>,
    duedate: Option<String>,
    labels: Option<Vec<String>>,
    parent: Option<Parent>,
    #[serde(default)]
    description: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Parent {
    fields: Option<ParentFields>,
}

#[derive(Debug, Deserialize)]
struct ParentFields {
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Status {
    name: Option<String>,
    #[serde(default, rename = "statusCategory")]
    status_category: Option<CategoryKey>,
}

#[derive(Debug, Deserialize)]
struct CategoryKey {
    key: String,
}

impl Status {
    fn category(&self) -> Option<StatusCategory> {
        self.status_category
            .as_ref()
            .and_then(|c| StatusCategory::parse(&c.key))
    }
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
    let mut returned = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut query = vec![
            ("jql", JQL.to_owned()),
            ("maxResults", MAX_RESULTS.to_string()),
            (
                "fields",
                "summary,status,updated,project,issuetype,priority,duedate,labels,parent,\
                 description"
                    .to_owned(),
            ),
        ];
        if let Some(t) = page_token.take() {
            query.push(("nextPageToken", t));
        }
        let body: SearchResponse = client
            .get(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .query(&query)
            .json_ok()
            .with_context(|| format!("jira search at {url}"))?;

        debug!(issues = body.issues.len(), "jira search returned");

        for issue in body.issues {
            let project_key = issue.key.split_once('-').map(|(p, _)| p.to_owned());
            let status = issue.fields.status;
            let details = repo::TicketDetails {
                issue_type: issue.fields.issuetype.and_then(|n| n.name),
                priority: issue.fields.priority.and_then(|n| n.name),
                due_date: issue.fields.duedate,
                labels: issue.fields.labels.unwrap_or_default(),
                parent_summary: issue
                    .fields
                    .parent
                    .and_then(|p| p.fields)
                    .and_then(|f| f.summary),
                description: Some(adf_to_text(&issue.fields.description)).filter(|d| !d.is_empty()),
            };
            let ticket = JiraTicket {
                key: issue.key,
                summary: issue.fields.summary.unwrap_or_default(),
                status: status.as_ref().and_then(|s| s.name.clone()),
                project_key,
                updated: issue.fields.updated,
                issue_id: issue.id,
            };
            repo::upsert_ticket(conn, &ticket)?;
            let category = status.as_ref().and_then(Status::category);
            repo::set_ticket_status(
                conn,
                &ticket.key,
                ticket.status.as_deref().unwrap_or(""),
                category,
            )?;
            repo::set_ticket_details(conn, &ticket.key, &details)?;
            returned.push(ticket.key);
            report.tickets_written += 1;
        }
        page_token = body.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    repo::mark_unreturned_done(conn, &returned)?;
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
    /// Jira accountId to assign; `None` → unassigned.
    pub assignee_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreatedIssue {
    id: String,
    key: String,
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

/// The `{"fields": ..}` body for `POST /issue`.
fn issue_body(issue: &NewIssue) -> serde_json::Value {
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
        fields.insert("description".into(), markdown_to_adf(desc));
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
    if let Some(id) = &issue.assignee_account_id {
        fields.insert("assignee".into(), serde_json::json!({ "id": id }));
    }
    serde_json::json!({ "fields": serde_json::Value::Object(fields) })
}

pub fn create_issue_with(auth: &JiraAuth, issue: &NewIssue, client: &Client) -> Result<JiraTicket> {
    let body = issue_body(issue);
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

// ───────────────────── status, transitions, comments ─────────────────────

#[derive(Debug, Deserialize)]
struct TransitionsResponse {
    transitions: Vec<RawTransition>,
}

#[derive(Debug, Deserialize)]
struct RawTransition {
    id: String,
    name: String,
    to: Status,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    fields: StatusFields,
}

#[derive(Debug, Deserialize)]
struct StatusFields {
    status: Status,
}

/// Send and return the body, mapping any non-2xx to `HubError::Upstream`
/// so the user sees Jira's own message.
fn send_expecting_success(req: reqwest::blocking::RequestBuilder, what: &str) -> Result<String> {
    let resp = req.send().with_context(|| format!("jira {what}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(HubError::Upstream {
            service: "Jira",
            status: status.as_u16(),
            body: text,
        }
        .into());
    }
    Ok(text)
}

pub fn list_transitions_with(
    auth: &JiraAuth,
    key: &str,
    client: &Client,
) -> Result<Vec<Transition>> {
    let url = format!("{}/rest/api/3/issue/{key}/transitions", auth.base_url);
    let text = send_expecting_success(
        client.get(&url).basic_auth(&auth.email, Some(&auth.token)),
        "list transitions",
    )?;
    let body: TransitionsResponse =
        serde_json::from_str(&text).with_context(|| format!("decode transitions: {text}"))?;
    Ok(body
        .transitions
        .into_iter()
        .map(|t| Transition {
            id: t.id,
            name: t.name,
            to_category: t.to.category(),
            to_status: t.to.name.unwrap_or_default(),
        })
        .collect())
}

pub fn transition_with(
    auth: &JiraAuth,
    key: &str,
    transition_id: &str,
    client: &Client,
) -> Result<()> {
    let url = format!("{}/rest/api/3/issue/{key}/transitions", auth.base_url);
    send_expecting_success(
        client
            .post(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .json(&serde_json::json!({ "transition": { "id": transition_id } })),
        "transition issue",
    )?;
    Ok(())
}

pub fn fetch_status_with(
    auth: &JiraAuth,
    key: &str,
    client: &Client,
) -> Result<(String, Option<StatusCategory>)> {
    let url = format!("{}/rest/api/3/issue/{key}", auth.base_url);
    let text = send_expecting_success(
        client
            .get(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .query(&[("fields", "status")]),
        "fetch status",
    )?;
    let body: StatusResponse =
        serde_json::from_str(&text).with_context(|| format!("decode status: {text}"))?;
    let category = body.fields.status.category();
    Ok((body.fields.status.name.unwrap_or_default(), category))
}

/// Jira renders the account as `{id, name|value}`; `id` is a number or string.
fn account_from(v: &serde_json::Value) -> Option<AllowedAccount> {
    let id = match v.get("id")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let name = str_at(v, &["name"]).or_else(|| str_at(v, &["value"]))?;
    Some(AllowedAccount { id, name })
}

fn get_json(
    auth: &JiraAuth,
    url: &str,
    query: &[(&str, &str)],
    what: &str,
    client: &Client,
) -> Result<serde_json::Value> {
    let text = send_expecting_success(
        client
            .get(url)
            .basic_auth(&auth.email, Some(&auth.token))
            .query(query),
        what,
    )?;
    serde_json::from_str(&text).with_context(|| format!("decode {what}: {text}"))
}

/// Collects every page of a `startAt`/`total` paginated Jira list under `key`.
fn get_all_pages(
    auth: &JiraAuth,
    url: &str,
    key: &str,
    what: &str,
    client: &Client,
) -> Result<Vec<serde_json::Value>> {
    let mut all = Vec::new();
    loop {
        let start = all.len().to_string();
        let page = get_json(auth, url, &[("startAt", &start)], what, client)?;
        let items = page[key].as_array().cloned().unwrap_or_default();
        let got = items.len();
        all.extend(items);
        let more = page["total"]
            .as_u64()
            .is_some_and(|t| (all.len() as u64) < t)
            && page["isLast"].as_bool() != Some(true);
        if got == 0 || !more {
            return Ok(all);
        }
    }
}

/// Accounts Jira will accept on create. An empty list is an error, never
/// "everything allowed".
pub fn allowed_accounts_with(
    auth: &JiraAuth,
    project: &str,
    issue_type: &str,
    field_id: &str,
    client: &Client,
) -> Result<Vec<AllowedAccount>> {
    let base = format!(
        "{}/rest/api/3/issue/createmeta/{project}/issuetypes",
        auth.base_url
    );
    let types = get_all_pages(auth, &base, "issueTypes", "createmeta issue types", client)?;
    let type_id = types
        .iter()
        .find(|t| str_at(t, &["name"]).as_deref() == Some(issue_type))
        .and_then(|t| str_at(t, &["id"]))
        .with_context(|| format!("jira project {project} has no issue type {issue_type}"))?;
    let fields = get_all_pages(
        auth,
        &format!("{base}/{type_id}"),
        "fields",
        "createmeta fields",
        client,
    )?;
    let accounts: Vec<AllowedAccount> = fields
        .iter()
        .find(|f| str_at(f, &["fieldId"]).as_deref() == Some(field_id))
        .and_then(|f| f["allowedValues"].as_array())
        .into_iter()
        .flatten()
        .filter_map(account_from)
        .collect();
    if accounts.is_empty() {
        anyhow::bail!("jira createmeta lists no allowed values for {field_id}");
    }
    Ok(accounts)
}

/// Newest tickets in `GENAI` that carry an account, with their summaries.
pub fn search_accounted_with(
    auth: &JiraAuth,
    field_id: &str,
    limit: usize,
    client: &Client,
) -> Result<Vec<(String, AllowedAccount)>> {
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let fields = format!("summary,{field_id}");
    let jql = "project = GENAI AND \"Account\" is not EMPTY ORDER BY created DESC";
    let body = get_json(
        auth,
        &url,
        &[
            ("jql", jql),
            ("maxResults", &limit.min(MAX_RESULTS as usize).to_string()),
            ("fields", &fields),
        ],
        "search accounted tickets",
        client,
    )?;
    Ok(body["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            let account = account_from(&i["fields"][field_id])?;
            Some((
                str_at(i, &["fields", "summary"]).unwrap_or_default(),
                account,
            ))
        })
        .collect())
}

pub fn fetch_account_with(
    auth: &JiraAuth,
    key: &str,
    field_id: &str,
    client: &Client,
) -> Result<Option<AllowedAccount>> {
    let url = format!("{}/rest/api/3/issue/{key}", auth.base_url);
    let body = get_json(auth, &url, &[("fields", field_id)], "fetch account", client)?;
    Ok(account_from(&body["fields"][field_id]))
}

fn str_at(v: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(str::to_owned)
}

fn attr(node: &serde_json::Value, name: &str) -> Option<String> {
    str_at(node, &["attrs", name])
}

fn adf_children(node: &serde_json::Value, out: &mut String) {
    for child in node["content"].as_array().into_iter().flatten() {
        adf_write(child, out);
    }
}

fn adf_list(node: &serde_json::Value, ordered: bool, out: &mut String) {
    for (i, item) in node["content"].as_array().into_iter().flatten().enumerate() {
        let mut text = String::new();
        adf_children(item, &mut text);
        let prefix = if ordered {
            format!("{}. ", i + 1)
        } else {
            "• ".into()
        };
        out.push_str(&format!("{prefix}{}\n", text.trim()));
    }
    out.push_str("\n\n");
}

fn adf_write(node: &serde_json::Value, out: &mut String) {
    match node["type"].as_str().unwrap_or_default() {
        "text" => out.push_str(node["text"].as_str().unwrap_or_default()),
        "hardBreak" => out.push('\n'),
        "mention" => out.push_str(&attr(node, "text").unwrap_or_default()),
        "emoji" => out.push_str(
            &attr(node, "text")
                .or_else(|| attr(node, "shortName"))
                .unwrap_or_default(),
        ),
        "inlineCard" | "blockCard" => out.push_str(&attr(node, "url").unwrap_or_default()),
        "rule" => out.push_str("———\n\n"),
        "bulletList" => adf_list(node, false, out),
        "orderedList" => adf_list(node, true, out),
        "paragraph" | "heading" | "blockquote" | "codeBlock" | "panel" => {
            adf_children(node, out);
            out.push_str("\n\n");
        }
        _ => adf_children(node, out),
    }
}

/// Flatten Atlassian Document Format (or a legacy plain string) to text.
fn adf_to_text(node: &serde_json::Value) -> String {
    if let Some(s) = node.as_str() {
        return s.to_owned();
    }
    let mut out = String::new();
    adf_write(node, &mut out);
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_owned()
}

pub fn fetch_detail_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<TicketDetail> {
    let url = format!("{}/rest/api/3/issue/{key}", auth.base_url);
    let text = send_expecting_success(
        client
            .get(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .query(&[(
                "fields",
                "summary,status,issuetype,priority,assignee,updated,description,comment,\
                 timetracking,parent,subtasks,issuelinks,reporter,created,components,\
                 fixVersions,attachment,labels,duedate",
            )]),
        "fetch detail",
    )?;
    let body: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("decode detail: {text}"))?;
    let f = &body["fields"];
    let comments = f["comment"]["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| TicketComment {
            id: str_at(c, &["id"]).unwrap_or_default(),
            author: str_at(c, &["author", "displayName"]).unwrap_or_else(|| "Unknown".into()),
            created: str_at(c, &["created"]).unwrap_or_default(),
            body: adf_to_text(&c["body"]),
        })
        .collect();
    Ok(TicketDetail {
        key: key.to_owned(),
        summary: str_at(f, &["summary"]).unwrap_or_default(),
        status: str_at(f, &["status", "name"]),
        status_category: str_at(f, &["status", "statusCategory", "key"])
            .and_then(|k| StatusCategory::parse(&k)),
        issue_type: str_at(f, &["issuetype", "name"]),
        priority: str_at(f, &["priority", "name"]),
        assignee: str_at(f, &["assignee", "displayName"]),
        updated: str_at(f, &["updated"]),
        url: format!("{}/browse/{key}", auth.base_url),
        description: adf_to_text(&f["description"]),
        comments,
        reporter: str_at(f, &["reporter", "displayName"]),
        created: str_at(f, &["created"]),
        labels: str_list(&f["labels"], None),
        due_date: str_at(f, &["duedate"]),
        components: str_list(&f["components"], Some("name")),
        fix_versions: str_list(&f["fixVersions"], Some("name")),
        time_spent_seconds: f["timetracking"]["timeSpentSeconds"].as_i64(),
        original_estimate_seconds: f["timetracking"]["originalEstimateSeconds"].as_i64(),
        remaining_estimate_seconds: f["timetracking"]["remainingEstimateSeconds"].as_i64(),
        parent: issue_ref(&f["parent"]),
        subtasks: f["subtasks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(issue_ref)
            .collect(),
        links: f["issuelinks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(issue_link)
            .collect(),
        attachments: f["attachment"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(attachment)
            .collect(),
    })
}

/// Strings of a JSON array, or of `field` on each element when given.
fn str_list(v: &serde_json::Value, field: Option<&str>) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| match field {
            Some(f) => x[f].as_str(),
            None => x.as_str(),
        })
        .map(str::to_owned)
        .collect()
}

/// A nested issue (`parent`, `subtasks[]`, link target); None without a key.
fn issue_ref(v: &serde_json::Value) -> Option<IssueRef> {
    Some(IssueRef {
        key: str_at(v, &["key"])?,
        summary: str_at(v, &["fields", "summary"]).unwrap_or_default(),
        status: str_at(v, &["fields", "status", "name"]),
        status_category: str_at(v, &["fields", "status", "statusCategory", "key"])
            .and_then(|k| StatusCategory::parse(&k)),
        issue_type: str_at(v, &["fields", "issuetype", "name"]),
    })
}

fn issue_link(v: &serde_json::Value) -> Option<IssueLink> {
    let (side, phrase) = if v["outwardIssue"].is_object() {
        ("outwardIssue", "outward")
    } else {
        ("inwardIssue", "inward")
    };
    Some(IssueLink {
        relation: str_at(v, &["type", phrase])?,
        issue: issue_ref(&v[side])?,
    })
}

fn attachment(v: &serde_json::Value) -> Option<Attachment> {
    Some(Attachment {
        filename: str_at(v, &["filename"])?,
        size_bytes: v["size"].as_i64().unwrap_or_default(),
        url: str_at(v, &["content"])?,
        created: str_at(v, &["created"]),
        author: str_at(v, &["author", "displayName"]),
    })
}

/// Paragraphs split on blank lines; single newlines become `hardBreak`.
fn adf_comment(text: &str) -> serde_json::Value {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs: Vec<_> = text
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let mut content = Vec::new();
            for (i, line) in p.split('\n').enumerate() {
                if i > 0 {
                    content.push(serde_json::json!({ "type": "hardBreak" }));
                }
                if !line.is_empty() {
                    content.push(serde_json::json!({ "type": "text", "text": line }));
                }
            }
            serde_json::json!({ "type": "paragraph", "content": content })
        })
        .collect();
    serde_json::json!({ "type": "doc", "version": 1, "content": paragraphs })
}

pub fn add_comment_with(auth: &JiraAuth, key: &str, text: &str, client: &Client) -> Result<()> {
    let url = format!("{}/rest/api/3/issue/{key}/comment", auth.base_url);
    send_expecting_success(
        client
            .post(&url)
            .basic_auth(&auth.email, Some(&auth.token))
            .json(&serde_json::json!({ "body": adf_comment(text) })),
        "add comment",
    )?;
    Ok(())
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
            assignee_account_id: None,
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
            assignee_account_id: None,
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

    fn test_auth(server: &MockServer) -> JiraAuth {
        JiraAuth {
            base_url: server.base_url(),
            email: "x".into(),
            token: "t".into(),
        }
    }

    fn first_page(req: &HttpMockRequest) -> bool {
        !req.query_params
            .iter()
            .flatten()
            .any(|(k, _)| k == "nextPageToken")
    }

    fn issue(key: &str, cat: &str) -> serde_json::Value {
        json!({"key": key, "fields": {"summary": "s", "status": {
            "name": "St", "statusCategory": {"key": cat}}}})
    }

    #[test]
    fn refresh_pages_fully_stores_category_and_marks_unreturned_done() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param("nextPageToken", "p2");
            then.status(200)
                .json_body(json!({"issues": [issue("A-2", "done")]}));
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .matches(first_page);
            then.status(200).json_body(
                json!({"issues": [issue("A-1", "indeterminate")], "nextPageToken": "p2"}),
            );
        });
        let conn = open_memory().unwrap();
        let mk = |key: &str, ext: i64| {
            conn.execute(
                "INSERT INTO jira_tickets (key, summary, external) VALUES (?1, 's', ?2)",
                rusqlite::params![key, ext],
            )
            .unwrap();
        };
        mk("GONE-1", 0);
        mk("EXT-1", 1);
        let report =
            fetch_open_tickets_with(&conn, &test_auth(&server), &http::client().unwrap()).unwrap();
        assert_eq!(report.tickets_written, 2);
        let cat = |key: &str| -> Option<String> {
            conn.query_row(
                "SELECT status_category FROM jira_tickets WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(cat("A-1").as_deref(), Some("indeterminate"));
        assert_eq!(cat("A-2").as_deref(), Some("done"));
        assert_eq!(cat("GONE-1").as_deref(), Some("done"));
        assert_eq!(cat("EXT-1"), None);
    }

    #[test]
    fn refresh_stores_details_and_nulls_when_absent() {
        let server = MockServer::start();
        let rich = json!({"key": "A-1", "fields": {"summary": "s",
            "status": {"name": "St", "statusCategory": {"key": "new"}},
            "issuetype": {"name": "Bug"}, "priority": {"name": "High"},
            "duedate": "2026-10-04", "labels": ["x", "y"],
            "parent": {"key": "A-0", "fields": {"summary": "Epic one"}},
            "description": {"type": "doc", "content": [{"type": "paragraph",
                "content": [{"type": "text", "text": "Why it matters"}]}]}}});
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/search/jql").query_param(
                "fields",
                "summary,status,updated,project,issuetype,priority,duedate,labels,parent,\
                 description",
            );
            then.status(200)
                .json_body(json!({"issues": [rich, issue("A-2", "new")]}));
        });
        let conn = open_memory().unwrap();
        fetch_open_tickets_with(&conn, &test_auth(&server), &http::client().unwrap()).unwrap();
        let row = |key: &str| -> [Option<String>; 6] {
            conn.query_row(
                "SELECT issue_type, priority, due_date, labels, parent_summary, description
                   FROM jira_tickets WHERE key = ?1",
                [key],
                |r| {
                    Ok([
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ])
                },
            )
            .unwrap()
        };
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(
            row("A-1"),
            [
                s("Bug"),
                s("High"),
                s("2026-10-04"),
                s(r#"["x","y"]"#),
                s("Epic one"),
                s("Why it matters")
            ]
        );
        assert_eq!(row("A-2"), [None, None, None, None, None, None]);
    }

    #[test]
    fn refresh_failing_page_does_not_mark_done() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param("nextPageToken", "p2");
            then.status(500).body("boom");
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .matches(first_page);
            then.status(200)
                .json_body(json!({"issues": [issue("A-1", "new")], "nextPageToken": "p2"}));
        });
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO jira_tickets (key, summary, external) VALUES ('GONE-1', 's', 0)",
            [],
        )
        .unwrap();
        assert!(
            fetch_open_tickets_with(&conn, &test_auth(&server), &http::client().unwrap()).is_err()
        );
        let cat: Option<String> = conn
            .query_row(
                "SELECT status_category FROM jira_tickets WHERE key = 'GONE-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cat, None);
    }

    #[test]
    fn list_transitions_maps_target_status_and_category() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/A-1/transitions");
            then.status(200).json_body(json!({"transitions": [
                {"id": "31", "name": "Finish", "to": {"name": "Done",
                    "statusCategory": {"key": "done"}}},
                {"id": "11", "name": "Weird", "to": {"name": "Limbo",
                    "statusCategory": {"key": "undefined"}}}
            ]}));
        });
        let got =
            list_transitions_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(
            got,
            vec![
                Transition {
                    id: "31".into(),
                    name: "Finish".into(),
                    to_status: "Done".into(),
                    to_category: Some(StatusCategory::Done),
                },
                Transition {
                    id: "11".into(),
                    name: "Weird".into(),
                    to_status: "Limbo".into(),
                    to_category: None,
                },
            ]
        );
    }

    #[test]
    fn transition_posts_id_and_accepts_204() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(POST)
                .path("/rest/api/3/issue/A-1/transitions")
                .json_body(json!({"transition": {"id": "31"}}));
            then.status(204);
        });
        transition_with(&test_auth(&server), "A-1", "31", &http::client().unwrap()).unwrap();
        m.assert();
    }

    #[test]
    fn transition_surfaces_upstream_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/rest/api/3/issue/A-1/transitions");
            then.status(400).body("bad transition");
        });
        let err =
            transition_with(&test_auth(&server), "A-1", "9", &http::client().unwrap()).unwrap_err();
        match err.downcast_ref::<HubError>() {
            Some(HubError::Upstream {
                service,
                status,
                body,
            }) => {
                assert_eq!(
                    (*service, *status, body.as_str()),
                    ("Jira", 400, "bad transition")
                );
            }
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[test]
    fn fetch_status_returns_name_and_category() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/A-1")
                .query_param("fields", "status");
            then.status(200)
                .json_body(json!({"key": "A-1", "fields": {"status": {
                "name": "In Review", "statusCategory": {"key": "indeterminate"}}}}));
        });
        let got = fetch_status_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(
            got,
            ("In Review".to_string(), Some(StatusCategory::Indeterminate))
        );
    }

    const DETAIL_FIELDS: &str =
        "summary,status,issuetype,priority,assignee,updated,description,comment,\
         timetracking,parent,subtasks,issuelinks,reporter,created,components,\
         fixVersions,attachment,labels,duedate";

    fn detail_server() -> MockServer {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/A-1")
                .query_param("fields", DETAIL_FIELDS);
            then.status(200).json_body(json!({"key": "A-1", "fields": {
                "summary": "Fix it",
                "status": {"name": "In Review", "statusCategory": {"key": "indeterminate"}},
                "issuetype": {"name": "Bug"},
                "priority": {"name": "High"},
                "assignee": {"displayName": "Tomas"},
                "updated": "2026-10-01T10:00:00.000+0000",
                "description": {"type": "doc", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "Why"}]}]},
                "comment": {"comments": [
                    {"id": "1", "author": {"displayName": "Ann"}, "created": "2026-09-30T09:00:00.000+0000",
                     "body": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "first"}]}]}},
                    {"id": "2", "created": "2026-10-01T09:00:00.000+0000",
                     "body": "second"}]},
                "reporter": {"displayName": "Rita"},
                "created": "2026-09-01T08:00:00.000+0000",
                "labels": ["backend", "urgent"],
                "duedate": "2026-10-15",
                "components": [{"name": "API"}, {"name": "UI"}],
                "fixVersions": [{"name": "1.2"}],
                "timetracking": {"timeSpentSeconds": 45000,
                    "originalEstimateSeconds": 57600, "remainingEstimateSeconds": 12600},
                "parent": {"key": "A-0", "fields": {"summary": "Epic",
                    "status": {"name": "To Do", "statusCategory": {"key": "new"}},
                    "issuetype": {"name": "Epic"}}},
                "subtasks": [{"key": "A-2", "fields": {"summary": "Sub",
                    "status": {"name": "Done", "statusCategory": {"key": "done"}},
                    "issuetype": {"name": "Sub-task"}}}],
                "issuelinks": [
                    {"type": {"inward": "is blocked by", "outward": "blocks"},
                     "outwardIssue": {"key": "A-3", "fields": {"summary": "Out"}}},
                    {"type": {"inward": "is blocked by", "outward": "blocks"},
                     "inwardIssue": {"key": "A-4", "fields": {"summary": "In",
                        "status": {"name": "Done", "statusCategory": {"key": "done"}}}}}],
                "attachment": [{"filename": "a.png", "size": 1234,
                    "content": "https://j.example/att/1", "created": "2026-09-02T08:00:00.000+0000",
                    "author": {"displayName": "Ann"}}]}}));
        });
        server
    }

    #[test]
    fn fetch_detail_maps_every_field_with_comments_oldest_first() {
        let server = detail_server();
        let got = fetch_detail_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(got.summary, "Fix it");
        assert_eq!(got.status.as_deref(), Some("In Review"));
        assert_eq!(got.status_category, Some(StatusCategory::Indeterminate));
        assert_eq!(got.issue_type.as_deref(), Some("Bug"));
        assert_eq!(got.priority.as_deref(), Some("High"));
        assert_eq!(got.assignee.as_deref(), Some("Tomas"));
        assert_eq!(got.updated.as_deref(), Some("2026-10-01T10:00:00.000+0000"));
        assert_eq!(got.url, format!("{}/browse/A-1", server.base_url()));
        assert_eq!(got.description, "Why");
        let c: Vec<_> = got
            .comments
            .iter()
            .map(|c| (c.id.as_str(), c.author.as_str(), c.body.as_str()))
            .collect();
        assert_eq!(c, vec![("1", "Ann", "first"), ("2", "Unknown", "second")]);
    }

    #[test]
    fn fetch_detail_maps_time_tracking_and_related_issues() {
        let server = detail_server();
        let got = fetch_detail_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(got.reporter.as_deref(), Some("Rita"));
        assert_eq!(got.created.as_deref(), Some("2026-09-01T08:00:00.000+0000"));
        assert_eq!(got.labels, ["backend", "urgent"]);
        assert_eq!(got.due_date.as_deref(), Some("2026-10-15"));
        assert_eq!(got.components, ["API", "UI"]);
        assert_eq!(got.fix_versions, ["1.2"]);
        assert_eq!(
            (
                got.time_spent_seconds,
                got.original_estimate_seconds,
                got.remaining_estimate_seconds
            ),
            (Some(45000), Some(57600), Some(12600))
        );
        let parent = got.parent.unwrap();
        assert_eq!(
            (parent.key.as_str(), parent.summary.as_str()),
            ("A-0", "Epic")
        );
        assert_eq!(parent.status.as_deref(), Some("To Do"));
        assert_eq!(parent.status_category, Some(StatusCategory::New));
        assert_eq!(parent.issue_type.as_deref(), Some("Epic"));
        assert_eq!(got.subtasks.len(), 1);
        assert_eq!(got.subtasks[0].key, "A-2");
        assert_eq!(got.subtasks[0].status_category, Some(StatusCategory::Done));
        let links: Vec<_> = got
            .links
            .iter()
            .map(|l| {
                (
                    l.relation.as_str(),
                    l.issue.key.as_str(),
                    l.issue.status.is_some(),
                )
            })
            .collect();
        assert_eq!(
            links,
            [("blocks", "A-3", false), ("is blocked by", "A-4", true)]
        );
        assert_eq!(got.attachments.len(), 1);
        let a = &got.attachments[0];
        assert_eq!((a.filename.as_str(), a.size_bytes), ("a.png", 1234));
        assert_eq!(a.url, "https://j.example/att/1");
        assert_eq!(a.created.as_deref(), Some("2026-09-02T08:00:00.000+0000"));
        assert_eq!(a.author.as_deref(), Some("Ann"));
    }

    #[test]
    fn fetch_detail_minimal_payload_defaults_new_fields() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/A-1");
            then.status(200).json_body(json!({"fields": {
                "summary": "s", "timetracking": {}, "parent": null, "labels": null,
                "subtasks": [], "issuelinks": [{"type": {"outward": "blocks"}}]}}));
        });
        let got = fetch_detail_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(got.summary, "s");
        assert_eq!(
            (got.reporter, got.created, got.due_date),
            (None, None, None)
        );
        assert!(got.labels.is_empty() && got.components.is_empty() && got.fix_versions.is_empty());
        assert_eq!(
            (
                got.time_spent_seconds,
                got.original_estimate_seconds,
                got.remaining_estimate_seconds
            ),
            (None, None, None)
        );
        assert!(got.parent.is_none());
        assert!(got.subtasks.is_empty() && got.links.is_empty() && got.attachments.is_empty());
    }

    #[test]
    fn fetch_detail_null_description_is_empty() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/A-1");
            then.status(200)
                .json_body(json!({"fields": {"summary": "s", "description": null}}));
        });
        let got = fetch_detail_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap();
        assert_eq!(got.description, "");
        assert!(got.comments.is_empty());
        assert_eq!(got.status, None);
    }

    #[test]
    fn fetch_detail_non_2xx_is_upstream() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/A-1");
            then.status(404).body("nope");
        });
        let err =
            fetch_detail_with(&test_auth(&server), "A-1", &http::client().unwrap()).unwrap_err();
        match err.downcast_ref::<HubError>() {
            Some(HubError::Upstream { status, body, .. }) => {
                assert_eq!((*status, body.as_str()), (404, "nope"));
            }
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    fn doc(content: serde_json::Value) -> serde_json::Value {
        json!({"type": "doc", "content": content})
    }

    fn p(text: &str) -> serde_json::Value {
        json!({"type": "paragraph", "content": [{"type": "text", "text": text}]})
    }

    #[test]
    fn adf_paragraphs_and_hard_breaks() {
        let d = doc(json!([
            {"type": "paragraph", "content": [
                {"type": "text", "text": "a"}, {"type": "hardBreak"}, {"type": "text", "text": "b"}]},
            p("c")
        ]));
        assert_eq!(adf_to_text(&d), "a\nb\n\nc");
        assert_eq!(adf_to_text(&json!("legacy")), "legacy");
        assert_eq!(adf_to_text(&json!(null)), "");
    }

    #[test]
    fn adf_bullet_and_ordered_lists() {
        let item = |t: &str| json!({"type": "listItem", "content": [p(t)]});
        let d = doc(json!([
            {"type": "bulletList", "content": [item("x"), item("y")]},
            {"type": "orderedList", "content": [item("one"), item("two")]}
        ]));
        assert_eq!(adf_to_text(&d), "• x\n• y\n\n1. one\n2. two");
    }

    #[test]
    fn adf_inline_nodes() {
        let d = doc(json!([{"type": "paragraph", "content": [
            {"type": "mention", "attrs": {"text": "@Ann"}},
            {"type": "text", "text": " "},
            {"type": "emoji", "attrs": {"shortName": ":tada:"}},
            {"type": "text", "text": " "},
            {"type": "inlineCard", "attrs": {"url": "https://x.test/1"}}]}]));
        assert_eq!(adf_to_text(&d), "@Ann :tada: https://x.test/1");
    }

    #[test]
    fn adf_nested_blockquote_and_rule() {
        let d = doc(json!([
            p("before"),
            {"type": "blockquote", "content": [{"type": "blockquote", "content": [p("deep")]}]},
            {"type": "rule"},
            p("after")
        ]));
        assert_eq!(adf_to_text(&d), "before\n\ndeep\n\n———\n\nafter");
    }

    #[test]
    fn adf_unknown_node_recurses_into_content() {
        let d = doc(json!([{"type": "mystery", "content": [p("inside")]}]));
        assert_eq!(adf_to_text(&d), "inside");
    }

    #[test]
    fn add_comment_sends_adf_paragraphs_and_hard_breaks() {
        let server = MockServer::start();
        let m = server.mock(|when, then| {
            when.method(POST)
                .path("/rest/api/3/issue/A-1/comment")
                .json_body(json!({"body": {"type": "doc", "version": 1, "content": [
                    {"type": "paragraph", "content": [
                        {"type": "text", "text": "one"},
                        {"type": "hardBreak"},
                        {"type": "text", "text": "two"}]},
                    {"type": "paragraph", "content": [{"type": "text", "text": "three"}]}
                ]}}));
            then.status(201).json_body(json!({"id": "1"}));
        });
        add_comment_with(
            &test_auth(&server),
            "A-1",
            "one\ntwo\n\nthree",
            &http::client().unwrap(),
        )
        .unwrap();
        m.assert();
    }

    #[test]
    fn adf_comment_normalises_crlf() {
        assert_eq!(adf_comment("a\r\nb\r\n\r\nc"), adf_comment("a\nb\n\nc"));
    }

    #[test]
    fn create_issue_description_goes_through_markdown_to_adf() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rest/api/3/issue")
                .json_body_partial(
                    r#"{"fields": {"description": {"type": "doc", "content": [
                    {"type": "heading", "attrs": {"level": 2}}
                ]}}}"#,
                );
            then.status(201)
                .json_body(json!({ "id": "1", "key": "GENAI-1" }));
        });
        let issue = NewIssue {
            project_key: "GENAI".into(),
            summary: "s".into(),
            issue_type: "Story".into(),
            description: Some("## Goal".into()),
            account_field_id: None,
            account_value: None,
            assignee_account_id: None,
        };
        create_issue_with(&test_auth(&server), &issue, &http::client().unwrap()).unwrap();
        mock.assert();
    }

    #[test]
    fn allowed_accounts_reads_createmeta_values_for_the_field() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
            then.status(200).json_body(json!({
                "issueTypes": [{"id": "1", "name": "Bug"}, {"id": "7", "name": "Story"}]
            }));
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
            then.status(200).json_body(json!({ "fields": [
                {"fieldId": "summary"},
                {"fieldId": "customfield_10100", "allowedValues": [
                    {"id": "42", "name": "Acme"},
                    {"id": 7, "value": "Globex"}
                ]}
            ]}));
        });
        let got = allowed_accounts_with(
            &test_auth(&server),
            "GENAI",
            "Story",
            "customfield_10100",
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(
            got,
            vec![
                AllowedAccount {
                    id: "42".into(),
                    name: "Acme".into()
                },
                AllowedAccount {
                    id: "7".into(),
                    name: "Globex".into()
                },
            ]
        );
    }

    #[test]
    fn allowed_accounts_follows_field_pages() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
            then.status(200)
                .json_body(json!({ "issueTypes": [{"id": "7", "name": "Story"}] }));
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7")
                .query_param("startAt", "0");
            then.status(200).json_body(json!({
                "startAt": 0, "total": 2, "fields": [{"fieldId": "summary"}]
            }));
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7")
                .query_param("startAt", "1");
            then.status(200).json_body(json!({
                "startAt": 1, "total": 2, "fields": [
                {"fieldId": "customfield_10100", "allowedValues": [{"id": "42", "name": "Acme"}]}
            ]}));
        });
        let got = allowed_accounts_with(
            &test_auth(&server),
            "GENAI",
            "Story",
            "customfield_10100",
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn allowed_accounts_empty_list_is_an_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
            then.status(200)
                .json_body(json!({ "issueTypes": [{"id": "7", "name": "Story"}] }));
        });
        server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
            then.status(200).json_body(json!({ "fields": [
                {"fieldId": "customfield_10100", "allowedValues": []}
            ]}));
        });
        let err = allowed_accounts_with(
            &test_auth(&server),
            "GENAI",
            "Story",
            "customfield_10100",
            &http::client().unwrap(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("customfield_10100"), "{err:#}");
    }

    #[test]
    fn search_accounted_sends_jql_and_maps_summary_and_account() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rest/api/3/search/jql")
                .query_param(
                    "jql",
                    "project = GENAI AND \"Account\" is not EMPTY ORDER BY created DESC",
                )
                .query_param("maxResults", "200")
                .query_param("fields", "summary,customfield_10100");
            then.status(200).json_body(json!({ "issues": [
                {"id": "1", "key": "GENAI-1", "fields": {
                    "summary": "Acme - fix login",
                    "customfield_10100": {"id": 42, "name": "Acme"}}},
                {"id": "2", "key": "GENAI-2", "fields": {
                    "summary": "no account", "customfield_10100": null}}
            ]}));
        });
        let got = search_accounted_with(
            &test_auth(&server),
            "customfield_10100",
            500,
            &http::client().unwrap(),
        )
        .unwrap();
        mock.assert();
        assert_eq!(
            got,
            vec![(
                "Acme - fix login".to_string(),
                AllowedAccount {
                    id: "42".into(),
                    name: "Acme".into()
                }
            )]
        );
    }

    #[test]
    fn fetch_account_returns_some_or_none() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/GENAI-1");
            then.status(200).json_body(json!({ "fields": {
                "customfield_10100": {"id": "42", "value": "Acme"}}}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/rest/api/3/issue/GENAI-2");
            then.status(200)
                .json_body(json!({ "fields": {"customfield_10100": null}}));
        });
        let client = http::client().unwrap();
        let auth = test_auth(&server);
        assert_eq!(
            fetch_account_with(&auth, "GENAI-1", "customfield_10100", &client).unwrap(),
            Some(AllowedAccount {
                id: "42".into(),
                name: "Acme".into()
            })
        );
        assert_eq!(
            fetch_account_with(&auth, "GENAI-2", "customfield_10100", &client).unwrap(),
            None
        );
    }
}
