//! Claude Code hook handler — reads stdin JSON, writes `events` + updates
//! `sessions`, never prints to stdout so Claude never sees our output.

use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::Connection;
use serde_json::Value;
use tracing::warn;

use crate::models::Event;
use crate::repo;
use crate::sessions;

/// Jira key regex — same as the collectors' regex so hook-produced events
/// get the same treatment as GitHub-sourced ones.
pub fn jira_re() -> Regex {
    Regex::new(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b").unwrap()
}

/// Events that close a session when received.
fn close_reason(event: &str) -> Option<&'static str> {
    match event {
        "Stop" => Some("stop"),
        "SessionEnd" => Some("session_end"),
        _ => None,
    }
}

fn first_jira_key(parts: &[Option<&str>]) -> Option<String> {
    let re = jira_re();
    for p in parts.iter().copied().flatten() {
        if let Some(m) = re.find(p) {
            return Some(m.as_str().to_owned());
        }
    }
    None
}

/// Jira key from the working directory path, else from its git branch.
pub fn path_or_branch_key(cwd: &str) -> Option<String> {
    first_jira_key(&[Some(cwd)]).or_else(|| {
        let branch = crate::git::current_branch(Path::new(cwd));
        first_jira_key(&[branch.as_deref()])
    })
}

fn prompt_of(payload: &Value) -> Option<&str> {
    payload
        .get("prompt")
        .and_then(Value::as_str)
        .or_else(|| payload.get("user_prompt").and_then(Value::as_str))
}

fn title_for(event: &str, prompt: Option<&str>) -> String {
    match prompt {
        Some(p) if !p.is_empty() => {
            let snippet = p.chars().take(80).collect::<String>();
            format!("{event} — {snippet}")
        }
        _ => event.to_owned(),
    }
}

/// Max chars preserved when storing a Claude Code user prompt into
/// `event.details`. 4 KiB is plenty for normal prompts but cheap insurance
/// against someone pasting a 50 KB error log — that would balloon the row
/// (and later the estimator's token bill) without adding real signal.
pub const PROMPT_CAP_CHARS: usize = 4096;

/// Slice `s` to at most `max` chars, appending a human-readable marker
/// `"…<truncated N chars>"` when truncation happens. Truncation is on
/// char boundaries so multi-byte UTF-8 is never corrupted.
fn cap_prompt(s: &str, max: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_owned();
    }
    let kept: String = s.chars().take(max).collect();
    let dropped = total - max;
    format!("{kept}…<truncated {dropped} chars>")
}

/// The full hook payload, scrubbed, for the Details view (spec 006).
fn raw_record(event: &str, payload: &Value) -> Option<String> {
    serde_json::to_string(&crate::clues_contract::RawRecord::Hook {
        event: event.to_owned(),
        payload: crate::scrub::scrub_json(payload),
    })
    .ok()
}

/// Collapse `/.claude/worktrees/<name>` to its repo root the same way
/// `collectors::claude_transcripts` does, or the same repo splits into two
/// project folders depending on which source saw it (A11/A12).
fn project_root(cwd: Option<&str>) -> Option<String> {
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    cwd.and_then(|c| crate::collectors::fish::repo_root_for(c, home.as_deref()))
}

/// Prefer the user prompt so the estimator has real substance to
/// summarise from — but run it through `redact_code` FIRST so source
/// code (pasted snippets, `<task-notification>` `<result>` blocks) never
/// lands in the DB at all, then cap what remains. `None` when there's no
/// prompt: the transcript path that used to be stored here is just a
/// pointer to the full session and carries no work-intent signal worth
/// keeping.
fn details_for(prompt: Option<&str>) -> Option<String> {
    match prompt {
        Some(p) if !p.is_empty() => {
            let clean = crate::estimate::redact_code(p);
            (!clean.is_empty()).then(|| cap_prompt(&clean, PROMPT_CAP_CHARS))
        }
        _ => None,
    }
}

