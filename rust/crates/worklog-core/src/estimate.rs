//! Block estimator — invokes `claude -p --output-format json --json-schema
//! <schema>` to fill `jira_issue` + `minutes` + `description` for every block
//! on a given day that hasn't been estimated yet.
//!
//! Design constraints:
//! * Ticket selection is hard-validated — Claude may only pick keys that
//!   appeared in the candidate cache OR were literal matches in event
//!   content. Anything else is treated as a hallucination and dropped.
//! * Any hard failure → `estimated_by = 'gap'` so the UI can surface it.
//! * `estimated_by = 'manual'` blocks are skipped unconditionally — a
//!   user's override is the ground truth.

use std::process::Command;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, warn};

pub const DEFAULT_MODEL: &str = "claude-haiku-4-5";
const ROUND_MINUTES: i64 = 15;
const SUBPROCESS_TIMEOUT_SECS: u64 = 60;

pub const SYSTEM_PROMPT: &str = "You are a Jira/Tempo worklog assistant. Given a JSON array of work events that\nhappened inside one contiguous time block, plus a candidate list of the user's\nopen Jira tickets, produce exactly one Tempo worklog entry.\n\nRules:\n- jira_issue: pick a candidate ticket when the block's `project_name` or\n  event content clearly maps to one of the candidate ticket summaries.\n  Match on MEANING, not just literal strings: ticket summaries are often\n  in Icelandic while project paths/repos are in English (e.g.\n  `sjukra` ↔ a ticket mentioning \"Sjúkra\"; `pdf-flipbook` /\n  `flipbook-generator` ↔ a ticket mentioning \"flettibók\"; `agent` /\n  `chatbot` ↔ \"spjallmenni\"). If a candidate ticket plausibly describes\n  the same product/feature/repo as the events, prefer it. Return null only\n  when:\n    * the work is generic infra / CLI / dotfiles / worklog tooling / build\n      tweaks that doesn't belong to any product ticket;\n    * the events span multiple unrelated tickets with no clear majority;\n    * you'd be guessing between several mediocre matches.\n  Wrong tickets are worse than no ticket — never pick the \"closest\" of\n  several weak matches. You may also pick a key from literal_matches\n  (keys that appeared verbatim in event content) but only if those events\n  dominate the block.\n- description: Jira-style imperative (e.g. \"Implement OAuth token refresh\",\n  \"Review PR for billing module\"). Avoid first-person (\"I\", \"we\"). For\n  meetings, \"Attend <topic> sync\".\n- minutes: prefer block_duration_minutes; only deviate if the events clearly\n  don't fill the block (e.g. a single 2-min commit in a 60-min gap). Round to\n  the nearest 15.\n- Output ONLY a JSON object matching the schema. No prose, no code fences.\n";

/// Output schema the model must produce. Identical to the Python version.
pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "jira_issue": {
                "type": ["string", "null"],
                "description": "Jira issue key to log against. MUST be chosen from candidate_tickets OR from literal_matches. Use null if neither list is confident enough."
            },
            "minutes": {
                "type": "integer",
                "description": "Estimated duration in minutes."
            },
            "description": {
                "type": "string",
                "description": "Tempo worklog description in Jira imperative style, max 120 chars."
            }
        },
        "required": ["jira_issue", "minutes", "description"],
        "additionalProperties": false
    })
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EstimateStats {
    pub estimated: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Deserialize)]
struct Reply {
    jira_issue: Option<String>,
    minutes: Option<i64>,
    description: Option<String>,
}

/// Invoke the estimator for every un-estimated block on `day`.
pub fn estimate_day(conn: &Connection, day: NaiveDate, model: &str) -> Result<EstimateStats> {
    estimate_day_with(conn, day, model, &ClaudeSubprocess)
}

/// Test seam — tests pass a fake invoker so we don't shell out to `claude`.
pub trait ModelInvoker {
    fn invoke(&self, system: &str, user: &str, schema: &Value, model: &str) -> Result<Value>;
}

pub struct ClaudeSubprocess;

impl ModelInvoker for ClaudeSubprocess {
    fn invoke(&self, system: &str, user: &str, schema: &Value, model: &str) -> Result<Value> {
        let schema_str = serde_json::to_string(schema)?;
        let mut cmd = Command::new("claude");
        cmd.args([
            "-p",
            "--model",
            model,
            "--output-format",
            "json",
            "--json-schema",
            &schema_str,
            "--system-prompt",
            system,
        ]);
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("spawning `claude`")?;

        // Write prompt, then close stdin so the process can finish.
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(user.as_bytes())?;
        }

