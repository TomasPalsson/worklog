//! Where the billing accounts a new ticket may use come from.

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;

use crate::collectors::jira::{self, account_from, get_all_pages, str_at, JiraAuth};
use crate::collectors::tempo::{get_hub_json, TempoAuth};
use crate::jira_assist_contract::AllowedAccount;

/// Accounts Jira will accept on create. Tempo secrets are read only when the
/// field is a Forge field with no allowed values in createmeta.
pub fn allowed_accounts_from_secrets(
    jira: &JiraAuth,
    project: &str,
    issue_type: &str,
    field_id: &str,
    client: &Client,
) -> Result<Vec<AllowedAccount>> {
    allowed_accounts_lazy(
        jira,
        TempoAuth::from_secrets,
        project,
        issue_type,
        field_id,
        client,
    )
}

pub fn allowed_accounts(
    jira: &JiraAuth,
    tempo: Option<&TempoAuth>,
    project: &str,
    issue_type: &str,
    field_id: &str,
    client: &Client,
) -> Result<Vec<AllowedAccount>> {
    allowed_accounts_lazy(
        jira,
        || tempo.cloned().context("tempo_api_token is not set"),
        project,
        issue_type,
        field_id,
        client,
    )
}

fn allowed_accounts_lazy(
    jira: &JiraAuth,
    tempo: impl FnOnce() -> Result<TempoAuth>,
    project: &str,
    issue_type: &str,
    field_id: &str,
    client: &Client,
) -> Result<Vec<AllowedAccount>> {
    let base = format!(
        "{}/rest/api/3/issue/createmeta/{project}/issuetypes",
        jira.base_url
    );
    let types = get_all_pages(jira, &base, "issueTypes", "createmeta issue types", client)?;
    let type_id = types
        .iter()
        .find(|t| str_at(t, &["name"]).as_deref() == Some(issue_type))
        .and_then(|t| str_at(t, &["id"]))
        .with_context(|| format!("jira project {project} has no issue type {issue_type}"))?;
    let fields = get_all_pages(
        jira,
        &format!("{base}/{type_id}"),
        "fields",
        "createmeta fields",
        client,
    )?;
    let field = fields
        .iter()
        .find(|f| str_at(f, &["fieldId"]).as_deref() == Some(field_id));
    if let Some(values) = field.and_then(|f| f["allowedValues"].as_array()) {
        let accounts: Vec<AllowedAccount> = values.iter().filter_map(account_from).collect();
        if accounts.is_empty() {
            bail!("jira createmeta lists no allowed values for {field_id}");
        }
        return Ok(accounts);
    }
    if field.is_none() {
        bail!("jira createmeta lists no allowed values for {field_id}");
    }
    // The cause names the missing secret (tempo_api_token, or the
    // jira_account_id TempoAuth also needs), so it is kept, not replaced.
    let tempo = tempo().with_context(|| {
        format!("{field_id} is a Forge field, so Jira lists no accounts for it and worklog reads them from Tempo")
    })?;
    forge_accounts(jira, &tempo, project, client)
}

/// Open Tempo accounts that are global or linked to the Jira project.
fn forge_accounts(
    jira: &JiraAuth,
    tempo: &TempoAuth,
    project: &str,
    client: &Client,
) -> Result<Vec<AllowedAccount>> {
    let url = format!("{}/rest/api/3/project/{project}", jira.base_url);
    let proj = jira::get_json(jira, &url, &[], "jira project", client)?;
    let project_id = match &proj["id"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => bail!("jira project {project} has no id"),
    };
    // ponytail: limit=1000, no paging; page on metadata.next if a tenant passes 1000 accounts
    let limit = ("limit", "1000".to_string());
    let links: serde_json::Value = get_hub_json(
        tempo,
        client,
        &format!("{}/account-links/project/{project_id}", tempo.base_url),
        std::slice::from_ref(&limit),
    )?;
    let linked: Vec<i64> = links["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            l["account"]["self"]
                .as_str()?
                .rsplit('/')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    let accounts: serde_json::Value = get_hub_json(
        tempo,
        client,
        &format!("{}/accounts", tempo.base_url),
        &[("status", "OPEN".to_string()), limit],
    )?;
    let out: Vec<AllowedAccount> = accounts["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let id = a["id"].as_i64()?;
            let global = a["global"].as_bool().unwrap_or(false);
            (global || linked.contains(&id)).then(|| AllowedAccount {
                id: id.to_string(),
                name: a["name"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect();
    if out.is_empty() {
        bail!("tempo lists no open accounts linked to Jira project {project}");
    }
    Ok(out)
}

#[cfg(test)]
#[path = "account_source_test.rs"]
mod tests;
