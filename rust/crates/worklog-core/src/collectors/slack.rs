//! Slack collector — the user's own sent messages, one event per message.
//!
//! See spec 003 T004.
//!
//! Uses Slack's `search.messages` Web API with a user token (bot tokens
//! can't search) so we don't have to enumerate every channel the user is
//! in. Slack signals errors with HTTP 200 + `"ok": false`, so a manual
//! check of that field sits alongside the usual status-code check.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::Deserialize;

use crate::http::{self, RequestBuilderExt};
use crate::models::Event;
use crate::repo;
use crate::routing_contract::{SLACK_TOKEN_KEY, SOURCE_SLACK};

use super::CollectReport;

/// Slack Web API production base. Tests swap in an httpmock URL.
pub const SLACK_API: &str = "https://slack.com/api";

/// Credentials for Slack.
#[derive(Debug, Clone)]
pub struct SlackAuth {
    pub token: String,
}

impl SlackAuth {
    pub fn from_secrets() -> Result<Self> {
        use crate::secrets;
        Ok(Self {
            token: secrets::require(SLACK_TOKEN_KEY)?,
        })
    }
}

/// Collect the user's sent messages between `since` (inclusive) and
/// `until` (exclusive). Both dates are UTC.
pub fn collect(
    conn: &Connection,
    auth: &SlackAuth,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    collect_with(conn, auth, since, until, &http::client()?, SLACK_API)
}

pub fn collect_with(
    conn: &Connection,
    auth: &SlackAuth,
    since: NaiveDate,
    until: NaiveDate,
    client: &Client,
    base_url: &str,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: SOURCE_SLACK,
        ..Default::default()
    };

    // Slack's `after:`/`before:` filters are both exclusive of the given
    // day, so step `since` back one day to keep it inclusive.
    let query = format!(
        "from:me after:{} before:{}",
        since - Duration::days(1),
        until
    );
    let url = format!("{base_url}/search.messages");

    // DM channel names resolve to the counterpart's user id; cache the
    // users.info lookup per run so a chatty DM doesn't refetch per message.
    let mut dm_names: HashMap<String, String> = HashMap::new();

    let mut page = 1u32;
    loop {
        let page_str = page.to_string();
        let body: SearchResponse = client
            .get(&url)
            .bearer_auth(&auth.token)
            .query(&[
                ("query", query.as_str()),
                ("count", "100"),
                // Default sort is relevance score, not time — beyond the
                // first page that would drop messages non-deterministically.
                ("sort", "timestamp"),
                ("page", page_str.as_str()),
            ])
            .json_ok()
            .context("slack search.messages")?;

        if !body.ok {
            anyhow::bail!(
                "slack search.messages: {}",
                body.error.as_deref().unwrap_or("unknown error")
            );
        }

        let Some(messages) = body.messages else {
            break;
        };
        for m in messages.matches {
            let started_at = match parse_slack_ts(&m.ts) {
                Ok(s) => s,
                Err(e) => {
                    report.skipped += 1;
                    report
                        .errors
                        .push(format!("{}:{}: {e:#}", m.channel.id, m.ts));
                    continue;
                }
            };
            let title = if m.channel.is_im {
                resolve_dm_name(
                    client,
                    base_url,
                    &auth.token,
                    &m.channel.name,
                    &mut dm_names,
                    &mut report.errors,
                )
            } else {
                m.channel.name.clone()
            };
            let ev = Event {
                id: None,
                source: SOURCE_SLACK.into(),
                source_id: format!("{}:{}", m.channel.id, m.ts),
                started_at,
                ended_at: None,
                duration_seconds: None,
                title,
                details: Some(m.text),
                repo: None,
                project_path: None,
                jira_issue: None,
                session_id: None,
                tempo_worklog_id: None,
                raw_json: None,
            };
            repo::upsert_event(conn, &ev)?;
            report.events_written += 1;
        }

        match messages.paging {
            Some(p) if p.page < p.pages => page += 1,
            _ => break,
        }
    }

    Ok(report)
}

/// Slack timestamps are `<seconds>.<microseconds>`, e.g.
/// `"1600000000.000100"`. Renders as UTC RFC3339 to match every other
/// collector's `Event::started_at`.
fn parse_slack_ts(ts: &str) -> Result<String> {
    let secs_f: f64 = ts
        .parse()
        .with_context(|| format!("parsing slack ts {ts}"))?;
    let secs = secs_f.trunc() as i64;
    let nanos = (secs_f.fract() * 1_000_000_000.0).round() as u32;
    let dt = DateTime::<Utc>::from_timestamp(secs, nanos)
        .ok_or_else(|| anyhow::anyhow!("invalid slack ts {ts}"))?;
    Ok(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Resolves a DM counterpart's display name via `users.info`, caching per
/// run. A lookup failure falls back to `user_id` and is recorded in
/// `errors` rather than failing the whole collect.
fn resolve_dm_name(
    client: &Client,
    base_url: &str,
    token: &str,
    user_id: &str,
    cache: &mut HashMap<String, String>,
    errors: &mut Vec<String>,
) -> String {
    if let Some(name) = cache.get(user_id) {
        return name.clone();
    }
    let name = match fetch_user_name(client, base_url, token, user_id) {
        Ok(name) => name,
        Err(e) => {
            errors.push(format!("users.info {user_id}: {e:#}"));
            user_id.to_string()
        }
    };
    cache.insert(user_id.to_string(), name.clone());
    name
}

fn fetch_user_name(client: &Client, base_url: &str, token: &str, user_id: &str) -> Result<String> {
    let url = format!("{base_url}/users.info");
    let body: UsersInfoResponse = client
        .get(&url)
        .bearer_auth(token)
        .query(&[("user", user_id)])
        .json_ok()
        .context("slack users.info")?;

    if !body.ok {
        anyhow::bail!("{}", body.error.as_deref().unwrap_or("unknown error"));
    }
    let profile = body.user.map(|u| u.profile).unwrap_or_default();
    Ok(profile
        .real_name
        .filter(|s| !s.is_empty())
        .or_else(|| profile.display_name.filter(|s| !s.is_empty()))
        .unwrap_or_else(|| user_id.to_string()))
}

// ───────────────────────── JSON shapes ─────────────────────────

#[derive(Debug, Deserialize)]
struct SearchResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    messages: Option<Messages>,
}

#[derive(Debug, Deserialize)]
struct Messages {
    #[serde(default)]
    matches: Vec<Match>,
    #[serde(default)]
    paging: Option<Paging>,
}

#[derive(Debug, Deserialize)]
struct Paging {
    page: u32,
    pages: u32,
}

#[derive(Debug, Deserialize)]
struct Match {
    channel: MatchChannel,
    ts: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct MatchChannel {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_im: bool,
}

#[derive(Debug, Deserialize)]
struct UsersInfoResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    user: Option<UserInfo>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(default)]
    profile: UserProfile,
}

#[derive(Debug, Default, Deserialize)]
struct UserProfile {
    #[serde(default)]
    real_name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

// Tests live in slack_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "slack_test.rs"]
mod tests;
