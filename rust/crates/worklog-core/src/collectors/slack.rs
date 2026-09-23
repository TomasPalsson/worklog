//! Slack collector — the user's own sent messages, one event per message.
//!
//! See spec 003 T004.
//!
//! Uses Slack's `search.messages` Web API with a user token (bot tokens
//! can't search) so we don't have to enumerate every channel the user is
//! in. Slack signals errors with HTTP 200 + `"ok": false`, so a manual
//! check of that field sits alongside the usual status-code check.

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
    let body: SearchResponse = client
        .get(&url)
        .bearer_auth(&auth.token)
        .query(&[("query", query.as_str()), ("count", "100")])
        .json_ok()
        .context("slack search.messages")?;

    if !body.ok {
        anyhow::bail!(
            "slack search.messages: {}",
            body.error.as_deref().unwrap_or("unknown error")
        );
    }

    let matches = body.messages.map(|m| m.matches).unwrap_or_default();
    for m in matches {
        let started_at = parse_slack_ts(&m.ts)?;
        let ev = Event {
            id: None,
            source: SOURCE_SLACK.into(),
            source_id: format!("{}:{}", m.channel.id, m.ts),
            started_at,
            ended_at: None,
            duration_seconds: None,
            title: m.channel.name,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use httpmock::prelude::*;
    use serde_json::json;

    fn auth() -> SlackAuth {
        SlackAuth {
            token: "xoxp-test".into(),
        }
    }

    #[test]
    fn collect_writes_events_with_channel_and_text() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200).json_body(json!({
                "ok": true,
                "messages": {
                    "matches": [
                        {
                            "channel": { "id": "C123", "name": "general" },
                            "ts": "1776513600.000100",
                            "text": "shipped the fix"
                        }
                    ]
                }
            }));
        });

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        let report = collect_with(
            &conn,
            &auth(),
            since,
            until,
            &http::client().unwrap(),
            &server.base_url(),
        )
        .unwrap();

        assert_eq!(report.events_written, 1);
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.source, "slack");
        assert_eq!(ev.source_id, "C123:1776513600.000100");
        assert_eq!(ev.title, "general");
        assert_eq!(ev.details.as_deref(), Some("shipped the fix"));
        // 1776513600 = 2026-04-18T12:00:00Z (hand-computed via
        // datetime.timestamp() on that UTC instant).
        assert_eq!(ev.started_at, "2026-04-18T12:00:00Z");
    }

    #[test]
    fn collect_is_idempotent_by_source_id() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200).json_body(json!({
                "ok": true,
                "messages": {
                    "matches": [
                        {
                            "channel": { "id": "C1", "name": "eng" },
                            "ts": "1776513600.000100",
                            "text": "hello"
                        }
                    ]
                }
            }));
        });

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        collect_with(
            &conn,
            &auth(),
            since,
            until,
            &http::client().unwrap(),
            &server.base_url(),
        )
        .unwrap();
        collect_with(
            &conn,
            &auth(),
            since,
            until,
            &http::client().unwrap(),
            &server.base_url(),
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
    fn collect_surfaces_ok_false_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(200)
                .json_body(json!({ "ok": false, "error": "invalid_auth" }));
        });

        let conn = open_memory().unwrap();
        let err = format!(
            "{:#}",
            collect_with(
                &conn,
                &auth(),
                NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
                NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
                &http::client().unwrap(),
                &server.base_url(),
            )
            .unwrap_err()
        );
        assert!(err.contains("invalid_auth"), "err = {err}");
    }

    #[test]
    fn collect_surfaces_http_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/search.messages");
            then.status(403).body("rate limited");
        });
        let conn = open_memory().unwrap();
        let err = format!(
            "{:#}",
            collect_with(
                &conn,
                &auth(),
                NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
                NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
                &http::client().unwrap(),
                &server.base_url(),
            )
            .unwrap_err()
        );
        assert!(err.contains("HTTP 403"), "err = {err}");
    }
}
