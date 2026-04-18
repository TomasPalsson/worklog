//! Block estimator — invokes `claude -p --output-format json --json-schema
//! <schema>` to fill `jira_issue` + `minutes` + `description` for every block
//! on a given day that hasn't been estimated yet.
//!
//! Ports `src/worklog/estimate.py` faithfully:
//! * Schema and system prompt are byte-for-byte the Python version so the
//!   model sees the same instruction regardless of caller.
//! * Ticket selection is hard-validated — Claude may only pick keys that
//!   appeared in the candidate cache OR were literal matches in event
//!   content. Anything else is treated as a hallucination and dropped.
//! * Any hard failure → `estimated_by = 'gap'` so the UI can surface it.

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

pub const SYSTEM_PROMPT: &str = "You are a Jira/Tempo worklog assistant. Given a JSON array of work events that\nhappened inside one contiguous time block, plus a candidate list of the user's\nopen Jira tickets, produce exactly one Tempo worklog entry.\n\nRules:\n- jira_issue: pick the single best matching ticket from candidate_tickets. You\n  may also pick a key from literal_matches (keys that appeared verbatim in\n  event content — e.g. in a commit message or branch name). If NEITHER list\n  gives you a confident match, return null. Never invent a key.\n- description: Jira-style imperative (e.g. \"Implement OAuth token refresh\",\n  \"Review PR for billing module\"). Avoid first-person (\"I\", \"we\"). For\n  meetings, \"Attend <topic> sync\".\n- minutes: prefer block_duration_minutes; only deviate if the events clearly\n  don't fill the block (e.g. a single 2-min commit in a 60-min gap). Round to\n  the nearest 15.\n- Output ONLY a JSON object matching the schema. No prose, no code fences.\n";

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
        if block.estimated_by.as_deref() == Some("claude_p") {
            stats.skipped += 1;
            continue;
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
        if ticket.is_none() && block.jira_issue.is_some() {
            // Preserve inference if Claude didn't confidently pick.
            ticket = block.jira_issue.clone();
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
    Ok(stats)
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
}

#[derive(Debug, Clone)]
struct EventRow {
    source: String,
    started_at: String,
    title: Option<String>,
    details: Option<String>,
    jira_issue: Option<String>,
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
        "SELECT id, started_at, ended_at, jira_issue, estimated_by
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_block_events(conn: &Connection, block_id: i64) -> Result<Vec<EventRow>> {
    let mut stmt = conn.prepare(
        "SELECT e.source, e.started_at, e.title, e.details, e.jira_issue
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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

fn build_user_message(
    block: &BlockRow,
    events: &[EventRow],
    candidates: &[Candidate],
    literals: &[String],
) -> String {
    let started: DateTime<Utc> = block.started_at.parse().unwrap_or_else(|_| Utc::now());
    let ended: DateTime<Utc> = block.ended_at.parse().unwrap_or_else(|_| Utc::now());
    let duration_min = (ended - started).num_seconds() / 60;

    let payload = json!({
        "block_duration_minutes": duration_min,
        "inferred_jira_issue":    block.jira_issue,
        "candidate_tickets":      candidates.iter().map(|c| json!({
            "key": c.key,
            "summary": c.summary,
            "status": c.status,
        })).collect::<Vec<_>>(),
        "literal_matches":        literals,
        "events":                 events.iter().map(|e| json!({
            "type":       e.source,
            "timestamp":  e.started_at,
            "summary":    trunc(e.title.as_deref().unwrap_or(""), 200),
            "details":    e.details.as_deref().map(|d| trunc(d, 200)),
            "jira_issue": e.jira_issue,
        })).collect::<Vec<_>>(),
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

/// Accept any of: raw JSON object, `{"result": "<string json>"}` envelope,
/// `{"result": {...}}` envelope, or prose-wrapped JSON. Matches Python.
pub fn parse_response(raw: &str) -> Result<Value> {
    let raw = raw.trim();

    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        if let Some(result) = parsed.get("result") {
            if let Some(s) = result.as_str() {
                return serde_json::from_str(s).context("envelope.result not JSON");
            }
            if result.is_object() {
                return Ok(result.clone());
            }
        }
        if parsed.is_object() {
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

    #[test]
    fn parse_response_handles_raw_object() {
        let v =
            parse_response(r#"{"jira_issue":"PROJ-1","minutes":30,"description":"x"}"#).unwrap();
        assert_eq!(v["jira_issue"], "PROJ-1");
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
    fn estimate_preserves_inferred_ticket_when_claude_is_unsure() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', 'INFER-1', '2026-04-18T10:00:00+00:00', '2026-04-18T10:30:00+00:00', 1800)",
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
        assert_eq!(block.jira_issue.as_deref(), Some("INFER-1"));
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
            },
            EventRow {
                source: "github_pr".into(),
                started_at: "2026-04-18T09:10:00+00:00".into(),
                title: None,
                details: Some("see PROJ-1 and PROJ-2".into()),
                jira_issue: None,
            },
        ];
        let got = collect_literal_matches(&events);
        assert_eq!(got, vec!["PROJ-1".to_string(), "PROJ-2".to_string()]);
    }
}
