//! Claude Code hook handler — reads stdin JSON, writes `events` + updates
//! `sessions`, never prints to stdout so Claude never sees our output.
//!
//! Matches `src/worklog/collectors/claude.py`. The Python file is the
//! reference implementation; the existing standalone Rust hook binary and
//! this module write the same rows.

use anyhow::Result;
use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Value};
use tracing::warn;

use crate::models::Event;
use crate::repo;
use crate::sessions;

/// Jira key regex — same as the collectors' regex so hook-produced events
/// get the same treatment as GitHub-sourced ones.
fn jira_re() -> Regex {
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
    let prompt = prompt_of(payload).map(str::to_owned);

    let jira_issue = first_jira_key(&[prompt.as_deref(), cwd.as_deref()]);

    let transcript = payload
        .get("transcript_path")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let raw_json = json!(payload).to_string();

    let ev = Event {
        id: None,
        source: "claude".into(),
        source_id: format!("{session_id}:{event}:{}", now.to_rfc3339()),
        started_at: now.to_rfc3339(),
        ended_at: None,
        duration_seconds: None,
        title: title_for(&event, prompt.as_deref()),
        details: transcript,
        repo: None,
        project_path: cwd.clone(),
        jira_issue,
        session_id: Some(session_id.clone()),
        tempo_worklog_id: None,
        raw_json: Some(raw_json),
    };
    repo::upsert_event(conn, &ev)?;

    sessions::open_session(conn, &session_id, now, cwd.as_deref())?;
    if let Some(reason) = close_reason(&event) {
        sessions::close_session(conn, &session_id, now, reason)?;
    }
    sessions::reap_stale(conn, now)?;

    Ok(())
}

/// CLI entrypoint. Reads stdin, opens db, handles. Always returns Ok —
/// hook-side errors are logged to stderr but never propagated (Claude
/// must never be blocked by a worklog failure).
pub fn run_from_stdin() -> Result<()> {
    let mut buf = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
        warn!("reading stdin failed: {e}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use chrono::TimeZone;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 18, 9, 30, 0).unwrap()
    }

    #[test]
    fn session_start_inserts_event_and_session_row() {
        let conn = open_memory().unwrap();
        let payload = json!({
            "hook_event_name": "SessionStart",
            "session_id": "abc-123",
            "cwd": "/Users/tomas/project",
            "user_prompt": "look at PROJ-42"
        });
        handle(&conn, &payload, now()).unwrap();

        // Event written with source=claude and jira key extracted.
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "claude");
        assert_eq!(events[0].jira_issue.as_deref(), Some("PROJ-42"));
        assert_eq!(events[0].session_id.as_deref(), Some("abc-123"));
        assert!(events[0].title.starts_with("SessionStart —"));

        // Sessions row exists with event_count = 1.
        let count: i64 = conn
            .query_row(
                "SELECT event_count FROM sessions WHERE session_id = ?1",
                rusqlite::params!["abc-123"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn stop_event_closes_session() {
        let conn = open_memory().unwrap();
        handle(
            &conn,
            &json!({
                "hook_event_name": "SessionStart",
                "session_id": "x",
                "cwd": "/p"
            }),
            now(),
        )
        .unwrap();
        handle(
            &conn,
            &json!({
                "hook_event_name": "Stop",
                "session_id": "x"
            }),
            now() + chrono::Duration::minutes(15),
        )
        .unwrap();

        let (ended_at, end_source): (String, String) = conn
            .query_row(
                "SELECT ended_at, end_source FROM sessions WHERE session_id = ?1",
                rusqlite::params!["x"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(ended_at.starts_with("2026-04-18T09:45"));
        assert_eq!(end_source, "stop");
    }

    #[test]
    fn dedupe_is_keyed_on_session_event_timestamp() {
        let conn = open_memory().unwrap();
        let p = json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s1" });
        handle(&conn, &p, now()).unwrap();
        handle(&conn, &p, now()).unwrap(); // same timestamp → same source_id → upsert, not duplicate
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn handle_does_not_panic_on_missing_fields() {
        let conn = open_memory().unwrap();
        // Only a session id — everything else is missing.
        let p = json!({ "session_id": "bare" });
        handle(&conn, &p, now()).unwrap();
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "unknown");
    }
}
