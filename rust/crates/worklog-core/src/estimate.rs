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

/// Which invoker a given day's estimate run will route through. Built
/// by [`resolve_provider`] from env + secrets. Kept as an enum (not a
/// boxed trait object) so tests can pattern-match without a downcast
/// and the compiler proves every arm is handled at the dispatch site.
pub enum ProviderChoice {
    /// The historical `claude -p` subprocess path. Default when nothing
    /// is configured — existing installs keep working unchanged.
    ClaudeSubprocess,
    /// LiteLLM / any OpenAI-compatible HTTP proxy.
    LiteLLM(LiteLLMInvoker),
}

impl std::fmt::Debug for ProviderChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't leak endpoint/model fields via default Derive — debug
        // output lands in test panic messages + tracing events.
        match self {
            ProviderChoice::ClaudeSubprocess => f.write_str("ClaudeSubprocess"),
            ProviderChoice::LiteLLM(_) => f.write_str("LiteLLM(<invoker>)"),
        }
    }
}

/// Reject empty-or-whitespace values so "secret exists but blank"
/// behaves like "secret absent". Keyring + `.env` fallback both admit
/// empty strings and we want a single rule everywhere.
fn read_trimmed_secret(key: &str) -> Result<Option<String>> {
    Ok(crate::secrets::get(key)?
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty()))
}

/// Env wins over secret. Returns the trimmed lowercase choice or None
/// when neither is set; caller defaults to `claude_subprocess`.
fn read_provider_selector() -> Result<Option<String>> {
    if let Some(v) = std::env::var("WORKLOG_ESTIMATOR_PROVIDER")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
    {
        return Ok(Some(v.to_lowercase()));
    }
    Ok(read_trimmed_secret("worklog_estimator_provider")?.map(|s| s.to_lowercase()))
}

/// Build a `LiteLLMInvoker` from secrets, with actionable errors when
/// required pieces are missing. Only `litellm_base_url` is required;
/// `api_key` can be empty (unauthed local proxies) and `model` falls
/// back to [`DEFAULT_LITELLM_MODEL`].
fn build_litellm_from_secrets() -> Result<LiteLLMInvoker> {
    let base_url = read_trimmed_secret("litellm_base_url")?.ok_or_else(|| {
        anyhow::anyhow!(
            "estimator provider `litellm` selected, but `litellm_base_url` is not set. \
             Run `worklog setup` or `worklog secret set litellm_base_url <URL>`."
        )
    })?;
    let api_key = crate::secrets::get("litellm_api_key")?.unwrap_or_default();
    let model =
        read_trimmed_secret("litellm_model")?.unwrap_or_else(|| DEFAULT_LITELLM_MODEL.to_owned());
    LiteLLMInvoker::new(base_url, api_key, model)
}

/// Best-effort reachability check for a LiteLLM / OpenAI-compatible
/// proxy. Returns `None` when `{base_url}/health` answers 2xx OR 4xx
/// (the latter still proves we talked to *something* on the port —
/// auth correctness is the proxy's job). Returns `Some(err_string)`
/// on connect failure / timeout / 5xx.
///
/// Used by the wizard's "probe failed — save anyway?" prompt and by
/// `worklog doctor` to surface `estimator.reachable` in its report.
/// 3s timeout keeps an unreachable proxy from hanging the setup flow.
pub fn probe_litellm(base_url: &str) -> Option<String> {
    let client = match reqwest::blocking::Client::builder()
        .user_agent("worklog")
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    match client.get(&url).send() {
        Ok(resp) if resp.status().is_success() || resp.status().is_client_error() => None,
        Ok(resp) => Some(format!("HTTP {} on /health", resp.status())),
        Err(e) => Some(format!("connect: {e}")),
    }
}

