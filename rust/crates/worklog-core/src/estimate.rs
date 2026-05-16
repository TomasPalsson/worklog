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
    // Reject non-http(s) schemes up-front so a misconfigured
    // `file:///…` or bare hostname in the keychain can't trigger an
    // accidental local-fs read / SSRF against an unintended target.
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Some(format!(
            "base_url must start with http:// or https:// (got `{base_url}`)"
        ));
    }
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
        // The spawned `claude -p` inherits this process's Claude Code hook
        // config, so it would re-fire worklog's own hook and log this
        // estimation prompt back into `events` as fake activity — which
        // then clusters into phantom blocks. `hook_run::run_from_stdin`
        // honours this env var by dropping the event entirely.
        cmd.env(crate::hook_run::SUPPRESS_ENV, "1");
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
/// # Contract
///
/// A single `LiteLLMInvoker` instance assumes `system` and `schema`
/// are IDENTICAL across every `invoke` call — `system_with_schema` is
/// memoised on first use and reused thereafter for perf. This matches
/// how `estimate_day` uses it (one invoker per run, same prompt +
/// schema for every block). Constructing a fresh invoker per
/// structurally-different task keeps the memoisation correct.
pub struct LiteLLMInvoker {
    base_url: String,
    api_key: String,
    default_model: String,
    client: reqwest::blocking::Client,
    /// Memoised `system + schema hint` — the upstream estimator passes
    /// the same `system` and `schema` for every block in a single
    /// `estimate_day` run, so we allocate the combined string once and
    /// reuse it. Per-block `build_request_body` now just clones a
    /// `String` reference instead of re-running `serde_json::to_string`
    /// and `format!` on every iteration. See the struct-level contract
    /// above — callers reusing one invoker for heterogeneous prompts
    /// would silently get the first call's cached string.
    system_with_schema: std::sync::OnceLock<String>,
}

/// Hard cap on the `/v1/chat/completions` response body. A compliant
/// proxy responding to `max_tokens: 512` emits at most ~3 KB; we grant
/// 1 MiB headroom for multi-turn or reasoning envelopes while bounding
/// the OOM surface from a hostile / compromised proxy (which could
/// otherwise stream gigabytes of JSON into `serde_json::from_slice`).
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