        // Simple wall-clock timeout via a thread (claude -p is fast on
        // haiku; 60s is generous). If it hangs, kill.
        let wait_start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    use std::io::Read;
                    if let Some(mut o) = child.stdout.take() {
                        o.read_to_string(&mut stdout).ok();
                    }
                    if let Some(mut e) = child.stderr.take() {
                        e.read_to_string(&mut stderr).ok();
                    }
                    if !status.success() {
                        anyhow::bail!(
                            "claude -p exited {} — {}",
                            status.code().unwrap_or(-1),
                            stderr.chars().take(500).collect::<String>()
                        );
                    }
                    return parse_response(&stdout);
                }
                None => {
                    if wait_start.elapsed().as_secs() > SUBPROCESS_TIMEOUT_SECS {
                        let _ = child.kill();
                        anyhow::bail!("claude -p timed out after {SUBPROCESS_TIMEOUT_SECS}s");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }
}

/// Shared test impl: feeds a canned JSON string back. Mirrors the shape
/// `claude -p --output-format json` returns (envelope with `result`).
#[cfg(test)]
pub struct FixedInvoker(pub Value);

#[cfg(test)]
impl ModelInvoker for FixedInvoker {
    fn invoke(&self, _s: &str, _u: &str, _sc: &Value, _m: &str) -> Result<Value> {
        Ok(self.0.clone())
    }
}

pub fn estimate_day_with<I: ModelInvoker>(
    conn: &Connection,
    day: NaiveDate,
    model: &str,
    invoker: &I,
) -> Result<EstimateStats> {
    let mut stats = EstimateStats::default();
    let day_iso = day.to_string();

    let open_tickets = load_open_tickets(conn)?;
    let blocks = load_blocks_for_estimator(conn, &day_iso)?;

    for block in blocks {
        // Personal blocks don't get an estimate — they're never going to
        // Tempo, and burning a `claude -p` call to write a Jira-style
        // description for "fixed my dotfiles" is wasted spend. Note: we
        // do NOT set `estimated_by`, so if the user reclassifies the
        // block as work later, the next `worklog estimate` run still
        // processes it normally.
        if block.is_personal {
            stats.skipped += 1;
            continue;
        }
        // Skip blocks we already processed (claude_p) OR that the user
        // has hand-edited (manual). Overwriting `manual` would silently
        // destroy the user's work — CLAUDE.md calls this out explicitly.
        match block.estimated_by.as_deref() {
            Some("claude_p") | Some("manual") => {
                stats.skipped += 1;
                continue;
            }
            _ => {}
        }

        let events = load_block_events(conn, block.id)?;
        let literals = collect_literal_matches(&events);
        let user_msg = build_user_message(&block, &events, &open_tickets, &literals);

        let reply = match invoker.invoke(SYSTEM_PROMPT, &user_msg, &response_schema(), model) {
            Ok(v) => v,
            Err(e) => {
                warn!(block_id = block.id, error = %e, "claude invocation failed");
                mark_gap(conn, block.id)?;
                stats.failed += 1;
                continue;
            }
        };

        let parsed: Reply = match serde_json::from_value(reply.clone()) {
            Ok(r) => r,
            Err(e) => {
                warn!(block_id = block.id, error = %e, value = %reply, "bad reply shape");
                mark_gap(conn, block.id)?;
                stats.failed += 1;
                continue;
            }
        };

        let description = match parsed.description {
            Some(d) if !d.trim().is_empty() => d,
            _ => {
                warn!(block_id = block.id, "claude returned no description");
                mark_gap(conn, block.id)?;
                stats.failed += 1;
                continue;
            }
        };

        let minutes = parsed.minutes.unwrap_or_else(|| {
            // Fall back to block's own wall-clock duration.
            fallback_block_minutes(&block)
        });
        let minutes = round_up_minutes(minutes);

        let ticket_claim = parsed.jira_issue;
        let mut ticket = validate_ticket(ticket_claim.as_deref(), &open_tickets, &literals);
        // Only carry the inferred ticket forward if it ALSO validates against
        // candidates/literals. Otherwise we'd resurrect regex noise like
        // `FINDING-01` (from pentest skill output) every time the model
        // correctly returned null. Trust null when the model picks null.
        if ticket.is_none() && block.jira_issue.is_some() {
            ticket = validate_ticket(block.jira_issue.as_deref(), &open_tickets, &literals);
        }

        conn.execute(
            "UPDATE blocks
                SET description      = ?1,
                    duration_seconds = ?2,
                    jira_issue       = ?3,
                    estimated_by     = 'claude_p'
              WHERE id = ?4",
            params![description, minutes * 60, ticket, block.id],
        )
        .context("updating block with estimate")?;
        stats.estimated += 1;
        debug!(block_id = block.id, "estimated by claude_p");
    }

    // After estimation, fold neighbouring blocks that landed on the same
    // ticket back together. project-aware splitting can fragment a single
    // ticket's work across multiple repos / cwds, but if the estimator
    // resolves them all to the same key the user wants one entry, not
    // five.
    let merged = merge_same_ticket_adjacent(conn, &day_iso)?;
    if merged > 0 {
        debug!(merged, "merged same-ticket adjacent blocks");
    }

    Ok(stats)
}