/// Read env + secrets to decide which invoker today's run uses. Called
/// by [`estimate_day`] and surfaced in `worklog doctor`.
pub fn resolve_provider() -> Result<ProviderChoice> {
    match read_provider_selector()?.as_deref() {
        None | Some("claude_subprocess") | Some("subprocess") | Some("claude") => {
            Ok(ProviderChoice::ClaudeSubprocess)
        }
        Some("litellm") => Ok(ProviderChoice::LiteLLM(build_litellm_from_secrets()?)),
        Some(other) => anyhow::bail!(
            "unknown estimator provider `{other}`. \
             Expected `claude_subprocess` or `litellm` \
             (set via WORKLOG_ESTIMATOR_PROVIDER env or `worklog setup`)."
        ),
    }
}

/// Invoke the estimator for every un-estimated block on `day`. Routes
/// through whichever [`ProviderChoice`] is active.
pub fn estimate_day(conn: &Connection, day: NaiveDate, model: &str) -> Result<EstimateStats> {
    match resolve_provider()? {
        ProviderChoice::ClaudeSubprocess => estimate_day_with(conn, day, model, &ClaudeSubprocess),
        ProviderChoice::LiteLLM(inv) => estimate_day_with(conn, day, model, &inv),
    }
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

/// Default model passed to LiteLLM when the caller leaves `--model`
/// unset AND the user hasn't configured one via secrets. LiteLLM
/// requires a `provider/model` prefix — unqualified names route
/// nowhere. Anthropic is the wizard's first-class provider.
pub const DEFAULT_LITELLM_MODEL: &str = "anthropic/claude-haiku-4-5";

/// OpenAI-compatible HTTP invoker. Points at any LiteLLM proxy (or any
/// OpenAI-shaped endpoint) and POSTs `/v1/chat/completions`. The
/// response's `choices[0].message.content` is handed to
/// [`parse_response`] so prose-wrapped JSON + envelope shapes work
/// identically to the subprocess path.
///
/// TLS + 30s default timeout come from [`crate::http::client`]. Tests
/// swap in a short-timeout client via [`Self::with_client`].
pub struct LiteLLMInvoker {
    base_url: String,
    api_key: String,
    default_model: String,
    client: reqwest::blocking::Client,
}

impl LiteLLMInvoker {
    /// Build from already-resolved config. `base_url` trailing slash is
    /// tolerated. Empty `api_key` omits the `Authorization` header on
    /// requests (some local proxies run unauthed).
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            default_model: model.into(),
            client: crate::http::client()?,
        })
    }

    /// Test seam: swap the HTTP client (short timeouts, custom TLS,
    /// mock routing). Not exposed in production because every caller
    /// outside tests wants the default `crate::http::client`.
    #[cfg(test)]
    pub fn with_client(mut self, client: reqwest::blocking::Client) -> Self {
        self.client = client;
        self
    }

    /// Where the request actually lands. Pulled into a method so tests
    /// and `doctor` can surface it without reaching inside the struct.
    pub fn endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }

    /// The model the invoker falls back to when the caller passes
    /// `""`. Exposed so `worklog doctor` can print the user-configured
    /// default without needing to re-read the secret.
    pub fn configured_model(&self) -> &str {
        &self.default_model
    }

    /// Chosen model for a given invocation: caller's `--model` wins,
    /// falling back to whatever the user configured in secrets.
    fn resolve_model<'a>(&'a self, caller: &'a str) -> &'a str {
        if caller.is_empty() {
            &self.default_model
        } else {
            caller
        }
    }
}

impl ModelInvoker for LiteLLMInvoker {
    fn invoke(&self, system: &str, user: &str, schema: &Value, model: &str) -> Result<Value> {
        let body = self.build_request_body(system, user, schema, model)?;
        let mut req = self
            .client
            .post(self.endpoint())
            .header("Content-Type", "application/json");
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let resp = req
            .json(&body)
            .send()
            .context("POST /v1/chat/completions")?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!(
                "HTTP {status} from LiteLLM proxy: {}",
                bounded_body_preview(resp)
            );
        }

        let envelope: Value = resp.json().context("decoding LiteLLM JSON response")?;
        let content = extract_message_content(&envelope)?;
        parse_response(content)
    }
}