/// Process a single hook payload against an already-open connection. Errors
/// are logged to stderr by the caller; this function bails on a hard db
/// failure only so the CLI entrypoint can still return exit 0 (we never
/// want to block the user's Claude session).
pub fn handle(conn: &Connection, payload: &Value, now: DateTime<Utc>) -> Result<()> {
    let event = payload
        .get("hook_event_name")
        .and_then(Value::as_str)
        .or_else(|| payload.get("event").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_owned();
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("no-session")
        .to_owned();
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .or_else(|| payload.get("project_path").and_then(Value::as_str))
        .map(str::to_owned);
    // Scrubbed BEFORE title_for's 80-char cut and redact_code so a live
    // token can never be sliced into a partial prefix the whole-token
    // regexes miss (FR-11, D-03).
    let prompt = prompt_of(payload).map(crate::scrub::scrub_secrets);

    // An explicit `worklog ticket use` beats a key guessed from the path
    // or branch, so the Owner can correct a misleading branch name.
    let jira_issue = first_jira_key(&[prompt.as_deref()])
        .or_else(|| {
            crate::session_tickets::get(conn, &session_id)
                .ok()
                .flatten()
        })
        .or_else(|| path_or_branch_key(cwd.as_deref()?));
    let details = details_for(prompt.as_deref());

    let ev = Event {
        id: None,
        source: "claude".into(),
        source_id: format!("{session_id}:{event}:{}", now.to_rfc3339()),
        started_at: now.to_rfc3339(),
        ended_at: None,
        duration_seconds: None,
        title: title_for(&event, prompt.as_deref()),
        details,
        repo: None,
        project_path: project_root(cwd.as_deref()),
        jira_issue,
        session_id: Some(session_id.clone()),
        tempo_worklog_id: None,
        raw_json: raw_record(&event, payload),
    };
    repo::upsert_event(conn, &ev)?;

    sessions::open_session(conn, &session_id, now, cwd.as_deref())?;
    if let Some(reason) = close_reason(&event) {
        sessions::close_session(conn, &session_id, now, reason)?;
    }
    sessions::reap_stale(conn, now)?;

    Ok(())
}

/// Hard cap on Claude Code hook payloads. Realistic events are a few KB
/// of JSON (session id + event name + a short prompt excerpt). 4MB is
/// generous headroom. Without this, a runaway Claude process could
/// flood the DB with a single huge row.
pub const MAX_STDIN_BYTES: u64 = 4 * 1024 * 1024;

/// Env var that suppresses this hook. worklog's estimator shells out to
/// `claude -p`, which inherits the user's Claude Code hook config and
/// would otherwise re-fire this hook — recording the estimation prompt
/// itself as a `claude` activity event. The estimator sets this on the
/// subprocess so the inner `worklog hook-run` no-ops.
pub const SUPPRESS_ENV: &str = "WORKLOG_HOOK_SUPPRESS";

/// CLI entrypoint. Reads stdin, opens db, handles. Always returns Ok —
/// hook-side errors are logged to stderr but never propagated (Claude
/// must never be blocked by a worklog failure).
pub fn run_from_stdin() -> Result<()> {
    // Drop the event when invoked from worklog's own estimator subprocess
    // (see `SUPPRESS_ENV`). Checked before reading stdin so the payload is
    // discarded unparsed.
    if std::env::var_os(SUPPRESS_ENV).is_some() {
        return Ok(());
    }
    use std::io::Read;
    let mut buf = String::new();
    // `Read::take` enforces MAX_STDIN_BYTES so a pathological payload
    // can't exhaust memory or balloon the DB. Anything at or near the
    // cap is truncated and rejected — we'd rather drop the event than
    // store 4MB of garbage.
    let stdin = std::io::stdin();
    let mut limited = stdin.lock().take(MAX_STDIN_BYTES + 1);
    if let Err(e) = limited.read_to_string(&mut buf) {
        warn!("reading stdin failed: {e}");
        return Ok(());
    }
    if buf.len() as u64 > MAX_STDIN_BYTES {
        eprintln!(
            "worklog hook: stdin exceeded {}MB cap — dropping event",
            MAX_STDIN_BYTES / 1024 / 1024
        );
        return Ok(());
    }
    let payload: Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("worklog hook: invalid JSON on stdin: {e}");
            return Ok(());
        }
    };

    let paths = crate::paths::Paths::resolve()?;
    paths.ensure()?;
    let conn = crate::db::open(&paths.db)?;
    if let Err(e) = handle(&conn, &payload, Utc::now()) {
        eprintln!("worklog hook: {e}");
    }
    Ok(())
}

// Tests live in hook_run_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "hook_run_test.rs"]
mod tests;

#[cfg(test)]
mod stored_ticket_tests {
    use super::*;
    use crate::db::open_memory;
    use serde_json::json;

    fn issue(conn: &Connection, prompt: &str, cwd: &str) -> Option<String> {
        let now = Utc::now();
        let payload = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "st1",
            "cwd": cwd,
            "prompt": prompt,
        });
        handle(conn, &payload, now).unwrap();
        let day = now.format("%Y-%m-%d").to_string();
        repo::load_day_events(conn, &day).unwrap()[0]
            .jira_issue
            .clone()
    }

    #[test]
    fn stored_session_ticket_beats_path_key() {
        let conn = open_memory().unwrap();
        crate::session_tickets::set(&conn, "st1", "GENAI-9").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("PROJ-42-x");
        std::fs::create_dir(&dir).unwrap();
        let cwd = dir.to_str().unwrap();
        assert_eq!(issue(&conn, "fix it", cwd).as_deref(), Some("GENAI-9"));
    }

    #[test]
    fn stored_session_ticket_without_other_keys() {
        let conn = open_memory().unwrap();
        crate::session_tickets::set(&conn, "st1", "GENAI-9").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        assert_eq!(issue(&conn, "fix it", cwd).as_deref(), Some("GENAI-9"));
    }

    #[test]
    fn prompt_key_beats_stored_session_ticket() {
        let conn = open_memory().unwrap();
        crate::session_tickets::set(&conn, "st1", "GENAI-9").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        assert_eq!(issue(&conn, "see PROJ-42", cwd).as_deref(), Some("PROJ-42"));
    }
}
