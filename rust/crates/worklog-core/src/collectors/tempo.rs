//! Tempo Cloud worklog sync.
//!
//! Aggregates blocks sharing the same `(day, jira_issue)` into a single
//! Tempo worklog. Writes the resulting `tempo_worklog_id` back onto
//! every block in the group so re-syncing is safe and cheap. Blocks
//! without `jira_issue` are skipped — the review UI prompts the user
//! to assign one before the next sync attempt.
//!
//! Three group states drive the dispatch (see [`GroupClassification`]):
//!
//! * `AllUnsynced` — POST one aggregated entry, write id back to all.
//! * `SharedId` — PUT the existing entry with the new aggregate;
//!   newly-added unsynced blocks inherit the id.
//! * `MixedLegacy` — fall back to per-block sync. Days previously
//!   synced one-Tempo-entry-per-block keep that shape; we never
//!   silently delete or merge their entries.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::debug;

use crate::collectors::jira::JiraAuth;
use crate::estimate::{self, ModelInvoker};
use crate::http::{self, RequestBuilderExt};
use crate::models::TempoAccount;
use crate::repo;

use super::CollectReport;

pub const DEFAULT_BASE: &str = "https://api.tempo.io/4";

/// We bill in half-hour units only — a worklog is always a multiple of
/// 0.5h. `1800` seconds = 30 minutes.
pub const HALF_HOUR_SECONDS: i64 = 1800;

/// Round a raw second count to the nearest half hour, ties rounding up
/// (15 min → 0.5h, 44 min → 0.5h, 45 min → 1h). A total under 15 minutes
/// rounds to `0`, which the sync path treats as "below the 0.5h minimum,
/// don't log". This is the single point where tracked seconds become
/// billable half-hours, so both the aggregated and legacy sync paths run
/// every duration through it before building the Tempo payload.
pub fn round_to_half_hour(seconds: i64) -> i64 {
    if seconds <= 0 {
        return 0;
    }
    ((seconds + HALF_HOUR_SECONDS / 2) / HALF_HOUR_SECONDS) * HALF_HOUR_SECONDS
}

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
        // `jira_account_id` is the Atlassian accountId (e.g.
        // "557058:abc-123") that Tempo's `authorAccountId` requires.
        // Fall back to `jira_email` for old setups — tempo.io rejects
        // emails ("User is invalid") and the sync function self-heals
        // by calling /myself + caching the real id back into secrets.
        let author = secrets::get("jira_account_id")
            .ok()
            .flatten()
            .or_else(|| secrets::get("jira_email").ok().flatten())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "neither jira_account_id nor jira_email is set — \
                     run `worklog secret set jira_email <you@example.com>` \
                     and let `worklog sync` resolve the account id from /myself"
                )
            })?;
        Ok(Self {
            token: secrets::require("tempo_api_token")?,
            author,
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

/// Sync a day's blocks without an LLM for description generation.
/// Aggregated multi-block groups fall back to a `;`-joined description
/// of distinct per-block descriptions. Callers that want a Claude /
/// LiteLLM summary should use [`sync_day_with_invoker`].
pub fn sync_day_with(
    conn: &Connection,
    auth: &TempoAuth,
    day: NaiveDate,
    dry_run: bool,
    client: &Client,
) -> Result<(CollectReport, Vec<SyncResult>)> {
    sync_day_with_invoker(
        conn,
        auth,
        day,
        dry_run,
        client,
        None,
        estimate::DEFAULT_MODEL,
    )
}

/// Sync a day's blocks, optionally using `invoker` to summarise the
/// joined descriptions of a multi-block ticket-day group into one
/// concise Jira-style sentence. Passing `None` for the invoker (or
/// running `dry_run = true`) skips the LLM call entirely.
pub fn sync_day_with_invoker(
    conn: &Connection,
    auth: &TempoAuth,
    day: NaiveDate,
    dry_run: bool,
    client: &Client,
    invoker: Option<&dyn ModelInvoker>,
    model: &str,
) -> Result<(CollectReport, Vec<SyncResult>)> {
    let mut report = CollectReport {
        source: "tempo",
        ..Default::default()
    };
    let mut results = Vec::new();

    // Tempo's authorAccountId field needs the Atlassian accountId
    // (e.g. "557058:abc-123"), not the email. If the configured author
    // looks like an email, ask Jira's /myself endpoint for the real id
    // and cache it back so the next sync is local-only.
    let author = resolve_account_id(&auth.author, client).unwrap_or_else(|_| auth.author.clone());

    // Eligible blocks = the set the old per-block sync would have
    // touched. Two kinds:
    //   (a) blocks never synced (`tempo_worklog_id` blank) → POST
    //   (b) blocks edited since their last sync (`dirty = 1`) → PUT
    // For aggregation we need the FULL ticket-day total later, but the
    // eligibility filter still gates whether a ticket-day group needs
    // network work this run.
    let eligible = fetch_eligible_blocks(conn, day)?;

    // Group eligible blocks by `jira_issue`. `BTreeMap` keeps a stable
    // order so the result table prints deterministically.
    let mut groups: BTreeMap<Option<String>, Vec<PendingBlock>> = BTreeMap::new();
    for b in eligible {
        groups.entry(b.jira_issue.clone()).or_default().push(b);
    }

    for (issue_opt, eligible_in_group) in groups {
        let Some(issue) = issue_opt else {
            // Unassigned eligible blocks → existing per-block skip.
            for b in eligible_in_group {
                report.skipped += 1;
                results.push(SyncResult {
                    block_id: b.id,
                    status: "skipped",
                    reason: Some("no jira_issue — assign one in the UI".into()),
                    tempo_id: None,
                    payload: None,
                    http_status: None,
                });
            }
            continue;
        };

        // Fetch every non-personal block for this `(day, issue)` so the
        // aggregate total accounts for already-synced clean blocks too.
        // Eligible-only would undercount the duration on re-sync after
        // adding a fresh block to a previously-aggregated group.
        let all_in_group = fetch_blocks_for_issue(conn, day, &issue)?;
        let classification = classify_group(&all_in_group);

        // Resolve numeric issue id once per group (cache hit after first).
        let issue_id = match resolve_issue_id(conn, &issue, client)? {
            Some(id) => id,
            None => {
                let msg = format!(
                    "couldn't resolve numeric issueId for {issue} — \
                     run `worklog collect jira` to refresh the ticket cache, \
                     or check that the key exists"
                );
                for b in &eligible_in_group {
                    report.errors.push(format!("block {}: {msg}", b.id));
                    results.push(SyncResult {
                        block_id: b.id,
                        status: "error",
                        reason: Some(msg.clone()),
                        tempo_id: None,
                        payload: None,
                        http_status: None,
                    });
                }
                continue;
            }
        };

        match classification {
            GroupClassification::AllUnsynced => {
                sync_group_aggregated(
                    conn,
                    auth,
                    client,
                    &issue,
                    &issue_id,
                    &author,
                    &all_in_group,
                    &eligible_in_group,
                    None,
                    dry_run,
                    invoker,
                    model,
                    &mut report,
                    &mut results,
                )?;
            }
            GroupClassification::SharedId(existing_id) => {
                sync_group_aggregated(
                    conn,
                    auth,
                    client,
                    &issue,
                    &issue_id,
                    &author,
                    &all_in_group,
                    &eligible_in_group,
                    Some(&existing_id),
                    dry_run,
                    invoker,
                    model,
                    &mut report,
                    &mut results,
                )?;
            }
            GroupClassification::MixedLegacy => {
                // Legacy days where every block has its own tempo entry
                // stay one-per-block. Don't aggregate behind the user's
                // back — that would orphan all-but-one Tempo entries.
                for b in &eligible_in_group {
                    sync_block_legacy(
                        conn,
                        auth,
                        client,
                        &issue,
                        &issue_id,
                        &author,
                        b,
                        dry_run,
                        &mut report,
                        &mut results,
                    )?;
                }
            }
        }
    }

    Ok((report, results))
}

/// Pull every block that today's WHERE clause would consider "needs
/// action" (unsynced or dirty, non-personal). Identical to the historic
/// query so behaviour for ungrouped paths matches one-to-one.
fn fetch_eligible_blocks(conn: &Connection, day: NaiveDate) -> Result<Vec<PendingBlock>> {
    let mut stmt = conn.prepare(
        "SELECT id, jira_issue, started_at, duration_seconds, description, day,
                tempo_worklog_id
           FROM blocks
          WHERE day = ?1
            AND is_personal = 0
            AND (
              tempo_worklog_id IS NULL
              OR tempo_worklog_id = ''
              OR dirty = 1
            )
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
            tempo_worklog_id: r.get(6)?,
        })
    })?;
    Ok(iter.collect::<Result<Vec<_>, _>>()?)
}

