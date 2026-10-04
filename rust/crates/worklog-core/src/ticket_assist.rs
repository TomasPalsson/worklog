//! Start, move and assist-create GENAI tickets (spec 013).

use anyhow::Result;
use reqwest::blocking::Client;
use rusqlite::Connection;

use crate::account_clues;
use crate::collectors::jira::{
    allowed_accounts_with, create_issue_with, fetch_account_with, fetch_detail_with,
    fetch_status_with, list_transitions_with, transition_with, JiraAuth, NewIssue,
};
use crate::jira_assist_contract::{
    is_writable_key, AllowedAccount, AssistCreateBody, AssistCreated, StartOutcome, StartResult,
    TicketView, CREATE_ISSUE_TYPE, WRITE_PROJECT,
};
use crate::tempo_hub_contract::{HubError, StatusCategory, TicketStatus, Transition};
use crate::ticket_text::has_emoji;

const IN_PROGRESS: &str = "In Progress";

fn refuse_unwritable(key: &str) -> Result<()> {
    if is_writable_key(key) {
        return Ok(());
    }
    Err(HubError::InvalidInput(format!(
        "{key} is not a {WRITE_PROJECT} ticket; worklog only moves {WRITE_PROJECT} tickets"
    ))
    .into())
}

/// Finds the live transition into `to_status` and posts it.
fn move_by_name(
    auth: &JiraAuth,
    key: &str,
    to_status: &str,
    client: &Client,
) -> Result<Transition> {
    let found = list_transitions_with(auth, key, client)?
        .into_iter()
        .find(|t| t.to_status.eq_ignore_ascii_case(to_status));
    let Some(transition) = found else {
        let (current, _) = fetch_status_with(auth, key, client)?;
        return Err(HubError::InvalidInput(format!(
            "Jira offers no {to_status} transition from {current}"
        ))
        .into());
    };
    transition_with(auth, key, &transition.id, client)?;
    Ok(transition)
}

pub fn start_ticket_with(
    auth: &JiraAuth,
    field_id: Option<&str>,
    key: &str,
    client: &Client,
) -> Result<StartResult> {
    let mut detail = fetch_detail_with(auth, key, client)?;
    let account = match field_id {
        Some(field_id) => fetch_account_with(auth, key, field_id, client)?,
        None => None,
    };
    let outcome = if !is_writable_key(key) {
        StartOutcome::NotWritable
    } else if detail.status_category == Some(StatusCategory::New) {
        let moved = move_by_name(auth, key, IN_PROGRESS, client)?;
        detail.status = Some(moved.to_status);
        detail.status_category = moved.to_category;
        StartOutcome::Moved
    } else {
        StartOutcome::AlreadyStarted
    };
    let (account_id, account_name) = match account {
        Some(AllowedAccount { id, name }) => (Some(id), Some(name)),
        None => (None, None),
    };
    Ok(StartResult {
        view: TicketView {
            detail,
            account_id,
            account_name,
        },
        outcome,
    })
}

pub fn move_ticket_with(
    auth: &JiraAuth,
    key: &str,
    to_status: &str,
    client: &Client,
) -> Result<TicketStatus> {
    refuse_unwritable(key)?;
    move_by_name(auth, key, to_status, client)?;
    let (status, status_category) = fetch_status_with(auth, key, client)?;
    Ok(TicketStatus {
        key: key.to_owned(),
        status,
        status_category,
    })
}

pub fn assist_create_with(
    conn: &Connection,
    auth: &JiraAuth,
    field_id: &str,
    body: &AssistCreateBody,
    client: &Client,
) -> Result<AssistCreated> {
    if has_emoji(&body.summary) || has_emoji(&body.description) {
        return Err(HubError::InvalidInput("ticket text must not contain emoji".into()).into());
    }
    let account = allowed_accounts_with(auth, WRITE_PROJECT, CREATE_ISSUE_TYPE, field_id, client)?
        .into_iter()
        .find(|a| a.id == body.account_id)
        .ok_or_else(|| {
            HubError::InvalidInput(format!(
                "account {} is not allowed for {WRITE_PROJECT} {CREATE_ISSUE_TYPE}",
                body.account_id
            ))
        })?;
    let created = create_issue_with(
        auth,
        &NewIssue {
            project_key: WRITE_PROJECT.into(),
            summary: body.summary.clone(),
            issue_type: CREATE_ISSUE_TYPE.into(),
            description: Some(body.description.clone()),
            account_field_id: Some(field_id.into()),
            account_value: Some(account.id.clone()),
        },
        client,
    )?;
    account_clues::record_decision(
        conn,
        &body.summary,
        &account,
        body.guessed_account_id.as_deref(),
        &body.clues,
    )?;
    // Never retry the create: the ticket exists, so a failed move reports its key.
    let key = created.key;
    let moved = move_by_name(auth, &key, IN_PROGRESS, client)
        .and_then(|_| fetch_status_with(auth, &key, client))
        .map_err(|e| e.context(format!("created {key} but could not start it")))?;
    Ok(AssistCreated {
        url: format!("{}/browse/{key}", auth.base_url),
        key,
        status: Some(moved.0),
        account,
    })
}

#[cfg(test)]
#[path = "ticket_assist_test.rs"]
mod tests;