/// Merge runs of consecutive blocks that share a non-null jira_issue.
/// Returns the count of blocks removed by merging.
///
/// Safe-skips:
/// - blocks with `tempo_worklog_id` set (already synced — would orphan the
///   Tempo entry)
/// - blocks with `estimated_by = 'manual'` (user hand-edited; merging
///   would silently change their work)
pub fn merge_same_ticket_adjacent(conn: &Connection, day_iso: &str) -> Result<u32> {
    let blocks = load_blocks_for_estimator(conn, day_iso)?;
    let mut removed = 0;
    let mut i = 0;
    let mut blocks = blocks;
    while i + 1 < blocks.len() {
        let a = &blocks[i];
        let b = &blocks[i + 1];
        let same = match (a.jira_issue.as_deref(), b.jira_issue.as_deref()) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        };
        let safe = a.estimated_by.as_deref() != Some("manual")
            && b.estimated_by.as_deref() != Some("manual")
            && block_is_unsynced(conn, a.id)?
            && block_is_unsynced(conn, b.id)?;
        if same && safe {
            merge_block_into(conn, a.id, b.id)?;
            // remove b from local view and try merging again from same i
            blocks.remove(i + 1);
            removed += 1;
        } else {
            i += 1;
        }
    }
    Ok(removed)
}

fn block_is_unsynced(conn: &Connection, block_id: i64) -> Result<bool> {
    let tid: Option<String> = conn.query_row(
        "SELECT tempo_worklog_id FROM blocks WHERE id = ?1",
        params![block_id],
        |r| r.get(0),
    )?;
    // Tempo treats both "" and NULL as unsynced — see
    // tempo::normalise_tempo_id. Mirror that here.
    Ok(tid.as_deref().map(str::trim).unwrap_or("").is_empty())
}

