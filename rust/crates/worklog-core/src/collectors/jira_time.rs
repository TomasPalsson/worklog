use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;

use super::jira::{get_all_pages, get_json, str_at, JiraAuth};
use super::tempo::{get_hub_json, TempoAuth};
use crate::estimate_progress_contract::RawWorklog;
use crate::tempo_hub_contract::TEMPO_PAGE_LIMIT;
use crate::tz;

/// The ticket's budget in seconds: time spent + remaining while Jira has
/// remaining time, so a hand-lowered (or raised) remaining estimate counts.
/// Jira floors remaining at 0, so at 0 the original estimate caps the budget
/// and an overrun still shows as over; with nothing logged it stays the
/// original, never a 0 budget that hides the bar.
fn budget_seconds(timetracking: &serde_json::Value) -> Option<i64> {
    let original = timetracking["originalEstimateSeconds"].as_i64();
    let spent = timetracking["timeSpentSeconds"].as_i64().unwrap_or(0);
    match timetracking["remainingEstimateSeconds"].as_i64() {
        Some(r) if r > 0 => Some(spent + r),
        Some(_) if spent > 0 => original.map(|o| o.min(spent)),
        _ => original,
    }
}

/// Budget in seconds per requested key (see [`budget_seconds`]). A key Jira
/// does not return, or one without an estimate, maps to `None`, never 0.
pub fn fetch_estimates_with(
    auth: &JiraAuth,
    keys: &[String],
    client: &Client,
) -> Result<HashMap<String, Option<i64>>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let quoted: Vec<String> = keys
        .iter()
        .map(|k| format!("\"{}\"", k.replace(['"', '\\'], "")))
        .collect();
    let jql = format!("key in ({})", quoted.join(","));
    let url = format!("{}/rest/api/3/search/jql", auth.base_url);
    let body = get_json(
        auth,
        &url,
        &[
            ("jql", jql.as_str()),
            ("maxResults", &keys.len().to_string()),
            ("fields", "timetracking"),
        ],
        "search estimates",
        client,
    )?;
    let mut out: HashMap<String, Option<i64>> = keys.iter().map(|k| (k.clone(), None)).collect();
    for issue in body["issues"].as_array().into_iter().flatten() {
        if let Some(key) = str_at(issue, &["key"]) {
            out.insert(key, budget_seconds(&issue["fields"]["timetracking"]));
        }
    }
    Ok(out)
}

/// Every author's worklogs on `key`, across all pages.
pub fn fetch_worklogs_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<Vec<RawWorklog>> {
    let url = format!("{}/rest/api/3/issue/{key}/worklog", auth.base_url);
    get_all_pages(auth, &url, "worklogs", "issue worklogs", client)?
        .iter()
        // Worklogs by deleted users carry no accountId; they can't be attributed.
        .filter_map(|w| Some(raw_worklog(w, str_at(w, &["author", "accountId"])?)))
        .collect()
}

/// Every author's worklogs on `key` as Tempo reports them. Jira's own worklog
/// list only shows the Tempo app as author, so real people come from Tempo.
pub fn fetch_tempo_worklogs_with(
    tempo: &TempoAuth,
    jira: &JiraAuth,
    key: &str,
    client: &Client,
) -> Result<Vec<RawWorklog>> {
    let issue = get_json(
        jira,
        &format!("{}/rest/api/3/issue/{key}", jira.base_url),
        &[("fields", "summary")],
        "issue id",
        client,
    )?;
    let id = str_at(&issue, &["id"]).context("issue has no id")?;
    let url = format!("{}/worklogs/issue/{id}", tempo.base_url);
    let mut raw: Vec<serde_json::Value> = Vec::new();
    loop {
        let page: serde_json::Value = get_hub_json(
            tempo,
            client,
            &url,
            &[
                ("limit", TEMPO_PAGE_LIMIT.to_string()),
                ("offset", raw.len().to_string()),
            ],
        )?;
        let results = page["results"].as_array().cloned().unwrap_or_default();
        let full_page = results.len() == TEMPO_PAGE_LIMIT;
        raw.extend(results);
        if !full_page {
            break;
        }
    }
    let mut names: HashMap<String, String> = HashMap::new();
    raw.iter()
        .map(|w| {
            let account_id =
                str_at(w, &["author", "accountId"]).context("worklog has no author")?;
            let start = str_at(w, &["startDateTimeUtc"]).context("worklog has no start")?;
            let start = DateTime::parse_from_rfc3339(&start)
                .with_context(|| format!("worklog start {start:?}"))?;
            let name = names
                .entry(account_id.clone())
                .or_insert_with(|| {
                    get_json(
                        jira,
                        &format!("{}/rest/api/3/user", jira.base_url),
                        &[("accountId", account_id.as_str())],
                        "user name",
                        client,
                    )
                    .ok()
                    .and_then(|u| str_at(&u, &["displayName"]))
                    .unwrap_or_else(|| account_id.clone())
                })
                .clone();
            Ok(RawWorklog {
                worklog_id: format!("tempo:{}", w["tempoWorklogId"]),
                account_id,
                name,
                day: tz::local_date(start.with_timezone(&Utc)),
                seconds: w["timeSpentSeconds"].as_i64().unwrap_or(0),
            })
        })
        .collect()
}

fn raw_worklog(w: &serde_json::Value, account_id: String) -> Result<RawWorklog> {
    let started = str_at(w, &["started"]).context("worklog has no started")?;
    let started = DateTime::parse_from_str(&started, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .with_context(|| format!("worklog started {started:?}"))?;
    Ok(RawWorklog {
        worklog_id: str_at(w, &["id"]).context("worklog has no id")?,
        name: str_at(w, &["author", "displayName"]).unwrap_or_else(|| account_id.clone()),
        account_id,
        day: tz::local_date(started.with_timezone(&Utc)),
        seconds: w["timeSpentSeconds"].as_i64().unwrap_or(0),
    })
}

#[cfg(test)]
#[path = "jira_time_test.rs"]
mod tests;

#[cfg(test)]
#[path = "jira_time_estimates_test.rs"]
mod estimates_tests;