/// All non-personal blocks for a `(day, issue)` ticket — synced and
/// unsynced alike. Used to build the aggregated total + earliest start
/// time + full description set so a re-sync of a previously-aggregated
/// group accounts for blocks that are clean and weren't pulled by the
/// eligibility query.
fn fetch_blocks_for_issue(
    conn: &Connection,
    day: NaiveDate,
    issue: &str,
) -> Result<Vec<PendingBlock>> {
    let mut stmt = conn.prepare(
        "SELECT id, jira_issue, started_at, duration_seconds, description, day,
                tempo_worklog_id
           FROM blocks
          WHERE day = ?1
            AND is_personal = 0
            AND jira_issue = ?2
          ORDER BY started_at",
    )?;
    let iter = stmt.query_map(params![day.to_string(), issue], |r| {
        Ok(PendingBlock {
            id: r.get(0)?,
            jira_issue: r.get(1)?,
            started_at: r.get(2)?,
            duration_seconds: r.get(3)?,
            description: r.get(4)?,
            day: r.get(5)?,
            tempo_worklog_id: r.get(6)?,
        })
    })?;
    Ok(iter.collect::<Result<Vec<_>, _>>()?)
}

/// Three group shapes drive sync dispatch. See module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupClassification {
    /// No block in the group has a Tempo id yet → POST one new entry.
    AllUnsynced,
    /// Every synced block in the group shares the same Tempo id (the
    /// group was previously aggregated, or has exactly one synced
    /// block). PUT updates that id. Any unsynced blocks in the group
    /// inherit the shared id on write-back.
    SharedId(String),
    /// Synced blocks have differing Tempo ids — this day was synced
    /// under the old one-entry-per-block scheme. Fall back to per-block
    /// behaviour; never merge legacy entries silently.
    MixedLegacy,
}

