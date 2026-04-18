//! Tempo Cloud worklog sync.
//!
//! One Tempo worklog per block. Writes `tempo_worklog_id` back onto the
//! block row after a successful POST so re-syncing is safe and cheap.
//! Blocks without `jira_issue` are skipped — the review UI prompts the
//! user to assign one before the next sync attempt.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::json;
use tracing::debug;

use crate::http;

use super::CollectReport;

pub const DEFAULT_BASE: &str = "https://api.tempo.io/4";

/// One row's outcome — useful for the CLI to print a table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncResult {
    pub block_id: i64,
    pub status: &'static str,
    pub reason: Option<String>,
    pub tempo_id: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct TempoAuth {
    pub token: String,
    /// Atlassian accountId stored as `jira_email` for now — Stage 2
    /// preserves the Python shape. Fix in Stage 2.1 once we add an
    /// explicit `jira_account_id` key.
    pub author: String,
    pub base_url: String,
}

impl TempoAuth {
    pub fn from_secrets() -> Result<Self> {
        use crate::secrets;
        Ok(Self {
            token: secrets::require("tempo_api_token")?,
            author: secrets::require("jira_email")?,
            base_url: DEFAULT_BASE.to_owned(),
        })
    }
}

pub fn sync_day(
    conn: &Connection,
    auth: &TempoAuth,
    day: NaiveDate,
    dry_run: bool,
) -> Result<(CollectReport, Vec<SyncResult>)> {
    sync_day_with(conn, auth, day, dry_run, &http::client()?)
}

pub fn sync_day_with(
    conn: &Connection,
    auth: &TempoAuth,
    day: NaiveDate,
    dry_run: bool,
    client: &Client,
) -> Result<(CollectReport, Vec<SyncResult>)> {
    let mut report = CollectReport {
        source: "tempo",
        ..Default::default()
    };
    let mut results = Vec::new();

    let rows: Vec<PendingBlock> = {
        let mut stmt = conn.prepare(
            "SELECT id, jira_issue, started_at, duration_seconds, description, day
               FROM blocks
              WHERE day = ?1
                AND (tempo_worklog_id IS NULL OR tempo_worklog_id = '')
              ORDER BY started_at",
        )?;
        let iter = stmt.query_map(params![day.to_string()], |r| {
            Ok(PendingBlock {
                id: r.get(0)?,
                jira_issue: r.get(1)?,
                started_at: r.get(2)?,
                duration_seconds: r.get(3)?,
                description: r.get(4)?,
                day: r.get(5)?,
            })
        })?;
        iter.collect::<Result<Vec<_>, _>>()?
    };

    for b in rows {
        let Some(issue) = b.jira_issue.clone() else {
            report.skipped += 1;
            results.push(SyncResult {
                block_id: b.id,
                status: "skipped",
                reason: Some("no jira_issue — assign one in the UI".into()),
                tempo_id: None,
                payload: None,
                http_status: None,
            });
            continue;
        };

        let payload = json!({
            "issueKey":         issue,
            "timeSpentSeconds": b.duration_seconds,
            "startDate":        b.day,
            "startTime":        start_time(&b.started_at),
            "description":      b.description.clone().unwrap_or_else(|| format!("Work on {issue}")),
            "authorAccountId":  auth.author,
        });

        if dry_run {
            results.push(SyncResult {
                block_id: b.id,
                status: "dry-run",
                reason: None,
                tempo_id: None,
                payload: Some(payload),
                http_status: None,
            });
            continue;
        }

        let url = format!("{}/worklogs", auth.base_url);
        let resp = client
            .post(&url)
            .bearer_auth(&auth.token)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .context("tempo POST")?;
        let http_status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            report
                .errors
                .push(format!("block {}: HTTP {http_status} — {body}", b.id));
            results.push(SyncResult {
                block_id: b.id,
                status: "error",
                reason: Some(body),
                tempo_id: None,
                payload: Some(payload),
                http_status: Some(http_status),
            });
            continue;
        }

        let parsed: TempoCreateResponse = resp.json().context("decode tempo response")?;
        // `tempoWorklogId` is an integer in real responses but we accept
        // string too for resilience. Reject Null / "" / "null" /
        // anything that would silently become a phantom canary.
        let tempo_id = match normalise_tempo_id(&parsed.tempo_worklog_id) {
            Some(s) => s,
            None => {
                let msg = format!(
                    "block {}: tempo returned no usable tempoWorklogId: {}",
                    b.id, parsed.tempo_worklog_id
                );
                report.errors.push(msg.clone());
                results.push(SyncResult {
                    block_id: b.id,
                    status: "error",
                    reason: Some(msg),
                    tempo_id: None,
                    payload: Some(payload),
                    http_status: Some(http_status),
                });
                continue;
            }
        };
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = ?1 WHERE id = ?2",
            params![tempo_id, b.id],
        )?;
        report.synced += 1;
        results.push(SyncResult {
            block_id: b.id,
            status: "synced",
            reason: None,
            tempo_id: Some(tempo_id),
            payload: None,
            http_status: Some(http_status),
        });
        debug!(block_id = b.id, "synced to tempo");
    }
    Ok((report, results))
}

