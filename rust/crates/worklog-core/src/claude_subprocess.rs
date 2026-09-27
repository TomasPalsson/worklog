//! `claude -p` subprocess [`ModelInvoker`] — split out of `estimate.rs`
//! to keep that file to call lines only.

use crate::estimate::{parse_response, ModelInvoker};
use anyhow::{Context, Result};
use serde_json::Value;
use std::process::{Child, Command};
use std::thread::JoinHandle;
use std::time::Instant;

/// Wall-clock timeout for the per-block estimator's `claude -p` calls —
/// unchanged from before [`ClaudeSubprocess::with_thinking`] existed.
const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Timeout for a thinking-enabled invocation (line texts): a bigger
/// thinking budget needs more wall-clock room than the per-block
/// estimator's 60s.
pub const THINKING_TIMEOUT_SECS: u64 = 180;

/// Shells out to `claude -p`. The default (`ClaudeSubprocess::default`)
/// behaves exactly as before thinking support existed: no
/// `MAX_THINKING_TOKENS`, [`DEFAULT_TIMEOUT_SECS`]. Use
/// [`ClaudeSubprocess::with_thinking`] for a caller (line texts) that
/// wants a thinking budget and the longer timeout that comes with it.
#[derive(Default)]
pub struct ClaudeSubprocess {
    thinking_tokens: Option<u32>,
    timeout_secs: Option<u64>,
}

impl ClaudeSubprocess {
    /// Sets `MAX_THINKING_TOKENS` on the spawned `claude -p` and switches
    /// the wall-clock timeout to [`THINKING_TIMEOUT_SECS`].
    pub fn with_thinking(tokens: u32) -> Self {
        Self {
            thinking_tokens: Some(tokens),
            timeout_secs: Some(THINKING_TIMEOUT_SECS),
        }
    }

    fn timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS)
    }
}

/// Args + env vars for one `claude -p` invocation, factored out of
/// [`ClaudeSubprocess::invoke`] so a test can assert on them without
/// spawning a process.
pub fn command_spec(
    system: &str,
    schema_str: &str,
    model: &str,
    thinking_tokens: Option<u32>,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut args: Vec<String> = [
        "-p",
        "--model",
        model,
        "--output-format",
        "json",
        "--json-schema",
        schema_str,
        "--system-prompt",
        system,
        // A pure text call: no tools, no MCP servers, none of the user's
        // hooks/plugins/settings. Without this every call loads the whole
        // user setup (~40k tokens) and may wander off using tools — the
        // line-text job took minutes instead of seconds.
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "project",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if thinking_tokens.is_some() {
        // User settings are skipped above, so thinking is requested here.
        args.extend(
            [
                "--settings",
                r#"{"alwaysThinkingEnabled":true}"#,
                "--effort",
                "high",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }
    // The spawned `claude -p` inherits this process's Claude Code hook
    // config, so it would re-fire worklog's own hook and log this
    // estimation prompt back into `events` as fake activity — which
    // then clusters into phantom blocks. `hook_run::run_from_stdin`
    // honours this env var by dropping the event entirely.
    let mut envs = vec![(crate::hook_run::SUPPRESS_ENV.to_string(), "1".to_string())];
    if let Some(tokens) = thinking_tokens {
        envs.push(("MAX_THINKING_TOKENS".to_string(), tokens.to_string()));
    }
    (args, envs)
}

impl ModelInvoker for ClaudeSubprocess {
    fn invoke(&self, system: &str, user: &str, schema: &Value, model: &str) -> Result<Value> {
        let schema_str = serde_json::to_string(schema)?;
        let (args, envs) = command_spec(system, &schema_str, model, self.thinking_tokens);
        let (child, out_handle, err_handle) = spawn_and_feed(&args, &envs, user)?;
        wait_for_reply(child, out_handle, err_handle, self.timeout_secs())
    }
}

/// Spawns `claude`, starts draining its stdout/stderr pipes on their own
/// threads (before waiting — see the comment below), writes `user` to
/// stdin and closes it. Split out of `invoke` to stay under the
/// per-function line guard.
fn spawn_and_feed(
    args: &[String],
    envs: &[(String, String)],
    user: &str,
) -> Result<(Child, JoinHandle<String>, JoinHandle<String>)> {
    let mut cmd = Command::new("claude");
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning `claude`")?;

    // Drain both pipes on their own threads, starting BEFORE we wait. A
    // piped child that outgrows the OS pipe buffer (64 KiB on macOS)
    // blocks forever on write until the parent reads; the poll loop in
    // `wait_for_reply` only reads once the child has already exited, so
    // reading there alone would deadlock every large response into the
    // timeout.
    let mut out_pipe = child.stdout.take().context("`claude` stdout missing")?;
    let mut err_pipe = child.stderr.take().context("`claude` stderr missing")?;
    let out_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let _ = out_pipe.read_to_string(&mut s);
        s
    });
    let err_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let _ = err_pipe.read_to_string(&mut s);
        s
    });

    // Write prompt, then close stdin so the process can finish. On a
    // write failure the child is already running, so kill and reap it
    // rather than leaking a live `claude` for the daemon's lifetime.
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(user.as_bytes()) {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow::Error::from(e).context("writing prompt to `claude` stdin"));
        }
    }
    Ok((child, out_handle, err_handle))
}

/// Polls `child` to completion or `timeout_secs`, whichever comes first.
/// Split out of `invoke` to stay under the per-function line guard.
fn wait_for_reply(
    mut child: Child,
    out_handle: JoinHandle<String>,
    err_handle: JoinHandle<String>,
    timeout_secs: u64,
) -> Result<Value> {
    // Simple wall-clock timeout (claude -p is fast on haiku; the default
    // 60s is generous — a thinking-enabled call gets more). If it hangs,
    // kill.
    let wait_start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout = out_handle.join().unwrap_or_default();
                let stderr = err_handle.join().unwrap_or_default();
                if !status.success() {
                    // `claude -p --output-format json` reports its own
                    // failures (prompt too long, rate limit, auth) as
                    // JSON on STDOUT and leaves stderr empty, so a
                    // stderr-only message logs a bare "exited 1 — " and
                    // throws the actual reason away. Carry both.
                    anyhow::bail!(
                        "claude -p exited {} — stderr: {} | stdout: {}",
                        status.code().unwrap_or(-1),
                        stderr.trim().chars().take(500).collect::<String>(),
                        stdout.trim().chars().take(500).collect::<String>(),
                    );
                }
                return parse_response(&stdout);
            }
            None => {
                if wait_start.elapsed().as_secs() > timeout_secs {
                    let _ = child.kill();
                    // kill() only signals; without wait() the corpse is
                    // never reaped and every timeout leaks a zombie for
                    // as long as the daemon lives.
                    let _ = child.wait();
                    anyhow::bail!("claude -p timed out after {timeout_secs}s");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[path = "claude_subprocess_test.rs"]
#[cfg(test)]
mod tests;