impl LiteLLMInvoker {
    /// Build from already-resolved config. `base_url` trailing slash is
    /// tolerated. Empty `api_key` omits the `Authorization` header on
    /// requests (some local proxies run unauthed).
    ///
    /// Rejects any scheme other than `http://` or `https://` so a
    /// misconfigured secret (`file:///etc/passwd`, `ftp://…`, bare
    /// hostnames) fails at construction time rather than at the first
    /// estimate call. RFC1918 / link-local IPs are intentionally
    /// allowed because `http://localhost:4000` is the documented happy
    /// path for a local LiteLLM proxy.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self> {
        let base_url = base_url.into();
        // LiteLLM's own docs show `http://localhost:4000/v1` as the
        // proxy URL for OpenAI-compatible clients — easy for a user to
        // paste into `litellm_base_url` directly. Our `endpoint()`
        // always appends `/v1/chat/completions`, so a user-provided
        // `/v1` suffix produced `…/v1/v1/chat/completions` and silently
        // gapped every block. Strip `/v1` (trailing-slash tolerant) up
        // front so either form works.
        let base_url = base_url
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_owned();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            anyhow::bail!(
                "litellm_base_url must start with http:// or https:// (got `{}`). \
                 Run `worklog secret set litellm_base_url <URL>` to fix.",
                base_url
            );
        }
        Ok(Self {
            base_url,
            api_key: api_key.into(),
            default_model: model.into(),
            client: crate::http::client()?,
            system_with_schema: std::sync::OnceLock::new(),
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
    /// Whitespace-only callers (`--model "   "`) are treated as empty
    /// so the fallback kicks in instead of forwarding garbage to the
    /// proxy.
    fn resolve_model<'a>(&'a self, caller: &'a str) -> &'a str {
        if caller.trim().is_empty() {
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

        // Read as bytes, cap the size, then decode. `.json()` would
        // buffer the entire body unbounded — a hostile proxy streaming
        // gigabytes of JSON would OOM the process. The 1 MiB cap is
        // comfortable headroom over the ~3 KB typical response.
        let bytes = resp.bytes().context("reading LiteLLM response body")?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            anyhow::bail!(
                "LiteLLM response body exceeded {} MiB cap — refusing to decode (possible hostile proxy)",
                MAX_RESPONSE_BYTES / (1024 * 1024)
            );
        }
        let envelope: Value =
            serde_json::from_slice(&bytes).context("decoding LiteLLM JSON response")?;
        let content = extract_message_content(&envelope)?;
        parse_response(content)
    }
}

impl LiteLLMInvoker {
    /// Build the OpenAI-compatible chat.completions body. The schema
    /// ends up in the system prompt so providers that ignore
    /// `response_format` (some on-prem proxies, Ollama) still see it.
    /// The combined system+schema string is memoised in
    /// `self.system_with_schema`; per-block calls clone-by-reference
    /// instead of re-running the format+serialize dance.
    fn build_request_body(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        model: &str,
    ) -> Result<Value> {
        let system_with_schema = self
            .system_with_schema
            .get_or_init(|| {
                let schema_str = serde_json::to_string(schema).unwrap_or_else(|_| "{}".into());
                format!("{system}\n\nRespond ONLY with JSON matching this schema:\n{schema_str}")
            })
            .as_str();
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
            // Sanitise BEFORE truncating: the estimator must see work
            // intent, never source code (see `redact_code`).
            let summary = redact_code(e.title.as_deref().unwrap_or(""));
            let details = e
                .details
                .as_deref()
                .map(redact_code)
                .filter(|d| !d.is_empty());
            json!({
                "type":       e.source,
                "timestamp":  e.started_at,
                "summary":    trunc(&summary, 200),
                "details":    details.as_deref().map(|d| trunc(d, cap)),
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

/// Strip source code and file-path leakage out of an event field before
/// it is handed to the estimator LLM (`claude -p` / LiteLLM). The
/// estimator only ever needs *work intent* — what was worked on — never
/// the code itself. Three known carriers of code reach `details`:
///
///  1. `<task-notification>` blocks — Claude Code injects these as
///     `UserPromptSubmit` prompts when a background agent finishes; the
///     `<result>` element holds whatever that agent produced (diffs,
///     review findings, full functions). We keep only the `<summary>`.
///  2. fenced code blocks pasted into a prompt — replaced with a marker.
///  3. a bare transcript path (`…/<uuid>.jsonl`) — handing the LLM a
///     path to the entire session transcript is itself an exposure, so
///     it is dropped.
///
/// Idempotent and cheap; safe to call on every field of every event.
fn redact_code(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    // (3) bare transcript path — no spaces, ends in `.jsonl`.
    if t.ends_with(".jsonl") && !t.contains(char::is_whitespace) {
        return String::new();
    }
    // (1) task-notification — collapse to its one-line summary.
    if t.contains("<task-notification>") {
        return match extract_xml_tag(t, "summary") {
            Some(s) => format!("background task: {}", s.trim()),
            None => "background task completed".to_string(),
        };
    }
    // (2) fenced code blocks — ``` … ``` → placeholder.
    strip_code_fences(raw)
}

/// Inner text of the first `<tag>…</tag>` in `s`, if present.
fn extract_xml_tag<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let rest = &s[start..];
    let end = rest.find(&close)?;
    Some(&rest[..end])
}

/// Replace every triple-backtick fenced block with `[code omitted]`. An
/// unterminated fence (truncated prompt) redacts to end-of-string so a
/// half-captured block never leaks.
fn strip_code_fences(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("```") {
        out.push_str(&rest[..open]);
        out.push_str("[code omitted]");
        let after = &rest[open + 3..];
        match after.find("```") {
            Some(close) => rest = &after[close + 3..],
            None => return out, // unterminated — drop the remainder
        }
    }
    out.push_str(rest);
    out
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

/// The project prefix of a Jira key — `GOJ-1310` → `GOJ`. `None` when
/// the string has no `-` (so it can't be a Jira key at all).
fn ticket_prefix(key: &str) -> Option<&str> {
    key.split_once('-').map(|(p, _)| p)
}

fn validate_ticket(
    claimed: Option<&str>,
    candidates: &[Candidate],
    literals: &[String],
) -> Option<String> {
    let claimed = claimed?;
    // An exact match against a cached open ticket is always trusted.
    if candidates.iter().any(|c| c.key == claimed) {
        return Some(claimed.to_owned());
    }
    // Literal fallback. The model echoed a `KEY-N` token that genuinely
    // appeared in the block's events — but that alone is far too weak:
    // ordinary text is full of `KEY-N`-shaped noise (`UTF-8`, `GPT-4`,
    // `SHA-256`, `ISO-8601`, severity tags like `CRIT-1` / `HIGH-2`).
    // Accepting those produced phantom tickets like `CRIT-1`.
    //
    // So only trust a literal when its project prefix matches a real
    // Jira project we have cached. A genuine but un-cached ticket
    // (closed, or filed after the last `collect jira`) still passes
    // because its project is known; an invented prefix never does.
    let claimed_prefix = ticket_prefix(claimed)?;
    let prefix_is_real = candidates
        .iter()
        .filter_map(|c| ticket_prefix(&c.key))
        .any(|p| p == claimed_prefix);
    if prefix_is_real && literals.iter().any(|l| l == claimed) {
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
        // An un-cached ticket under the SAME real project, present as a
        // literal in the events, is accepted (closed / freshly filed).
        let literals = vec!["PROJ-2".to_string()];
        assert_eq!(
            validate_ticket(Some("PROJ-1"), &candidates, &literals).as_deref(),
            Some("PROJ-1")
        );
        assert_eq!(
            validate_ticket(Some("PROJ-2"), &candidates, &literals).as_deref(),
            Some("PROJ-2")
        );
        // Hallucinated key — must be rejected.
        assert_eq!(
            validate_ticket(Some("FAKE-99"), &candidates, &literals),
            None
        );
        assert_eq!(validate_ticket(None, &candidates, &literals), None);
    }

    #[test]
    fn validate_ticket_rejects_noise_literals_with_unknown_prefixes() {
        // Regression: `KEY-N`-shaped noise in event text used to be
        // accepted as a ticket because it matched the literal regex.
        // Severity tags, version strings, etc. must NOT become tickets
        // even when the model echoes them and they appear as literals.
        let candidates = vec![Candidate {
            key: "GOJ-1310".into(),
            summary: "real work".into(),
            status: None,
        }];
        for noise in ["CRIT-1", "UTF-8", "GPT-4", "SHA-256", "HIGH-2"] {
            let literals = vec![noise.to_string()];
            assert_eq!(
                validate_ticket(Some(noise), &candidates, &literals),
                None,
                "{noise} has no real project prefix — must be rejected"
            );
        }
        // …but a real un-cached ticket under the GOJ project still passes.
        let literals = vec!["GOJ-9001".to_string()];
        assert_eq!(
            validate_ticket(Some("GOJ-9001"), &candidates, &literals).as_deref(),
            Some("GOJ-9001"),
        );
    }

    // ───────────── estimator code-leak redaction ─────────────

    #[test]
    fn redact_code_leaves_plain_prose_untouched() {
        let prose = "Fix the OAuth token refresh in the auth module";
        assert_eq!(redact_code(prose), prose);
    }

    #[test]
    fn redact_code_collapses_task_notifications_to_their_summary() {
        // Claude Code injects these as prompts when a background agent
        // finishes; the <result> holds code we must not forward.
        let notif = "<task-notification><task-id>abc</task-id>\
             <summary>Agent \"PR #39 security review\" completed</summary>\
             <result>{\"file\": \"handler.py\", \"line\": 50, \
             \"code\": \"def handler(event): return secret\"}</result>\
             </task-notification>";
        let got = redact_code(notif);
        assert_eq!(
            got,
            "background task: Agent \"PR #39 security review\" completed"
        );
        assert!(!got.contains("def handler"), "result code must be gone");
        assert!(!got.contains("handler.py"), "result paths must be gone");
    }

    #[test]
    fn redact_code_strips_fenced_code_blocks() {
        let pasted = "look at this bug:\n```rust\nfn leak() { dbg!(secret); }\n```\nwhy?";
        let got = redact_code(pasted);
        assert!(got.contains("look at this bug"));
        assert!(got.contains("why?"));
        assert!(got.contains("[code omitted]"));
        assert!(!got.contains("secret"), "fenced code must not survive");
    }

    #[test]
    fn redact_code_drops_an_unterminated_fence_entirely() {
        // A truncated prompt can leave a half-captured code block — the
        // remainder after an unclosed fence is dropped, not forwarded.
        let got = redact_code("debugging:\n```python\nAPI_KEY = 'sk-live-xyz'");
        assert!(got.contains("debugging"));
        assert!(
            !got.contains("sk-live-xyz"),
            "unterminated fence must be dropped"
        );
    }

    #[test]
    fn redact_code_drops_bare_transcript_paths() {
        let path = "/Users/x/.claude/projects/-Users-x-proj/abc-123.jsonl";
        assert_eq!(redact_code(path), "");
        // …but a sentence that merely mentions a .jsonl file is kept.
        let prose = "wrote results to output.jsonl for the report";
        assert_eq!(redact_code(prose), prose);
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

    /// QA regression — LiteLLM's own docs sometimes print
    /// `http://localhost:4000/v1` as the proxy URL, so users paste
    /// that straight into `litellm_base_url`. Without the strip, we
    /// would POST to `…/v1/v1/chat/completions` and silently gap
    /// every block. The constructor strips a trailing `/v1` so both
    /// `http://…:4000` and `http://…:4000/v1` land at the same
    /// `endpoint()`.
    #[test]
    fn litellm_invoker_new_strips_trailing_v1_from_base_url() {
        let a = LiteLLMInvoker::new("http://localhost:4000", "k", "m").unwrap();
        let b = LiteLLMInvoker::new("http://localhost:4000/v1", "k", "m").unwrap();
        let c = LiteLLMInvoker::new("http://localhost:4000/v1/", "k", "m").unwrap();
        assert_eq!(a.endpoint(), "http://localhost:4000/v1/chat/completions");
        assert_eq!(b.endpoint(), "http://localhost:4000/v1/chat/completions");
        assert_eq!(c.endpoint(), "http://localhost:4000/v1/chat/completions");
    }

    /// QA regression — non-http(s) schemes must error at construction
    /// so a typo in the keychain fails fast rather than hanging on
    /// the first estimate call. Using `.err()` (not `.unwrap_err()`)
    /// avoids requiring Debug on the Ok variant; LiteLLMInvoker
    /// intentionally doesn't derive Debug so api_key can't leak.
    #[test]
    fn litellm_invoker_new_rejects_non_http_scheme() {
        let err = LiteLLMInvoker::new("file:///etc/passwd", "k", "m")
            .err()
            .expect("non-http scheme must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("http://") && msg.contains("https://"),
            "error must name the valid schemes: {msg}"
        );
    }

    /// QA regression — `--model "   "` must fall back to the
    /// configured default, not forward whitespace as the model name.
    #[test]
    fn litellm_invoker_resolve_model_treats_whitespace_as_empty() {
        let inv = LiteLLMInvoker::new("http://localhost:4000", "k", "anthropic/x").unwrap();
        assert_eq!(inv.resolve_model(""), "anthropic/x");
        assert_eq!(inv.resolve_model("   "), "anthropic/x");
        assert_eq!(inv.resolve_model("\t\n "), "anthropic/x");
        assert_eq!(inv.resolve_model("openai/gpt-4o"), "openai/gpt-4o");
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
    /// constant the wizard uses. Asserts via the public
    /// `configured_model()` surface rather than the private
    /// dispatch helper.
    #[test]
    fn resolve_provider_uses_default_litellm_model_when_model_secret_missing() {
        let _g = PROVIDER_ENV_LOCK.lock().unwrap();
        clear_provider_state();
        std::env::set_var("WORKLOG_ESTIMATOR_PROVIDER", "litellm");
        crate::secrets::set("litellm_base_url", "http://localhost:4000").unwrap();
        // no litellm_model

        match resolve_provider().unwrap() {
            ProviderChoice::LiteLLM(inv) => {
                assert_eq!(inv.configured_model(), DEFAULT_LITELLM_MODEL);
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