/// Extract `HH:MM:SS` from an ISO-8601 started_at string. Kept as the
/// Python does — Tempo v4's `startTime` is a wall clock in the user's
/// tempo-configured timezone, not UTC, so we pass through verbatim.
fn start_time(iso: &str) -> String {
    if iso.len() >= 19 {
        iso[11..19].to_owned()
    } else {
        "09:00:00".to_owned()
    }
}

#[derive(Debug)]
struct PendingBlock {
    id: i64,
    jira_issue: Option<String>,
    started_at: String,
    duration_seconds: i64,
    description: Option<String>,
    day: String,
}

#[derive(Debug, Deserialize)]
struct TempoCreateResponse {
    #[serde(rename = "tempoWorklogId")]
    tempo_worklog_id: serde_json::Value,
}

/// Extract a non-empty string id from Tempo's `tempoWorklogId` field.
/// Real responses return an integer; we accept string for resilience
/// but reject `null`, empty string, and the literal "null" so a phantom
/// canary can never be written to the DB.
fn normalise_tempo_id(v: &serde_json::Value) -> Option<String> {
    let s = match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => return None,
    };
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use httpmock::prelude::*;
    use serde_json::json;

    fn insert_block(
        conn: &Connection,
        day: &str,
        started: &str,
        ended: &str,
        secs: i64,
        jira_issue: Option<&str>,
        description: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![day, jira_issue, started, ended, secs, description],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()
    }

    fn auth(base: String) -> TempoAuth {
        TempoAuth {
            token: "tempo_tok".into(),
            author: "tomas@p5.is".into(),
            base_url: base,
        }
    }

    #[test]
    fn sync_posts_and_records_tempo_id() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            then.status(200)
                .json_body(json!({ "tempoWorklogId": 12345 }));
        });
        let conn = open_memory().unwrap();
        let id = insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:45:00Z",
            2700,
            Some("PROJ-1"),
            Some("Set up the combobox"),
        );

        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.synced, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "synced");
        assert_eq!(results[0].tempo_id.as_deref(), Some("12345"));

        let stored: Option<String> = conn
            .query_row(
                "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), Some("12345"));
    }

    #[test]
    fn sync_skips_blocks_without_jira_issue() {
        let server = MockServer::start();
        // No mock set — if sync tried to POST it'd fail, proving we skipped.
        let conn = open_memory().unwrap();
        insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            None,
            None,
        );
        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.skipped, 1);
        assert_eq!(report.synced, 0);
        assert_eq!(results[0].status, "skipped");
        assert!(results[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("no jira_issue"));
    }

    #[test]
    fn sync_dry_run_never_posts() {
        let server = MockServer::start();
        // No mock set — fail loudly if dry-run POSTs.
        let conn = open_memory().unwrap();
        insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("edit schema"),
        );
        let (_report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            true,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(results[0].status, "dry-run");
        assert!(results[0].payload.is_some());
    }

    #[test]
    fn sync_wont_reposted_already_synced_blocks() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            then.status(200).json_body(json!({"tempoWorklogId": 1}));
        });
        let conn = open_memory().unwrap();
        insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some(""),
        );
        // Sync twice; second sync must see the stored tempo_worklog_id and skip.
        sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        let (report, _) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.synced, 0);
    }

    #[test]
    fn sync_rejects_empty_tempo_worklog_id_response() {
        // Hardening for the "tempo_worklog_id is the canary" invariant:
        // if Tempo returns {"tempoWorklogId": ""} or {"tempoWorklogId": null}
        // (or the field is missing entirely), we must NOT write an empty
        // or non-integer value to the DB. A subsequent sync should still
        // see the block as unsynced.
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            // Malformed response: no integer id.
            then.status(200).body(r#"{"tempoWorklogId": ""}"#);
        });
        let conn = open_memory().unwrap();
        let bid = insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("x"),
        );
        let _ = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        );
        let tempo_id: Option<String> = conn
            .query_row(
                "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
                params![bid],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            tempo_id.is_none() || tempo_id.as_deref() == Some(""),
            "must not record a phantom tempo id — got {tempo_id:?}"
        );

        // Second sync attempt should also include this block — the guard
        // must treat empty-string and NULL as equivalently unsynced.
        // Seed an empty string deliberately and re-run a dry_run; the
        // block must show up.
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '' WHERE id = ?1",
            params![bid],
        )
        .unwrap();
        let (_, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            true,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(
            results.len(),
            1,
            "empty-string tempo_worklog_id must be treated as unsynced"
        );
    }

    #[test]
    fn sync_records_errors_without_crashing() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            then.status(400).body("bad issue key");
        });
        let conn = open_memory().unwrap();
        insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("NOPE-1"),
            Some("x"),
        );
        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.synced, 0);
        assert_eq!(results[0].status, "error");
        assert_eq!(results[0].http_status, Some(400));
        assert_eq!(report.errors.len(), 1);
    }
}