impl LiteLLMInvoker {
    /// Build the OpenAI-compatible chat.completions body. The schema
    /// ends up in the system prompt so providers that ignore
    /// `response_format` (some on-prem proxies, Ollama) still see it.
    fn build_request_body(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        model: &str,
    ) -> Result<Value> {
        let schema_str = serde_json::to_string(schema)?;
        let system_with_schema =
            format!("{system}\n\nRespond ONLY with JSON matching this schema:\n{schema_str}");
        Ok(json!({
            "model":           self.resolve_model(model),
            "messages": [
                { "role": "system", "content": system_with_schema },
                { "role": "user",   "content": user },
            ],
            "response_format": { "type": "json_object" },
            "temperature":     0,
            "max_tokens":      512,
        }))
    }
}

/// Some proxies echo the full request payload on 5xx — which can
/// include the user's event content. Cap at 500 chars so errors never
/// accidentally persist unbounded PII into logs.
fn bounded_body_preview(resp: reqwest::blocking::Response) -> String {
    resp.text()
        .unwrap_or_else(|_| "<unreadable body>".into())
        .chars()
        .take(500)
        .collect()
}

/// Extract `choices[0].message.content` as a `&str`, with an error
/// that carries the raw envelope so debugging isn't guesswork.
fn extract_message_content(envelope: &Value) -> Result<&str> {
    envelope
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!("LiteLLM response missing choices[0].message.content: {envelope}")
        })
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

    let payload = json!({
        "block_duration_minutes": duration_min,
        "inferred_jira_issue":    block.jira_issue,
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
        };
        let claude_event = EventRow {
            source: "claude".into(),
            started_at: "2026-04-18T09:05:00+00:00".into(),
            title: Some("UserPromptSubmit — fix auth".into()),
            details: Some("c".repeat(500)),
            jira_issue: None,
        };
        let github_event = EventRow {
            source: "github_commit".into(),
            started_at: "2026-04-18T09:10:00+00:00".into(),
            title: Some("Initial commit".into()),
            details: Some("g".repeat(500)),
            jira_issue: None,
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

    // ───────────────── LiteLLM invoker (v0.7 — Phase 2) ─────────────────

    /// OpenAI-compatible content envelope the proxy returns. Keeping this
    /// as a helper keeps the per-test fixtures readable.
    fn openai_envelope(content: &str) -> serde_json::Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
            }]
        })
    }

    fn short_timeout_client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .user_agent("worklog-test")
            .timeout(std::time::Duration::from_millis(200))
            .build()
            .unwrap()
    }

    /// B4: a well-formed proxy reply → the invoker returns the parsed
    /// worklog JSON as a `Value`. The schema the caller sends is embedded
    /// in the system prompt so downstream validation (validate_ticket,
    /// round_up_minutes) keeps working identically to the subprocess path.
    #[test]
    fn litellm_invoker_returns_parsed_reply_on_200() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let content =
            r#"{"jira_issue":"PROJ-1","minutes":30,"description":"Implement auth refresh"}"#;
        server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer test_key");
            then.status(200).json_body(openai_envelope(content));
        });

        let inv = LiteLLMInvoker::new(server.base_url(), "test_key", "anthropic/claude-haiku-4-5")
            .unwrap();
        let schema = response_schema();
        let got = inv.invoke("sys", "user", &schema, "").unwrap();

        assert_eq!(got["jira_issue"], "PROJ-1");
        assert_eq!(got["minutes"], 30);
        assert_eq!(got["description"], "Implement auth refresh");
    }

    /// B5: a 401 must bubble up as a readable error — the outer
    /// `estimate_day_with` loop converts any `Err` into a `gap` row, so
    /// the error message is what lands in the `warn!` tracing event the
    /// user sees when debugging.
    #[test]
    fn litellm_invoker_bails_on_401() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(401).body("invalid api key");
        });

        let inv = LiteLLMInvoker::new(server.base_url(), "bad_key", "m").unwrap();
        let schema = response_schema();
        let err = inv
            .invoke("sys", "user", &schema, "")
            .expect_err("401 must bubble as an Err");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("401"),
            "error should name the HTTP status: {msg}"
        );
    }

    /// B6: providers sometimes wrap JSON in prose ("Here you go: {...}").
    /// The invoker reuses the existing `parse_response` helper so this
    /// path is already covered for the subprocess; we just need to prove
    /// the LiteLLM path delegates into it.
    #[test]
    fn litellm_invoker_handles_prose_wrapped_json_content() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let prose =
            r#"Here you go: {"jira_issue":"PROJ-2","minutes":15,"description":"Fix flaky test"}"#;
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(openai_envelope(prose));
        });

        let inv = LiteLLMInvoker::new(server.base_url(), "k", "m").unwrap();
        let schema = response_schema();
        let got = inv.invoke("sys", "user", &schema, "").unwrap();
        assert_eq!(got["jira_issue"], "PROJ-2");
        assert_eq!(got["minutes"], 15);
    }

    /// B7: if the proxy hangs we want a deterministic failure, not a
    /// silently-hung block. Test uses a 200ms-timeout client against an
    /// httpmock `delay` of 500ms so the request is guaranteed to time out.
    #[test]
    fn litellm_invoker_bails_on_timeout() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200)
                .delay(std::time::Duration::from_millis(500))
                .json_body(openai_envelope("{}"));
        });

        let inv = LiteLLMInvoker::new(server.base_url(), "k", "m")
            .unwrap()
            .with_client(short_timeout_client());
        let schema = response_schema();
        let err = inv
            .invoke("sys", "user", &schema, "")
            .expect_err("timeout must return Err");
        let msg = format!("{err:#}").to_lowercase();
        assert!(
            msg.contains("timed out") || msg.contains("timeout") || msg.contains("operation"),
            "expected timeout-shaped error, got: {msg}"
        );
    }

    /// B8: a local LiteLLM proxy run without auth ignores the
    /// Authorization header — but some servers reject requests that
    /// carry an empty bearer token. If the caller leaves `api_key` empty
    /// we must omit the header entirely, not send `Authorization: Bearer `.
    #[test]
    fn litellm_invoker_omits_authorization_header_when_key_empty() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let hit = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header_exists("Content-Type")
                .matches(|req| {
                    req.headers
                        .as_ref()
                        .map(|h| h.iter().all(|(k, _)| k.to_lowercase() != "authorization"))
                        .unwrap_or(true)
                });
            then.status(200).json_body(openai_envelope(
                r#"{"jira_issue":null,"minutes":5,"description":"x"}"#,
            ));
        });

        let inv = LiteLLMInvoker::new(server.base_url(), "", "m").unwrap();
        let schema = response_schema();
        inv.invoke("sys", "user", &schema, "").unwrap();
        hit.assert();
    }

    /// The invoker's `--model` passthrough: when the caller (e.g.
    /// `worklog estimate --model openai/gpt-4o`) passes a non-empty
    /// model, it wins over the invoker's configured default.
    #[test]
    fn litellm_invoker_uses_caller_model_when_provided() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let hit = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body_partial(r#"{"model":"openai/gpt-4o"}"#);
            then.status(200).json_body(openai_envelope(
                r#"{"jira_issue":null,"minutes":5,"description":"x"}"#,
            ));
        });

        let inv =
            LiteLLMInvoker::new(server.base_url(), "k", "anthropic/claude-haiku-4-5").unwrap();
        let schema = response_schema();
        inv.invoke("sys", "user", &schema, "openai/gpt-4o").unwrap();
        hit.assert();
    }

    // ───────────────── Provider factory (v0.7 — Phase 3) ─────────────────
    //
    // resolve_provider() reads env + secrets and returns a ProviderChoice
    // that estimate_day dispatches on. These tests serialize on their own
    // mutex because they mutate std::env which is process-global.

    use std::sync::Mutex;
    static PROVIDER_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_provider_state() {
        // env
        std::env::remove_var("WORKLOG_ESTIMATOR_PROVIDER");
        std::env::remove_var("WORKLOG_LITELLM_BASE_URL");
        std::env::remove_var("WORKLOG_LITELLM_API_KEY");
        std::env::remove_var("WORKLOG_LITELLM_MODEL");
        // secrets (test backend is a process-global HashMap)
        let _ = crate::secrets::delete("worklog_estimator_provider");
        let _ = crate::secrets::delete("litellm_base_url");
        let _ = crate::secrets::delete("litellm_api_key");
        let _ = crate::secrets::delete("litellm_model");
    }

    /// B1: nothing configured → fall back to the existing subprocess
    /// behaviour. This is the back-compat contract — existing installs
    /// MUST see no behaviour change.
    #[test]
    fn resolve_provider_defaults_to_claude_subprocess_when_nothing_set() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        match resolve_provider().unwrap() {
            ProviderChoice::ClaudeSubprocess => {}
            other => panic!("expected ClaudeSubprocess, got {other:?}"),
        }
    }

    /// B2: env var picks LiteLLM and the minimum required secrets are
    /// present → factory returns a configured LiteLLMInvoker.
    #[test]
    fn resolve_provider_picks_litellm_when_env_says_so_and_url_present() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        std::env::set_var("WORKLOG_ESTIMATOR_PROVIDER", "litellm");
        crate::secrets::set("litellm_base_url", "http://localhost:4000").unwrap();
        crate::secrets::set("litellm_model", "anthropic/claude-haiku-4-5").unwrap();

        let choice = resolve_provider().unwrap();
        match &choice {
            ProviderChoice::LiteLLM(inv) => {
                assert!(inv.endpoint().starts_with("http://localhost:4000"));
                assert!(inv.endpoint().ends_with("/v1/chat/completions"));
            }
            other => panic!("expected LiteLLM, got {other:?}"),
        }
        clear_provider_state();
    }

    /// B3: user selected LiteLLM but forgot to configure the URL. The
    /// error must name the missing key AND point at `worklog setup` so
    /// the recovery path is obvious.
    #[test]
    fn resolve_provider_errors_when_litellm_selected_but_url_missing() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        std::env::set_var("WORKLOG_ESTIMATOR_PROVIDER", "litellm");
        // no base_url secret
        let err = resolve_provider().unwrap_err().to_string();
        assert!(
            err.contains("litellm_base_url"),
            "err should name the missing key: {err}"
        );
        assert!(
            err.contains("worklog setup") || err.contains("worklog secret set"),
            "err should point at the recovery command: {err}"
        );
        clear_provider_state();
    }

    /// Env is process-wide and ephemeral; the persistent choice also
    /// lives in the keychain under `worklog_estimator_provider`. Env
    /// wins when both are set, but when env is unset the secret is
    /// consulted.
    #[test]
    fn resolve_provider_reads_secret_when_env_unset() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        crate::secrets::set("worklog_estimator_provider", "litellm").unwrap();
        crate::secrets::set("litellm_base_url", "http://localhost:4000").unwrap();
        let choice = resolve_provider().unwrap();
        assert!(matches!(choice, ProviderChoice::LiteLLM(_)));
        clear_provider_state();
    }

    /// If the user didn't set `litellm_model` we fall back to the
    /// first-class default (`anthropic/claude-haiku-4-5`) — the same
    /// constant the wizard uses.
    #[test]
    fn resolve_provider_uses_default_litellm_model_when_model_secret_missing() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        std::env::set_var("WORKLOG_ESTIMATOR_PROVIDER", "litellm");
        crate::secrets::set("litellm_base_url", "http://localhost:4000").unwrap();
        // no litellm_model

        match resolve_provider().unwrap() {
            ProviderChoice::LiteLLM(inv) => {
                assert_eq!(inv.resolve_model(""), DEFAULT_LITELLM_MODEL);
            }
            _ => panic!("expected LiteLLM"),
        }
        clear_provider_state();
    }

    /// An unrecognised provider string must error, not silently fall
    /// back to one of the two valid choices — typos in a config file
    /// shouldn't quietly run the wrong estimator.
    #[test]
    fn resolve_provider_errors_on_unknown_provider_string() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        std::env::set_var("WORKLOG_ESTIMATOR_PROVIDER", "openai_direct");
        let err = resolve_provider().unwrap_err().to_string();
        assert!(
            err.contains("openai_direct") || err.contains("unknown"),
            "err should name the bad value: {err}"
        );
        clear_provider_state();
    }
}