/// Merge `src` into `dst`. After this call `src` no longer exists; all
/// its events are linked to `dst` and `dst`'s wall-clock + duration
/// covers both.
fn merge_block_into(conn: &Connection, dst: i64, src: i64) -> Result<()> {
    // Pick wider time range. ended_at is stored as RFC3339; lexical max
    // works because the prefix is fixed-width.
    let (dst_start, dst_end): (String, String) = conn.query_row(
        "SELECT started_at, ended_at FROM blocks WHERE id = ?1",
        params![dst],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (src_start, src_end): (String, String) = conn.query_row(
        "SELECT started_at, ended_at FROM blocks WHERE id = ?1",
        params![src],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let new_start = if src_start < dst_start {
        src_start
    } else {
        dst_start
    };
    let new_end = if src_end > dst_end { src_end } else { dst_end };
    let new_dur = duration_seconds_between(&new_start, &new_end);

    conn.execute(
        "UPDATE blocks SET started_at = ?1, ended_at = ?2, duration_seconds = ?3 WHERE id = ?4",
        params![new_start, new_end, new_dur, dst],
    )?;
    // Re-point junction rows. block_events uniqueness is per (block_id,
    // event_id) so we use INSERT OR IGNORE to handle any (theoretical)
    // overlap, then drop the src side.
    conn.execute(
        "INSERT OR IGNORE INTO block_events (block_id, event_id)
           SELECT ?1, event_id FROM block_events WHERE block_id = ?2",
        params![dst, src],
    )?;
    conn.execute("DELETE FROM block_events WHERE block_id = ?1", params![src])?;
    conn.execute("DELETE FROM blocks WHERE id = ?1", params![src])?;
    Ok(())
}

fn duration_seconds_between(start_iso: &str, end_iso: &str) -> i64 {
    let s: DateTime<Utc> = start_iso.parse().unwrap_or_else(|_| Utc::now());
    let e: DateTime<Utc> = end_iso.parse().unwrap_or_else(|_| Utc::now());
    (e - s).num_seconds().max(0)
}

// ───────────────────────── helpers ─────────────────────────

#[derive(Debug, Clone)]
struct Candidate {
    key: String,
    summary: String,
    status: Option<String>,
}

#[derive(Debug, Clone)]
struct BlockRow {
    id: i64,
    started_at: String,
    ended_at: String,
    jira_issue: Option<String>,
    estimated_by: Option<String>,
    is_personal: bool,
}

#[derive(Debug, Clone)]
struct EventRow {
    source: String,
    started_at: String,
    title: Option<String>,
    details: Option<String>,
    jira_issue: Option<String>,
    project_path: Option<String>,
}

fn load_open_tickets(conn: &Connection) -> Result<Vec<Candidate>> {
    let mut stmt =
        conn.prepare("SELECT key, summary, status FROM jira_tickets ORDER BY updated DESC")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Candidate {
                key: r.get(0)?,
                summary: r.get(1)?,
                status: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_blocks_for_estimator(conn: &Connection, day_iso: &str) -> Result<Vec<BlockRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, started_at, ended_at, jira_issue, estimated_by, is_personal
           FROM blocks WHERE day = ?1 ORDER BY started_at",
    )?;
    let rows = stmt
        .query_map(params![day_iso], |r| {
            Ok(BlockRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                jira_issue: r.get(3)?,
                estimated_by: r.get(4)?,
                is_personal: r.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_block_events(conn: &Connection, block_id: i64) -> Result<Vec<EventRow>> {
    let mut stmt = conn.prepare(
        "SELECT e.source, e.started_at, e.title, e.details, e.jira_issue, e.project_path
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1
          ORDER BY e.started_at",
    )?;
    let rows = stmt
        .query_map(params![block_id], |r| {
            Ok(EventRow {
                source: r.get(0)?,
                started_at: r.get(1)?,
                title: r.get(2)?,
                details: r.get(3)?,
                jira_issue: r.get(4)?,
                project_path: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Pick the project path that dominates the block. We send this to the
/// estimator so it can refuse to assign a ticket when the dominant repo
/// doesn't belong to any candidate's project — preventing "closest
/// matching ticket" mis-assignments across unrelated codebases.
fn dominant_project(events: &[EventRow]) -> Option<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for e in events {
        if let Some(p) = &e.project_path {
            *counts.entry(p.clone()).or_insert(0) += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p)
}

/// Last path segment is what humans recognise — `/Users/tomas/Desktop/Work/sjukra`
/// → `sjukra`. Sent alongside the full path so the model has both signals.
fn project_name(path: &str) -> &str {
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(path)
}

fn collect_literal_matches(events: &[EventRow]) -> Vec<String> {
    let re = Regex::new(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b").unwrap();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in events {
        for blob in [&e.title, &e.details] {
            let Some(blob) = blob else { continue };
            for m in re.find_iter(blob) {
                let key = m.as_str().to_owned();
                if seen.insert(key.clone()) {
                    out.push(key);
                }
            }
        }
    }
    out
}

/// Per-event details char cap in the payload sent to the estimator.
/// Claude Code events get a generous cap because `hook_run` now stores the
/// full user prompt there — that prompt *is* the description signal. Other
/// sources (gcal, github, jira) pass through a `details` blob that is
/// usually already short; keep the old bound so a chatty meeting
/// description doesn't blow up the token bill.
const DETAILS_CAP_CLAUDE: usize = 800;
const DETAILS_CAP_OTHER: usize = 200;

fn event_details_cap(source: &str) -> usize {
    if source == "claude" {
        DETAILS_CAP_CLAUDE
    } else {
        DETAILS_CAP_OTHER
    }
}

fn build_user_message(
    block: &BlockRow,
    events: &[EventRow],
    candidates: &[Candidate],
    literals: &[String],
) -> String {
    let started: DateTime<Utc> = block.started_at.parse().unwrap_or_else(|_| Utc::now());
    let ended: DateTime<Utc> = block.ended_at.parse().unwrap_or_else(|_| Utc::now());
    let duration_min = (ended - started).num_seconds() / 60;

    let dom = dominant_project(events);
    let payload = json!({
        "block_duration_minutes": duration_min,
        "inferred_jira_issue":    block.jira_issue,
        "project_path":           dom,
        "project_name":           dom.as_deref().map(project_name),
        "candidate_tickets":      candidates.iter().map(|c| json!({
            "key": c.key,
            "summary": c.summary,
            "status": c.status,
        })).collect::<Vec<_>>(),
        "literal_matches":        literals,
        "events":                 events.iter().map(|e| {
            let cap = event_details_cap(&e.source);
            json!({
                "type":       e.source,
                "timestamp":  e.started_at,
                "summary":    trunc(e.title.as_deref().unwrap_or(""), 200),
                "details":    e.details.as_deref().map(|d| trunc(d, cap)),
                "jira_issue": e.jira_issue,
                "project":    e.project_path.as_deref().map(project_name),
            })
        }).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into())
}

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn fallback_block_minutes(block: &BlockRow) -> i64 {
    let started: DateTime<Utc> = block.started_at.parse().unwrap_or_else(|_| Utc::now());
    let ended: DateTime<Utc> = block.ended_at.parse().unwrap_or_else(|_| Utc::now());
    ((ended - started).num_seconds() / 60).max(1)
}

fn round_up_minutes(m: i64) -> i64 {
    let m = m.max(1);
    ROUND_MINUTES * ((m + ROUND_MINUTES - 1) / ROUND_MINUTES)
}

fn validate_ticket(
    claimed: Option<&str>,
    candidates: &[Candidate],
    literals: &[String],
) -> Option<String> {
    let claimed = claimed?;
    if candidates.iter().any(|c| c.key == claimed) {
        return Some(claimed.to_owned());
    }
    if literals.iter().any(|l| l == claimed) {
        return Some(claimed.to_owned());
    }
    None
}

fn mark_gap(conn: &Connection, block_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE blocks SET estimated_by = 'gap' WHERE id = ?1",
        params![block_id],
    )?;
    Ok(())
}

/// Accept any of: `{"structured_output": {...}}` (current `claude -p
/// --json-schema` envelope — `result` is prose), raw JSON object,
/// `{"result": "<string json>"}` envelope, `{"result": {...}}` envelope,
/// or prose-wrapped JSON.
pub fn parse_response(raw: &str) -> Result<Value> {
    let raw = raw.trim();

    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        // Preferred: claude -p --json-schema now emits the schema-validated
        // object under `structured_output`. `result` is a prose summary.
        if let Some(so) = parsed.get("structured_output") {
            if so.is_object() {
                return Ok(so.clone());
            }
            if let Some(s) = so.as_str() {
                if let Ok(v) = serde_json::from_str::<Value>(s) {
                    return Ok(v);
                }
            }
        }
        if let Some(result) = parsed.get("result") {
            if let Some(s) = result.as_str() {
                if let Ok(v) = serde_json::from_str::<Value>(s) {
                    return Ok(v);
                }
                // result is prose (new CLI behavior) — fall through to
                // the regex extractor below in case it embeds JSON.
            }
            if result.is_object() {
                return Ok(result.clone());
            }
        }
        if parsed.is_object()
            && parsed.get("structured_output").is_none()
            && parsed.get("result").is_none()
        {
            return Ok(parsed);
        }
    }

    let re = Regex::new(r"(?s)\{.*\}").unwrap();
    if let Some(m) = re.find(raw) {
        return serde_json::from_str(m.as_str()).context("embedded JSON invalid");
    }
    anyhow::bail!("no JSON object in response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::{Event, JiraTicket};
    use crate::repo;

    fn insert_block(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', NULL, '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        ).unwrap();
        conn.last_insert_rowid()
    }

    fn link(conn: &Connection, block_id: i64, event_id: i64) {
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, event_id],
        )
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)] // test fixture
    fn insert_block_with(
        conn: &Connection,
        day: &str,
        started: &str,
        ended: &str,
        duration: i64,
        jira: Option<&str>,
        estimated_by: Option<&str>,
        tempo: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, estimated_by, tempo_worklog_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![day, jira, started, ended, duration, estimated_by, tempo],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn merge_combines_adjacent_blocks_with_same_ticket() {
        let conn = open_memory().unwrap();
        let a = insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:00:00+00:00",
            "2026-05-12T09:30:00+00:00",
            1800,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        let b = insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:31:00+00:00",
            "2026-05-12T10:00:00+00:00",
            1740,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        let removed = merge_same_ticket_adjacent(&conn, "2026-05-12").unwrap();
        assert_eq!(removed, 1);
        let remaining: Vec<(i64, String, String)> = conn
            .prepare("SELECT id, started_at, ended_at FROM blocks WHERE day='2026-05-12'")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0, a);
        assert_eq!(remaining[0].1, "2026-05-12T09:00:00+00:00");
        assert_eq!(remaining[0].2, "2026-05-12T10:00:00+00:00");
        let _ = b;
    }

    #[test]
    fn merge_leaves_different_tickets_alone() {
        let conn = open_memory().unwrap();
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:00:00+00:00",
            "2026-05-12T09:30:00+00:00",
            1800,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:31:00+00:00",
            "2026-05-12T10:00:00+00:00",
            1740,
            Some("GOJ-2"),
            Some("claude_p"),
            None,
        );
        let removed = merge_same_ticket_adjacent(&conn, "2026-05-12").unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn merge_skips_manual_blocks() {
        let conn = open_memory().unwrap();
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:00:00+00:00",
            "2026-05-12T09:30:00+00:00",
            1800,
            Some("GENAI-1"),
            Some("manual"),
            None,
        );
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:31:00+00:00",
            "2026-05-12T10:00:00+00:00",
            1740,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        let removed = merge_same_ticket_adjacent(&conn, "2026-05-12").unwrap();
        assert_eq!(removed, 0, "manual blocks must not be merged");
    }

    #[test]
    fn merge_skips_synced_blocks() {
        let conn = open_memory().unwrap();
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:00:00+00:00",
            "2026-05-12T09:30:00+00:00",
            1800,
            Some("GENAI-1"),
            Some("claude_p"),
            Some("12345"),
        );
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:31:00+00:00",
            "2026-05-12T10:00:00+00:00",
            1740,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        let removed = merge_same_ticket_adjacent(&conn, "2026-05-12").unwrap();
        assert_eq!(
            removed, 0,
            "synced (tempo_worklog_id set) blocks must not be merged"
        );
    }

    #[test]
    fn merge_chains_three_same_ticket_blocks() {
        let conn = open_memory().unwrap();
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:00:00+00:00",
            "2026-05-12T09:20:00+00:00",
            1200,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:25:00+00:00",
            "2026-05-12T09:45:00+00:00",
            1200,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        insert_block_with(
            &conn,
            "2026-05-12",
            "2026-05-12T09:50:00+00:00",
            "2026-05-12T10:10:00+00:00",
            1200,
            Some("GENAI-1"),
            Some("claude_p"),
            None,
        );
        let removed = merge_same_ticket_adjacent(&conn, "2026-05-12").unwrap();
        assert_eq!(removed, 2, "three same-ticket blocks collapse to one");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE day='2026-05-12'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn build_user_message_preserves_long_claude_details_but_trims_others() {
        // B4: events from the Claude Code hook now carry the full user
        // prompt (up to 4KiB). When we hand the payload to the estimator
        // we keep up to 800 chars of `details` for `source='claude'` so
        // Claude has substance to summarise from — but non-claude sources
        // stay at the 200-char cap so a chatty gcal description doesn't
        // blow up the token bill.
        let block = BlockRow {
            id: 1,
            started_at: "2026-04-18T09:00:00+00:00".into(),
            ended_at: "2026-04-18T09:30:00+00:00".into(),
            jira_issue: None,
            estimated_by: None,
            is_personal: false,
        };
        let claude_event = EventRow {
            source: "claude".into(),
            started_at: "2026-04-18T09:05:00+00:00".into(),
            title: Some("UserPromptSubmit — fix auth".into()),
            details: Some("c".repeat(500)),
            jira_issue: None,
            project_path: None,
        };
        let github_event = EventRow {
            source: "github_commit".into(),
            started_at: "2026-04-18T09:10:00+00:00".into(),
            title: Some("Initial commit".into()),
            details: Some("g".repeat(500)),
            jira_issue: None,
            project_path: None,
        };

        let msg = build_user_message(&block, &[claude_event, github_event], &[], &[]);
        let payload: Value = serde_json::from_str(&msg).unwrap();
        let events = payload["events"].as_array().unwrap();

        let claude_details = events[0]["details"].as_str().unwrap();
        assert_eq!(
            claude_details.chars().count(),
            500,
            "claude event should keep all 500 chars (cap is 800 for this source)"
        );

        let github_details = events[1]["details"].as_str().unwrap();
        assert_eq!(
            github_details.chars().count(),
            200,
            "non-claude events stay at the old 200-char cap"
        );
    }

    #[test]
    fn parse_response_handles_raw_object() {
        let v =
            parse_response(r#"{"jira_issue":"PROJ-1","minutes":30,"description":"x"}"#).unwrap();
        assert_eq!(v["jira_issue"], "PROJ-1");
    }

    #[test]
    fn parse_response_prefers_structured_output_over_prose_result() {
        // Current `claude -p --json-schema` envelope: `result` is a prose
        // summary, the schema-validated object lives in `structured_output`.
        // Regression: parser used to read `result` first and bail with
        // "envelope.result not JSON", marking every block as `gap`.
        let v = parse_response(
            r#"{
              "type": "result",
              "result": "Done. Worklog entry created for PROJ-1.",
              "structured_output": {
                "jira_issue": "PROJ-1",
                "minutes": 30,
                "description": "x"
              }
            }"#,
        )
        .unwrap();
        assert_eq!(v["jira_issue"], "PROJ-1");
        assert_eq!(v["minutes"], 30);
    }

    #[test]
    fn parse_response_handles_envelope_with_string() {
        let v = parse_response(
            r#"{"result":"{\"jira_issue\":\"X-1\",\"minutes\":5,\"description\":\"x\"}"}"#,
        )
        .unwrap();
        assert_eq!(v["jira_issue"], "X-1");
    }

    #[test]
    fn parse_response_handles_envelope_with_object() {
        let v = parse_response(
            r#"{"result": {"jira_issue": "Z-9", "minutes": 15, "description": "x"}}"#,
        )
        .unwrap();
        assert_eq!(v["jira_issue"], "Z-9");
    }

    #[test]
    fn parse_response_handles_prose_wrapped_json() {
        let v = parse_response(
            "Here you go: {\"jira_issue\": \"P-1\", \"minutes\": 5, \"description\": \"hi\"}",
        )
        .unwrap();
        assert_eq!(v["jira_issue"], "P-1");
    }

    #[test]
    fn round_up_minutes_rounds_to_nearest_15() {
        assert_eq!(round_up_minutes(1), 15);
        assert_eq!(round_up_minutes(15), 15);
        assert_eq!(round_up_minutes(16), 30);
        assert_eq!(round_up_minutes(30), 30);
        assert_eq!(round_up_minutes(31), 45);
    }

    #[test]
    fn validate_ticket_accepts_candidates_only() {
        let candidates = vec![Candidate {
            key: "PROJ-1".into(),
            summary: "x".into(),
            status: None,
        }];
        let literals = vec!["OTHER-2".to_string()];
        assert_eq!(
            validate_ticket(Some("PROJ-1"), &candidates, &literals).as_deref(),
            Some("PROJ-1")
        );
        assert_eq!(
            validate_ticket(Some("OTHER-2"), &candidates, &literals).as_deref(),
            Some("OTHER-2")
        );
        // Hallucinated key — must be rejected.
        assert_eq!(
            validate_ticket(Some("FAKE-99"), &candidates, &literals),
            None
        );
        assert_eq!(validate_ticket(None, &candidates, &literals), None);
    }

    #[test]
    fn estimate_updates_block_on_success() {
        let conn = open_memory().unwrap();
        repo::upsert_ticket(
            &conn,
            &JiraTicket {
                key: "PROJ-1".into(),
                summary: "fix thing".into(),
                status: Some("In Progress".into()),
                project_key: Some("PROJ".into()),
                updated: None,
                issue_id: None,
            },
        )
        .unwrap();
        let eid = repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "abc",
                "2026-04-18T09:05:00+00:00",
                "commit",
            ),
        )
        .unwrap();
        let bid = insert_block(&conn);
        link(&conn, bid, eid);

        let invoker = FixedInvoker(json!({
            "jira_issue": "PROJ-1",
            "minutes": 30,
            "description": "Implement auth refresh"
        }));
        let stats = estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "test-model",
            &invoker,
        )
        .unwrap();
        assert_eq!(stats.estimated, 1);

        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert_eq!(block.description.as_deref(), Some("Implement auth refresh"));
        assert_eq!(block.jira_issue.as_deref(), Some("PROJ-1"));
        assert_eq!(block.estimated_by.as_deref(), Some("claude_p"));
        assert_eq!(block.duration_seconds, 30 * 60);
    }

    #[test]
    fn estimate_skips_manual_blocks() {
        // The user has already hand-edited this block (set description or
        // duration, which flips estimated_by = 'manual'). Re-running the
        // estimator MUST NOT overwrite their work.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description, estimated_by)
             VALUES ('2026-04-18', 'PROJ-7', '2026-04-18T10:00:00+00:00', '2026-04-18T10:30:00+00:00', 1800, 'user typed this', 'manual')",
            [],
        ).unwrap();
        let bid = conn.last_insert_rowid();

        // If the estimator WERE to call out, it would try to overwrite.
        // This invoker would clobber both jira_issue and description.
        let invoker = FixedInvoker(json!({
            "jira_issue": "PROJ-9",
            "minutes": 90,
            "description": "AI-rewritten description"
        }));
        let stats = estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        assert_eq!(stats.skipped, 1, "manual block must be skipped");
        assert_eq!(stats.estimated, 0);

        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert_eq!(block.jira_issue.as_deref(), Some("PROJ-7"));
        assert_eq!(block.description.as_deref(), Some("user typed this"));
        assert_eq!(block.estimated_by.as_deref(), Some("manual"));
        assert_eq!(block.duration_seconds, 1800);
    }

    #[test]
    fn estimate_marks_gap_on_no_description() {
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn);
        let invoker = FixedInvoker(json!({
            "jira_issue": null,
            "minutes": 15,
            "description": ""
        }));
        let stats = estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        assert_eq!(stats.failed, 1);
        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert_eq!(block.estimated_by.as_deref(), Some("gap"));
    }

    #[test]
    fn estimate_rejects_hallucinated_ticket() {
        let conn = open_memory().unwrap();
        let bid = insert_block(&conn);
        let invoker = FixedInvoker(json!({
            "jira_issue": "MADEUP-1",
            "minutes": 15,
            "description": "work"
        }));
        estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert!(
            block.jira_issue.is_none(),
            "hallucinated key must be dropped"
        );
        assert_eq!(block.estimated_by.as_deref(), Some("claude_p"));
        assert_eq!(block.description.as_deref(), Some("work"));
    }

    #[test]
    fn estimate_skips_personal_blocks() {
        // Personal blocks are skipped without invoking the model and
        // without flipping `estimated_by` (so a later reclassify can
        // bring them back into the work flow).
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, is_personal)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 1)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let invoker = FixedInvoker(json!({
            "jira_issue": "SHOULD-NOT-USE",
            "minutes": 30,
            "description": "should-not-run"
        }));
        let stats = estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        assert_eq!(stats.estimated, 0);
        assert_eq!(stats.skipped, 1);
        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert!(block.description.is_none(), "must not write description");
        assert!(
            block.estimated_by.is_none(),
            "must not stamp estimated_by — leaves reclassify path open"
        );
    }

    #[test]
    fn estimate_drops_inferred_ticket_when_not_in_candidates() {
        // If the regex-inferred ticket on the block doesn't match any
        // real Jira project (e.g. `FINDING-01` from /pentest output or
        // `CVE-2025` from a security skill), and Claude correctly returns
        // null, we must NOT resurrect the noise. Trust null.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', 'FINDING-01', '2026-04-18T10:00:00+00:00', '2026-04-18T10:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let invoker = FixedInvoker(json!({
            "jira_issue": null,
            "minutes": 30,
            "description": "Work"
        }));
        estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert_eq!(
            block.jira_issue, None,
            "noise key from inference must not survive null estimate"
        );
    }

    #[test]
    fn estimate_keeps_inferred_ticket_when_it_is_a_real_candidate() {
        // If the inferred ticket DOES match a real Jira candidate (e.g.
        // a GENAI-* key cached from the jira collector) and Claude
        // returns null, preserve the inference — the model just wasn't
        // confident enough to commit, but the signal is valid.
        let conn = open_memory().unwrap();
        repo::upsert_ticket(
            &conn,
            &JiraTicket {
                key: "GENAI-1".into(),
                summary: "Real ticket".into(),
                status: Some("In Progress".into()),
                project_key: Some("GENAI".into()),
                updated: Some("2026-04-18T00:00:00Z".into()),
                issue_id: None,
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', 'GENAI-1', '2026-04-18T10:00:00+00:00', '2026-04-18T10:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let invoker = FixedInvoker(json!({
            "jira_issue": null,
            "minutes": 30,
            "description": "Work"
        }));
        estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        let block = repo::get_block(&conn, bid).unwrap().unwrap();
        assert_eq!(block.jira_issue.as_deref(), Some("GENAI-1"));
    }

    #[test]
    fn estimate_skips_already_estimated_blocks() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, estimated_by)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 'claude_p')",
            [],
        )
        .unwrap();
        let invoker =
            FixedInvoker(json!({"jira_issue":null,"minutes":1,"description":"should not run"}));
        let stats = estimate_day_with(
            &conn,
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            "m",
            &invoker,
        )
        .unwrap();
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.estimated, 0);
    }

    #[test]
    fn collect_literal_matches_dedupes_keys_across_events() {
        let events = vec![
            EventRow {
                source: "github_commit".into(),
                started_at: "2026-04-18T09:00:00+00:00".into(),
                title: Some("PROJ-1 fix".into()),
                details: None,
                jira_issue: None,
                project_path: None,
            },
            EventRow {
                source: "github_pr".into(),
                started_at: "2026-04-18T09:10:00+00:00".into(),
                title: None,
                details: Some("see PROJ-1 and PROJ-2".into()),
                jira_issue: None,
                project_path: None,
            },
        ];
        let got = collect_literal_matches(&events);
        assert_eq!(got, vec!["PROJ-1".to_string(), "PROJ-2".to_string()]);
    }
}