fn classify_group(blocks: &[PendingBlock]) -> GroupClassification {
    let mut seen: Option<String> = None;
    for b in blocks {
        let Some(id) = normalised_tempo_id_str(&b.tempo_worklog_id) else {
            continue;
        };
        match &seen {
            None => seen = Some(id),
            Some(prev) if prev == &id => {}
            Some(_) => return GroupClassification::MixedLegacy,
        }
    }
    match seen {
        Some(id) => GroupClassification::SharedId(id),
        None => GroupClassification::AllUnsynced,
    }
}

fn normalised_tempo_id_str(raw: &Option<String>) -> Option<String> {
    raw.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[allow(clippy::too_many_arguments)]
fn sync_group_aggregated(
    conn: &Connection,
    auth: &TempoAuth,
    client: &Client,
    issue: &str,
    issue_id: &str,
    author: &str,
    all_in_group: &[PendingBlock],
    eligible_in_group: &[PendingBlock],
    existing_id: Option<&str>,
    dry_run: bool,
    invoker: Option<&dyn ModelInvoker>,
    model: &str,
    report: &mut CollectReport,
    results: &mut Vec<SyncResult>,
) -> Result<()> {
    // Aggregate totals from the full group (synced + unsynced) so we
    // PUT the right number on a re-sync that added a new block, then
    // round to the nearest half hour — Tempo only ever sees 0.5h units.
    let raw_seconds: i64 = all_in_group.iter().map(|b| b.duration_seconds).sum();
    let total_seconds = round_to_half_hour(raw_seconds);
    // A whole ticket-day that rounds to 0 (under 15 min of tracked work)
    // is below the half-hour minimum — skip rather than POST a 0s
    // worklog. Any pre-existing Tempo entry is left untouched.
    if total_seconds == 0 {
        for b in eligible_in_group {
            report.skipped += 1;
            results.push(SyncResult {
                block_id: b.id,
                status: "skipped",
                reason: Some(format!(
                    "rounds to 0h — {raw_seconds}s of tracked work is under the 0.5h minimum"
                )),
                tempo_id: None,
                payload: None,
                http_status: None,
            });
        }
        return Ok(());
    }
    let earliest_started = all_in_group
        .iter()
        .map(|b| b.started_at.as_str())
        .min()
        .unwrap_or("");
    let day_str = all_in_group
        .first()
        .map(|b| b.day.clone())
        .unwrap_or_default();
    let descriptions: Vec<String> = all_in_group
        .iter()
        .filter_map(|b| b.description.clone())
        .collect();
    let description = summarize_descriptions(invoker, issue, &descriptions, model, dry_run);

    let payload = json!({
        "issueId":          issue_id,
        "timeSpentSeconds": total_seconds,
        "startDate":        day_str,
        "startTime":        start_time(earliest_started),
        "description":      description,
        "authorAccountId":  author,
    });

    if dry_run {
        let status = if existing_id.is_some() {
            "dry-run-update"
        } else {
            "dry-run"
        };
        for (i, b) in eligible_in_group.iter().enumerate() {
            let agg_status = if eligible_in_group.len() > 1 && i > 0 {
                if existing_id.is_some() {
                    "dry-run-update-aggregated"
                } else {
                    "dry-run-aggregated"
                }
            } else {
                status
            };
            results.push(SyncResult {
                block_id: b.id,
                status: agg_status,
                reason: None,
                tempo_id: existing_id.map(str::to_owned),
                payload: Some(payload.clone()),
                http_status: None,
            });
        }
        return Ok(());
    }

    let (url, method) = match existing_id {
        Some(id) => (format!("{}/worklogs/{}", auth.base_url, id), "PUT"),
        None => (format!("{}/worklogs", auth.base_url), "POST"),
    };
    let req = if method == "PUT" {
        client.put(&url)
    } else {
        client.post(&url)
    };
    let resp = req
        .bearer_auth(&auth.token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .with_context(|| format!("tempo {method}"))?;
    let http_status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        let head_id = eligible_in_group.first().map(|b| b.id).unwrap_or(-1);
        report
            .errors
            .push(format!("group {issue}: HTTP {http_status} — {body}"));
        for b in eligible_in_group {
            results.push(SyncResult {
                block_id: b.id,
                status: "error",
                reason: Some(body.clone()),
                tempo_id: None,
                payload: if b.id == head_id {
                    Some(payload.clone())
                } else {
                    None
                },
                http_status: Some(http_status),
            });
        }
        return Ok(());
    }

    let parsed: TempoCreateResponse = resp.json().context("decode tempo response")?;
    let tempo_id = match (normalise_tempo_id(&parsed.tempo_worklog_id), existing_id) {
        (Some(s), _) => s,
        (None, Some(prev)) => prev.to_owned(),
        (None, None) => {
            let msg = format!(
                "group {issue}: tempo returned no usable tempoWorklogId: {}",
                parsed.tempo_worklog_id
            );
            report.errors.push(msg.clone());
            for b in eligible_in_group {
                results.push(SyncResult {
                    block_id: b.id,
                    status: "error",
                    reason: Some(msg.clone()),
                    tempo_id: None,
                    payload: None,
                    http_status: Some(http_status),
                });
            }
            return Ok(());
        }
    };

    // Write the resolved tempo id back to every eligible block in the
    // group — the new ones get the id for the first time, dirty ones
    // get `dirty = 0` cleared. Clean already-synced blocks aren't in
    // the eligible set so they're left untouched (their state is
    // already correct).
    {
        let mut stmt =
            conn.prepare("UPDATE blocks SET tempo_worklog_id = ?1, dirty = 0 WHERE id = ?2")?;
        for b in eligible_in_group {
            stmt.execute(params![tempo_id, b.id])?;
        }
    }
    report.synced += 1;
    let head_status = if method == "PUT" { "updated" } else { "synced" };
    let tail_status = if method == "PUT" {
        "updated-aggregated"
    } else {
        "synced-aggregated"
    };
    for (i, b) in eligible_in_group.iter().enumerate() {
        let status = if eligible_in_group.len() > 1 && i > 0 {
            tail_status
        } else {
            head_status
        };
        results.push(SyncResult {
            block_id: b.id,
            status,
            reason: None,
            tempo_id: Some(tempo_id.clone()),
            payload: None,
            http_status: Some(http_status),
        });
    }
    debug!(
        issue,
        block_count = eligible_in_group.len(),
        method,
        "synced aggregated group to tempo"
    );
    Ok(())
}

/// One-block-per-Tempo-entry fallback used for `MixedLegacy` groups. A
/// straight extraction of the original loop body — kept verbatim so
/// already-synced days behave identically.
#[allow(clippy::too_many_arguments)]
fn sync_block_legacy(
    conn: &Connection,
    auth: &TempoAuth,
    client: &Client,
    issue: &str,
    issue_id: &str,
    author: &str,
    b: &PendingBlock,
    dry_run: bool,
    report: &mut CollectReport,
    results: &mut Vec<SyncResult>,
) -> Result<()> {
    // Same half-hour rounding as the aggregated path. A legacy block
    // under 15 min rounds to 0 and is skipped — we never PUT/POST a 0s
    // worklog (leaving any existing Tempo entry as-is).
    let timespent = round_to_half_hour(b.duration_seconds);
    if timespent == 0 {
        report.skipped += 1;
        results.push(SyncResult {
            block_id: b.id,
            status: "skipped",
            reason: Some(format!(
                "rounds to 0h — {}s of tracked work is under the 0.5h minimum",
                b.duration_seconds
            )),
            tempo_id: None,
            payload: None,
            http_status: None,
        });
        return Ok(());
    }
    let payload = json!({
        "issueId":          issue_id,
        "timeSpentSeconds": timespent,
        "startDate":        b.day,
        "startTime":        start_time(&b.started_at),
        "description":      b.description.clone().unwrap_or_else(|| format!("Work on {issue}")),
        "authorAccountId":  author,
    });

    let existing_id = b
        .tempo_worklog_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if dry_run {
        results.push(SyncResult {
            block_id: b.id,
            status: if existing_id.is_some() {
                "dry-run-update"
            } else {
                "dry-run"
            },
            reason: None,
            tempo_id: existing_id.map(|s| s.to_owned()),
            payload: Some(payload),
            http_status: None,
        });
        return Ok(());
    }

    let (url, method) = match existing_id {
        Some(id) => (format!("{}/worklogs/{}", auth.base_url, id), "PUT"),
        None => (format!("{}/worklogs", auth.base_url), "POST"),
    };
    let req = if method == "PUT" {
        client.put(&url)
    } else {
        client.post(&url)
    };
    let resp = req
        .bearer_auth(&auth.token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .with_context(|| format!("tempo {method}"))?;
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
        return Ok(());
    }

    let parsed: TempoCreateResponse = resp.json().context("decode tempo response")?;
    let tempo_id = match (normalise_tempo_id(&parsed.tempo_worklog_id), existing_id) {
        (Some(s), _) => s,
        (None, Some(prev)) => prev.to_owned(),
        (None, None) => {
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
            return Ok(());
        }
    };
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = ?1, dirty = 0 WHERE id = ?2",
        params![tempo_id, b.id],
    )?;
    report.synced += 1;
    results.push(SyncResult {
        block_id: b.id,
        status: if method == "PUT" { "updated" } else { "synced" },
        reason: None,
        tempo_id: Some(tempo_id),
        payload: None,
        http_status: Some(http_status),
    });
    debug!(block_id = b.id, method, "synced block to tempo (legacy)");
    Ok(())
}

