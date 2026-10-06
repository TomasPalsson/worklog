//! Mirres (Apró's project system) billable status for ticket lines, read
//! through the Apró MCP gateway. Ticket → Tempo account key (Jira + Tempo)
//! → Mirres project → [`LineBilling`], stored per day in
//! `mirres_line_billing`. Nothing here writes to Tempo or blocks.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::collectors::jira::{self, JiraAuth};
use crate::collectors::tempo::{self, TempoAuth};
use crate::http;
use crate::secrets;
use crate::tempo_line_contract::{BillingClass, LineBilling};

const KEYS: [&str; 4] = [
    "mirres_gateway_url",
    "mirres_token_url",
    "mirres_client_id",
    "mirres_client_secret",
];

#[derive(Debug, Clone)]
pub struct MirresAuth {
    /// Full MCP URL, POSTed to as-is.
    pub gateway_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl MirresAuth {
    pub fn from_secrets() -> Result<Self> {
        let vals: Vec<Option<String>> = KEYS
            .iter()
            .map(|k| {
                secrets::get(k)
                    .ok()
                    .flatten()
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty())
            })
            .collect();
        let missing: Vec<&str> = KEYS
            .iter()
            .zip(&vals)
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| *k)
            .collect();
        if !missing.is_empty() {
            anyhow::bail!("Mirres is not configured; missing {}", missing.join(", "));
        }
        let mut it = vals.into_iter().flatten();
        Ok(Self {
            gateway_url: it.next().unwrap_or_default(),
            token_url: it.next().unwrap_or_default(),
            client_id: it.next().unwrap_or_default(),
            client_secret: it.next().unwrap_or_default(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Customer {
    pub short_name: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub responsible: Option<Person>,
}

#[derive(Debug, Deserialize)]
pub struct Person {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Team {
    #[serde(default)]
    pub lead: Option<Person>,
}

#[derive(Debug, Deserialize)]
pub struct IncludedHours {
    #[serde(default)]
    pub contract_status: Option<String>,
    #[serde(default)]
    pub usage_status: Option<String>,
    #[serde(default)]
    pub remaining_hours: Option<f64>,
    #[serde(default)]
    pub counts_as_billed: bool,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub allowance_hours: Option<f64>,
    #[serde(default)]
    pub used_hours: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub tempo_account_key: String,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub project_type: Option<String>,
    #[serde(default)]
    pub billable: bool,
    #[serde(default)]
    pub customer: Option<Customer>,
    #[serde(default)]
    pub included_hours: Option<IncludedHours>,
    #[serde(default)]
    pub owner: Option<Person>,
    #[serde(default)]
    pub team: Option<Team>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub contract_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectsResult {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub not_found: Vec<String>,
}

pub fn fetch_token_with(auth: &MirresAuth, client: &Client) -> Result<String> {
    let resp = client
        .post(&auth.token_url)
        .basic_auth(&auth.client_id, Some(&auth.client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .with_context(|| format!("mirres token at {}", auth.token_url))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("mirres token: HTTP {} — {text}", status.as_u16());
    }
    let v: Value = serde_json::from_str(&text).context("decode mirres token response")?;
    v["access_token"]
        .as_str()
        .map(str::to_owned)
        .context("mirres token response has no access_token")
}

/// Unwraps the gateway reply: JSON-RPC error, tool `isError`, then the
/// optional `content[0].text` JSON and `body` envelopes.
fn unwrap_projects(v: &Value) -> Result<ProjectsResult> {
    if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
        anyhow::bail!("mirres get_projects: {e}");
    }
    let result = v.get("result").unwrap_or(v);
    let text = result["content"][0]["text"].as_str();
    if result["isError"].as_bool() == Some(true) {
        anyhow::bail!("mirres get_projects: {}", text.unwrap_or("tool error"));
    }
    let parsed: Value;
    let mut data = result;
    if let Some(t) = text {
        parsed = serde_json::from_str(t).context("decode mirres tool text")?;
        data = &parsed;
    }
    if let Some(body) = data.get("body").filter(|b| b.is_object()) {
        data = body;
    }
    serde_json::from_value(data.clone()).context("decode mirres projects")
}

pub fn get_projects_with(
    auth: &MirresAuth,
    token: &str,
    keys: &[String],
    as_of: &str,
    client: &Client,
) -> Result<ProjectsResult> {
    if keys.is_empty() {
        return Ok(ProjectsResult::default());
    }
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {
            "name": "mirres___get_projects",
            "arguments": {"account_keys": keys, "as_of_date": as_of},
        },
    });
    let resp = client
        .post(&auth.gateway_url)
        .bearer_auth(token)
        // MCP streamable HTTP: the gateway may answer as JSON or as SSE.
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .with_context(|| format!("mirres gateway at {}", auth.gateway_url))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("mirres get_projects: HTTP {} — {text}", status.as_u16());
    }
    let v: Value = serde_json::from_str(sse_payload(&text))
        .with_context(|| format!("decode mirres gateway response: {text}"))?;
    unwrap_projects(&v)
}

/// The last `data:` line of an SSE reply, or the whole text when it is plain JSON.
fn sse_payload(text: &str) -> &str {
    text.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .next_back()
        .map(str::trim)
        .unwrap_or(text)
}

/// Icelandic decimal comma, trailing `.0` dropped: 1.5 -> "1,5", 2.0 -> "2".
fn fmt_hours(h: f64) -> String {
    format!("{h:.1}").trim_end_matches(".0").replace('.', ",")
}

/// Plain-Icelandic text for a Mirres `contract_status` other than OK.
fn contract_warning(status: &str) -> &'static str {
    match status {
        "NO_HOURS" => "Tímafjölda vantar á samning í Mirres",
        "MIXED_PERIODS" => "Fleiri en ein tegund samnings í Mirres",
        "UNKNOWN_CONTRACT_TYPE" => "Óþekkt samningstegund í Mirres",
        _ => "Samning vantar í Mirres",
    }
}

pub fn classify(p: &Project) -> (BillingClass, Option<String>) {
    let inc = p.included_hours.as_ref();
    let class = if p.billable {
        BillingClass::Billable
    } else if inc.is_some_and(|i| i.counts_as_billed) {
        BillingClass::Included
    } else {
        BillingClass::NotBillable
    };
    let warning = inc.and_then(|i| {
        match i.contract_status.as_deref() {
            Some("OK") | None => {}
            Some(s) => return Some(contract_warning(s).to_owned()),
        }
        match i.usage_status.as_deref() {
            Some("USED_UP") => Some("Innifaldir tímar uppurnir".to_owned()),
            Some("NEAR_LIMIT") => Some(match i.remaining_hours {
                Some(r) => format!("Innifaldir tímar að klárast ({} klst eftir)", fmt_hours(r)),
                None => "Innifaldir tímar að klárast".to_owned(),
            }),
            _ => None,
        }
    });
    (class, warning)
}

fn line_billing(p: &Project) -> LineBilling {
    let (class, warning) = classify(p);
    let customer = p
        .customer
        .as_ref()
        .and_then(|c| c.short_name.clone().or_else(|| c.name.clone()));
    let project = match (customer.clone(), p.project_name.clone()) {
        (Some(c), Some(n)) => Some(format!("{c} · {n}")),
        (None, Some(n)) => Some(n),
        (Some(c), None) => Some(c),
        (None, None) => None,
    };
    LineBilling {
        account_key: p.tempo_account_key.clone(),
        project,
        project_type: p.project_type.clone(),
        class,
        warning,
        customer,
        details: details::details(p),
    }
}

fn not_found_billing(key: &str) -> LineBilling {
    LineBilling {
        account_key: key.to_owned(),
        project: None,
        project_type: None,
        class: BillingClass::NotBillable,
        warning: Some("Ekki virkt Mirres-verkefni".to_owned()),
        customer: None,
        details: None,
    }
}

pub fn fetch_day_billing(day: &str, issues: &[String]) -> Result<Vec<(String, LineBilling)>> {
    let mirres_auth = MirresAuth::from_secrets()?;
    let jira_auth = JiraAuth::from_secrets()?;
    let field_id = secrets::get("jira_account_field_id")
        .ok()
        .flatten()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .context("`jira_account_field_id` is not configured")?;
    let tempo_auth = TempoAuth::from_secrets()?;
    fetch_day_billing_with(
        day,
        issues,
        &jira_auth,
        &field_id,
        &tempo_auth,
        &mirres_auth,
        &http::client()?,
    )
}

pub fn fetch_day_billing_with(
    day: &str,
    issues: &[String],
    jira_auth: &JiraAuth,
    field_id: &str,
    tempo_auth: &TempoAuth,
    mirres_auth: &MirresAuth,
    client: &Client,
) -> Result<Vec<(String, LineBilling)>> {
    // ponytail: one Jira call per ticket; batch with search/jql "key in (...)" if a day grows past ~20 tickets.
    let mut account_ids: Vec<(String, String)> = Vec::new();
    for issue in issues {
        let account = jira::fetch_account_with(jira_auth, issue, field_id, client)
            .with_context(|| format!("jira account of {issue}"))?;
        if let Some(a) = account {
            account_ids.push((issue.clone(), a.id));
        }
    }
    if account_ids.is_empty() {
        return Ok(Vec::new());
    }
    let key_by_id: HashMap<String, String> = tempo::list_accounts_with(tempo_auth, client)?
        .into_iter()
        .map(|a| (a.id.to_string(), a.key))
        .collect();
    let issue_keys: Vec<(String, String)> = account_ids
        .into_iter()
        .filter_map(|(issue, id)| key_by_id.get(&id).map(|k| (issue, k.clone())))
        .collect();
    if issue_keys.is_empty() {
        return Ok(Vec::new());
    }
    let unique: Vec<String> = issue_keys
        .iter()
        .map(|(_, k)| k.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let token = fetch_token_with(mirres_auth, client)?;
    let result = get_projects_with(mirres_auth, &token, &unique, day, client)?;
    let by_key: HashMap<&str, LineBilling> = result
        .projects
        .iter()
        .map(|p| (p.tempo_account_key.as_str(), line_billing(p)))
        .collect();
    // A key neither returned nor listed in `not_found` is treated the same:
    // no active Mirres project.
    Ok(issue_keys
        .into_iter()
        .map(|(issue, key)| {
            let lb = by_key
                .get(key.as_str())
                .cloned()
                .unwrap_or_else(|| not_found_billing(&key));
            (issue, lb)
        })
        .collect())
}

#[path = "mirres_details.rs"]
mod details;

#[path = "mirres_store.rs"]
mod store;
pub(crate) use store::{ensure_details_columns, stored_billing};
pub use store::{overview, store_day};

#[path = "mirres_test.rs"]
#[cfg(test)]
mod tests;