const SUMMARY_MAX_CHARS: usize = 250;

/// Hard cap on the worklog description sent to Tempo. Truncates on a
/// char boundary and appends `…` to make the cut visible.
fn cap_description(s: &str) -> String {
    if s.chars().count() <= SUMMARY_MAX_CHARS {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(SUMMARY_MAX_CHARS - 1).collect();
        out.push('…');
        out
    }
}

const DESCRIPTION_SYSTEM_PROMPT: &str =
    "You are a Tempo worklog assistant. Given a Jira issue key and a list of \
per-block descriptions of work done on that issue throughout one day, \
produce ONE concise Jira-style imperative sentence (max 140 chars) that \
summarises the day's work on that issue. Use imperative voice \
(\"Implement…\", \"Review…\", \"Fix…\"). Avoid first-person (\"I\", \"we\"). \
Do not invent work that isn't represented in the input. Output ONLY a \
JSON object matching the schema.";

fn description_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "description": {
                "type": "string",
                "description": "One concise Jira-style imperative sentence covering the day's work on this issue. Max 140 chars."
            }
        },
        "required": ["description"],
        "additionalProperties": false
    })
}

/// Build the description sent to Tempo for an aggregated group:
///   * single non-empty source description → use verbatim (no LLM call)
///   * all empty → "Work on {issue}"
///   * multiple distinct → ask the invoker for a one-sentence summary;
///     fall back to `;`-joined distinct descriptions on any failure or
///     when no invoker is configured / dry_run is set.
fn summarize_descriptions(
    invoker: Option<&dyn ModelInvoker>,
    issue: &str,
    descriptions: &[String],
    model: &str,
    dry_run: bool,
) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<String> = Vec::new();
    for d in descriptions {
        let t = d.trim();
        if t.is_empty() {
            continue;
        }
        if seen.insert(t.to_owned()) {
            unique.push(t.to_owned());
        }
    }

    if unique.is_empty() {
        return format!("Work on {issue}");
    }
    if unique.len() == 1 {
        return cap_description(&unique[0]);
    }

    let joined_fallback = || cap_description(&unique.join("; "));

    if dry_run {
        return joined_fallback();
    }
    let Some(invoker) = invoker else {
        return joined_fallback();
    };

    let schema = description_response_schema();
    let user_payload = json!({
        "issue":              issue,
        "block_descriptions": unique,
    });
    let user_msg = serde_json::to_string(&user_payload).unwrap_or_default();
    let reply = match invoker.invoke(DESCRIPTION_SYSTEM_PROMPT, &user_msg, &schema, model) {
        Ok(v) => v,
        Err(e) => {
            debug!(issue, error = %e, "description summariser failed; using joined fallback");
            return joined_fallback();
        }
    };
    let summary = reply
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match summary {
        Some(s) => cap_description(s),
        None => joined_fallback(),
    }
}

/// Delete a worklog entry from Tempo. Used when the user deletes a
/// block locally — otherwise the worklog would orphan in Tempo and
/// the user's daily total would silently include it.
///
/// Treats HTTP 404 as success (the entry's already gone — same end
/// state). All other non-2xx statuses bubble up so the caller can
/// surface the failure to the user instead of swallowing it.
pub fn delete_worklog(auth: &TempoAuth, tempo_worklog_id: &str) -> Result<()> {
    delete_worklog_with(auth, tempo_worklog_id, &http::client()?)
}

pub fn delete_worklog_with(
    auth: &TempoAuth,
    tempo_worklog_id: &str,
    client: &Client,
) -> Result<()> {
    let id = tempo_worklog_id.trim();
    if id.is_empty() {
        return Ok(());
    }
    let url = format!("{}/worklogs/{}", auth.base_url, id);
    let resp = client
        .delete(&url)
        .bearer_auth(&auth.token)
        .send()
        .with_context(|| format!("tempo DELETE {url}"))?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let body = resp.text().unwrap_or_default();
    anyhow::bail!(
        "tempo DELETE /worklogs/{id} returned HTTP {} — {body}",
        status.as_u16()
    )
}

/// Resolve a Jira key (`PROJ-123`) to its numeric Atlassian issue id.
///
/// Walks the cache first; on miss, calls Jira's REST `/issue/{key}`
/// endpoint with basic auth read from the secrets layer, and writes
/// the id back to `jira_tickets` so the next sync is local-only.
///
/// Returns `None` only when Jira itself can't find the key (404) or
/// the credentials are missing — both bubble up as a sync error per
/// block, not a hard fatal.
fn resolve_issue_id(conn: &Connection, key: &str, client: &Client) -> Result<Option<String>> {
    if let Some(id) = repo::get_ticket_issue_id(conn, key)? {
        return Ok(Some(id));
    }
    // Not cached — call Jira.
    let Ok(jira_auth) = JiraAuth::from_secrets() else {
        debug!(key, "jira creds unavailable; can't resolve issue_id");
        return Ok(None);
    };
    let url = format!(
        "{}/rest/api/3/issue/{}?fields=summary",
        jira_auth.base_url, key
    );
    let resp: JiraIssueLookup = match client
        .get(&url)
        .basic_auth(&jira_auth.email, Some(&jira_auth.token))
        .json_ok()
    {
        Ok(r) => r,
        Err(e) => {
            debug!(key, error = %e, "jira issue lookup failed");
            return Ok(None);
        }
    };
    let id = resp.id;
    repo::set_ticket_issue_id(conn, key, &id)?;
    Ok(Some(id))
}

#[derive(Debug, Deserialize)]
struct JiraIssueLookup {
    id: String,
}

/// Normalise the configured `author` value to a real Atlassian accountId.
/// If it already contains a colon (the accountId shape `123:abc`) we
/// trust it. Otherwise call `/rest/api/3/myself` and cache the result
/// in the `jira_account_id` secret so subsequent runs skip the network
/// hop.
fn resolve_account_id(author: &str, client: &Client) -> Result<String> {
    if author.contains(':') && !author.contains('@') {
        // Looks like an accountId already.
        return Ok(author.to_owned());
    }
    let jira_auth = JiraAuth::from_secrets()?;
    let url = format!("{}/rest/api/3/myself", jira_auth.base_url);
    let resp: JiraMyself = client
        .get(&url)
        .basic_auth(&jira_auth.email, Some(&jira_auth.token))
        .json_ok()
        .context("jira /myself")?;
    // Cache it so we don't pay the round-trip every sync. Skip the
    // write when the env var is set — that bypasses the keychain
    // entirely, and writing back would trigger a macOS keychain
    // password prompt the user didn't ask for.
    let env_set = std::env::var("WORKLOG_JIRA_ACCOUNT_ID")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !env_set {
        if let Err(e) = crate::secrets::set("jira_account_id", &resp.account_id) {
            debug!(error = %e, "couldn't cache jira_account_id");
        }
    }
    Ok(resp.account_id)
}

#[derive(Debug, Deserialize)]
struct JiraMyself {
    #[serde(rename = "accountId")]
    account_id: String,
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
    /// Present means "already synced once" — sync uses PUT instead of POST
    /// so the existing Tempo entry gets updated rather than duplicated.
    tempo_worklog_id: Option<String>,
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

// ───────────────────────── accounts ─────────────────────────

#[derive(Debug, Deserialize)]
struct AccountsResponse {
    #[serde(default)]
    results: Vec<AccountValue>,
}

#[derive(Debug, Deserialize)]
struct AccountValue {
    id: i64,
    key: String,
    name: String,
    #[serde(default)]
    customer: Option<CustomerValue>,
}

#[derive(Debug, Deserialize)]
struct CustomerValue {
    name: Option<String>,
}

/// List the Tempo accounts (billing buckets) for the create-ticket
/// account picker. The chosen account's `id` is written to the new
/// issue's Tempo account custom field so its worklogs map to a customer.
pub fn list_accounts(auth: &TempoAuth) -> Result<Vec<TempoAccount>> {
    list_accounts_with(auth, &http::client()?)
}

pub fn list_accounts_with(auth: &TempoAuth, client: &Client) -> Result<Vec<TempoAccount>> {
    let url = format!("{}/accounts", auth.base_url);
    // A large limit avoids paging — a tenant's account list is small and
    // the picker wants them all at once.
    let resp = client
        .get(&url)
        .bearer_auth(&auth.token)
        .query(&[("limit", "1000")])
        .send()
        .with_context(|| format!("tempo accounts at {url}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("tempo accounts: HTTP {} — {text}", status.as_u16());
    }
    let parsed: AccountsResponse = serde_json::from_str(&text)
        .with_context(|| format!("decode tempo accounts response: {text}"))?;
    Ok(parsed
        .results
        .into_iter()
        .map(|a| TempoAccount {
            id: a.id,
            key: a.key,
            name: a.name,
            customer: a.customer.and_then(|c| c.name),
        })
        .collect())
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
        // Seed a synthetic issue_id so resolve_issue_id can find one
        // without needing real Jira credentials in tests. Anything
        // non-empty works — sync only passes it through to the Tempo
        // payload, where the mocked endpoint doesn't validate.
        if let Some(key) = jira_issue {
            repo::set_ticket_issue_id(conn, key, "10000").unwrap();
        }
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
    fn round_to_half_hour_rounds_to_nearest_with_zero_floor() {
        // Exact multiples are untouched.
        assert_eq!(round_to_half_hour(0), 0);
        assert_eq!(round_to_half_hour(1800), 1800);
        assert_eq!(round_to_half_hour(5400), 5400);
        // Under 15 min → 0 (below the half-hour minimum).
        assert_eq!(round_to_half_hour(1), 0);
        assert_eq!(round_to_half_hour(14 * 60), 0);
        // The 15-min tie rounds up to 0.5h.
        assert_eq!(round_to_half_hour(15 * 60), 1800);
        assert_eq!(round_to_half_hour(26 * 60 + 40), 1800);
        // 44 min → 0.5h, 45 min (tie) → 1h.
        assert_eq!(round_to_half_hour(44 * 60), 1800);
        assert_eq!(round_to_half_hour(45 * 60), 3600);
        // Negatives clamp to 0 rather than going negative.
        assert_eq!(round_to_half_hour(-100), 0);
    }

    #[test]
    fn list_accounts_maps_results_and_customer() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/accounts");
            then.status(200).json_body(json!({
                "results": [
                    { "id": 42, "key": "ACME", "name": "Acme Co",
                      "customer": { "name": "Acme Corporation" } },
                    { "id": 7, "key": "INTERNAL", "name": "Internal" }
                ]
            }));
        });
        let accounts =
            list_accounts_with(&auth(server.base_url()), &http::client().unwrap()).unwrap();
        mock.assert();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].id, 42);
        assert_eq!(accounts[0].key, "ACME");
        assert_eq!(accounts[0].customer.as_deref(), Some("Acme Corporation"));
        assert_eq!(accounts[1].id, 7);
        assert_eq!(accounts[1].customer, None);
    }

    #[test]
    fn aggregated_post_rounds_duration_to_half_hour() {
        // 600s + 1000s = 1600s (26m40s) → one POST rounded up to 1800s.
        let server = MockServer::start();
        let post_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/worklogs")
                .json_body_partial(r#"{"timeSpentSeconds": 1800}"#);
            then.status(200).json_body(json!({"tempoWorklogId": 7777}));
        });
        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:10:00Z",
            600,
            Some("PROJ-1"),
            Some("first chunk"),
        );
        insert_block(
            &conn,
            day_s,
            "2026-04-18T09:10:00Z",
            "2026-04-18T09:27:00Z",
            1000,
            Some("PROJ-1"),
            Some("second chunk"),
        );
        let (report, _results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        post_mock.assert();
        assert_eq!(report.errors, Vec::<String>::new());
        assert!(report.synced >= 1, "the rounded group should sync");
    }

    #[test]
    fn aggregated_skips_group_under_fifteen_minutes() {
        // A 10-minute ticket-day rounds to 0 — no POST is made (the mock
        // would error if hit) and the block is reported as skipped.
        let server = MockServer::start();
        let _never = server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            then.status(500).body("must not be called");
        });
        let conn = open_memory().unwrap();
        insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:10:00Z",
            600,
            Some("PROJ-1"),
            Some("tiny"),
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
        assert_eq!(report.skipped, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
        assert!(results[0].reason.as_deref().unwrap().contains("0.5h"));
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
    fn sync_writes_integer_tempo_worklog_id_from_response() {
        // The happy path: Tempo returns a numeric id. Must be persisted
        // as a non-empty string. Previously only the negative case
        // (empty/null) was tested — if Value::Number handling broke,
        // no test would catch it.
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs");
            then.status(200).body(r#"{"tempoWorklogId": 42}"#);
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
        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.synced, 1);
        assert_eq!(results[0].status, "synced");
        assert_eq!(results[0].tempo_id.as_deref(), Some("42"));
        let stored: Option<String> = conn
            .query_row(
                "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
                params![bid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), Some("42"));
    }

    #[test]
    fn normalise_tempo_id_accepts_integer_and_string() {
        use serde_json::json;
        assert_eq!(normalise_tempo_id(&json!(42)).as_deref(), Some("42"));
        assert_eq!(normalise_tempo_id(&json!(0)).as_deref(), Some("0"));
        assert_eq!(normalise_tempo_id(&json!("tw-7")).as_deref(), Some("tw-7"));
        assert_eq!(normalise_tempo_id(&json!("  42  ")).as_deref(), Some("42"));
    }

    #[test]
    fn normalise_tempo_id_rejects_garbage() {
        use serde_json::json;
        assert_eq!(normalise_tempo_id(&json!(null)), None);
        assert_eq!(normalise_tempo_id(&json!("")), None);
        assert_eq!(normalise_tempo_id(&json!("null")), None);
        assert_eq!(normalise_tempo_id(&json!("NULL")), None);
        assert_eq!(normalise_tempo_id(&json!({"id": 7})), None);
        assert_eq!(normalise_tempo_id(&json!([42])), None);
        assert_eq!(normalise_tempo_id(&json!(true)), None);
    }

    #[test]
    fn sync_excludes_personal_blocks() {
        // A personal block must never reach Tempo. No mock is set, so any
        // POST attempt would fail loudly.
        let server = MockServer::start();
        let conn = open_memory().unwrap();
        let personal_id = insert_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("hacking on dotfiles"),
        );
        conn.execute(
            "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
            params![personal_id],
        )
        .unwrap();

        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();
        assert_eq!(report.synced, 0);
        assert_eq!(report.skipped, 0, "personal blocks aren't even listed");
        assert!(
            results.is_empty(),
            "personal block must not appear in results"
        );
    }

    /// Helper for the new aggregation tests — counts mocked endpoint
    /// hits without forcing every test to track its own `Arc<AtomicU32>`.
    fn count_blocks_with_tempo(conn: &Connection, day: &str, issue: &str, tempo_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM blocks
              WHERE day = ?1 AND jira_issue = ?2 AND tempo_worklog_id = ?3 AND dirty = 0",
            params![day, issue, tempo_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn aggregated_post_sums_durations_and_writes_shared_id_to_all_blocks() {
        // Three unsynced blocks on the same ticket → ONE POST with the
        // summed duration + earliest start; all three blocks end up
        // sharing the returned tempo_id with dirty=0.
        let server = MockServer::start();
        let post_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/worklogs")
                .json_body_partial(r#"{"timeSpentSeconds": 5400, "startTime": "09:00:00"}"#);
            then.status(200).json_body(json!({"tempoWorklogId": 9001}));
        });

        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        let b1 = insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        let b2 = insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        let b3 = insert_block(
            &conn,
            day_s,
            "2026-04-18T11:00:00Z",
            "2026-04-18T11:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );

        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();

        post_mock.assert_hits(1);
        assert_eq!(report.synced, 1, "one group → one synced count");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].status, "synced");
        assert_eq!(results[1].status, "synced-aggregated");
        assert_eq!(results[2].status, "synced-aggregated");
        for r in &results {
            assert_eq!(r.tempo_id.as_deref(), Some("9001"));
        }

        // All three blocks share the new id and are clean.
        for bid in [b1, b2, b3] {
            let (tid, dirty): (Option<String>, i64) = conn
                .query_row(
                    "SELECT tempo_worklog_id, dirty FROM blocks WHERE id = ?1",
                    params![bid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(tid.as_deref(), Some("9001"));
            assert_eq!(dirty, 0);
        }
    }

    #[test]
    fn aggregated_put_updates_shared_group_when_new_block_added() {
        // Group was previously aggregated (two synced blocks sharing
        // tempo_id=42). User adds a third block on the same ticket.
        // Re-sync must PUT /worklogs/42 with the new total (sum of
        // ALL THREE durations) and copy the shared id onto the new
        // block.
        let server = MockServer::start();
        let put_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/worklogs/42")
                .json_body_partial(r#"{"timeSpentSeconds": 5400}"#);
            then.status(200).json_body(json!({"tempoWorklogId": 42}));
        });

        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        // Two pre-synced blocks sharing the same tempo id.
        let b1 = insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        let b2 = insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '42', dirty = 0 WHERE id IN (?1, ?2)",
            params![b1, b2],
        )
        .unwrap();
        // New unsynced third block on the same ticket.
        let b3 = insert_block(
            &conn,
            day_s,
            "2026-04-18T11:00:00Z",
            "2026-04-18T11:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );

        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();

        put_mock.assert_hits(1);
        assert_eq!(report.synced, 1);
        // Only the third block was eligible → only it shows in results.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].block_id, b3);
        assert_eq!(results[0].status, "updated");
        assert_eq!(results[0].tempo_id.as_deref(), Some("42"));
        // All three blocks now share id=42.
        assert_eq!(count_blocks_with_tempo(&conn, day_s, "PROJ-1", "42"), 3);
    }

    #[test]
    fn mixed_legacy_tempo_ids_fall_back_to_per_block_sync() {
        // Two pre-synced blocks with DIFFERENT tempo ids (the legacy
        // one-Tempo-entry-per-block shape) plus one dirty block on the
        // same ticket. The aggregator must NOT merge them — it falls
        // back to per-block sync (PUT each dirty/unsynced individually).
        let server = MockServer::start();
        let put_77 = server.mock(|when, then| {
            when.method(PUT).path("/worklogs/77");
            then.status(200).json_body(json!({"tempoWorklogId": 77}));
        });

        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        let b1 = insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("x"),
        );
        let b2 = insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("y"),
        );
        // Distinct tempo ids (legacy one-per-block sync).
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '55', dirty = 0 WHERE id = ?1",
            params![b1],
        )
        .unwrap();
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '77', dirty = 1 WHERE id = ?1",
            params![b2],
        )
        .unwrap();

        let (report, results) = sync_day_with(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
        )
        .unwrap();

        // Only b2 was dirty → exactly one PUT to its own id, no POST,
        // no merge, b1 untouched.
        put_77.assert_hits(1);
        assert_eq!(report.synced, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].block_id, b2);
        assert_eq!(results[0].status, "updated");
        let b1_id: Option<String> = conn
            .query_row(
                "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
                params![b1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(b1_id.as_deref(), Some("55"), "legacy block id untouched");
    }

    #[test]
    fn aggregated_dry_run_never_invokes_claude_or_network() {
        // Dry-run on a multi-block group: no POST/PUT, no invoker call,
        // every block produces a payload-bearing dry-run result.
        let server = MockServer::start();
        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Review API spec"),
        );

        // FixedInvoker would panic the test if called — we'd see its
        // payload in the description. Instead it shouldn't be invoked
        // at all on dry-run, and the joined fallback should appear.
        let invoker = estimate::FixedInvoker(json!({"description": "DO NOT USE THIS"}));
        let (report, results) = sync_day_with_invoker(
            &conn,
            &auth(server.base_url()),
            day(),
            true,
            &http::client().unwrap(),
            Some(&invoker),
            estimate::DEFAULT_MODEL,
        )
        .unwrap();

        assert_eq!(report.synced, 0);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, "dry-run");
        assert_eq!(results[1].status, "dry-run-aggregated");
        for r in &results {
            let payload = r.payload.as_ref().unwrap();
            let desc = payload["description"].as_str().unwrap();
            assert!(
                desc.contains("Implement OAuth refresh") && desc.contains("Review API spec"),
                "dry-run uses joined fallback, not the invoker — got `{desc}`"
            );
            assert_eq!(payload["timeSpentSeconds"], 3600);
        }
    }

    #[test]
    fn single_description_skips_invoker() {
        // When the group collapses to one distinct description after
        // dedup, we don't need to ask Claude — pass it through verbatim.
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/worklogs")
                .json_body_partial(r#"{"description": "Implement OAuth refresh"}"#);
            then.status(200).json_body(json!({"tempoWorklogId": 1}));
        });
        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );

        // Use a panicking invoker to prove we don't call it.
        struct PanicInvoker;
        impl ModelInvoker for PanicInvoker {
            fn invoke(
                &self,
                _: &str,
                _: &str,
                _: &serde_json::Value,
                _: &str,
            ) -> anyhow::Result<serde_json::Value> {
                panic!("invoker must not be called for a single distinct description");
            }
        }

        let (report, _) = sync_day_with_invoker(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
            Some(&PanicInvoker),
            estimate::DEFAULT_MODEL,
        )
        .unwrap();
        assert_eq!(report.synced, 1);
    }

    #[test]
    fn multi_descriptions_use_invoker_summary() {
        // Two distinct descriptions → invoker is called and its
        // `description` field ends up in the Tempo POST body.
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/worklogs").json_body_partial(
                r#"{"description": "Implement OAuth refresh and review API spec"}"#,
            );
            then.status(200).json_body(json!({"tempoWorklogId": 1}));
        });
        let conn = open_memory().unwrap();
        let day_s = "2026-04-18";
        insert_block(
            &conn,
            day_s,
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Implement OAuth refresh"),
        );
        insert_block(
            &conn,
            day_s,
            "2026-04-18T10:00:00Z",
            "2026-04-18T10:30:00Z",
            1800,
            Some("PROJ-1"),
            Some("Review API spec"),
        );

        let invoker = estimate::FixedInvoker(json!({
            "description": "Implement OAuth refresh and review API spec"
        }));
        let (report, _) = sync_day_with_invoker(
            &conn,
            &auth(server.base_url()),
            day(),
            false,
            &http::client().unwrap(),
            Some(&invoker),
            estimate::DEFAULT_MODEL,
        )
        .unwrap();
        assert_eq!(report.synced, 1);
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
