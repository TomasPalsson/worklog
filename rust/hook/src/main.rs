//! worklog-hook — fast Claude Code hook.
//!
//! Reads a JSON event on stdin, writes an events row + maintains sessions,
//! exits 0 no matter what. Never prints to stdout.

mod classify;
mod config;
mod db;
mod sessions;

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;
use std::io::Read;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::classify::classify;
use crate::config::{load_companies, Paths};
use crate::sessions::{close_session, open_session, reap_stale, REAPER_TTL_SECONDS};

static JIRA_RE: OnceLock<regex::Regex> = OnceLock::new();

fn jira_re() -> &'static regex::Regex {
    JIRA_RE.get_or_init(|| regex::Regex::new(r"\b([A-Z][A-Z0-9]{1,9}-\d+)\b").unwrap())
}

#[derive(Debug, Deserialize)]
struct Payload {
    #[serde(default, rename = "hook_event_name")]
    hook_event_name: Option<String>,
    #[serde(default, alias = "event")]
    event: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    project_path: Option<String>,
    #[serde(default)]
    transcript_path: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    user_prompt: Option<String>,
}

impl Payload {
    fn event_name(&self) -> &str {
        self.hook_event_name
            .as_deref()
            .or(self.event.as_deref())
            .unwrap_or("unknown")
    }
    fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref().or(self.project_path.as_deref())
    }
    fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref().or(self.user_prompt.as_deref())
    }
}

fn close_reason_for(event: &str) -> Option<&'static str> {
    match event {
        "Stop" => Some("stop"),
        "SessionEnd" => Some("session_end"),
        _ => None,
    }
}

fn first_jira_key(parts: &[Option<&str>]) -> Option<String> {
    let re = jira_re();
    for p in parts.iter().copied().flatten() {
        if let Some(m) = re.captures(p).and_then(|c| c.get(1)) {
            return Some(m.as_str().to_string());
        }
    }
    None
}

fn now_utc_iso(offset_seconds: i64) -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64 + offset_seconds;
    let nanos = d.subsec_nanos();
    format_iso(secs, nanos)
}

fn format_iso(unix_secs: i64, nanos: u32) -> String {
    // Dependency-free UTC ISO-8601 with microsecond precision.
    // Date math: Howard Hinnant's civil_from_days.
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day / 60) % 60;
    let second = secs_of_day % 60;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    let us = nanos / 1000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{us:06}+00:00")
}

struct EventRow<'a> {
    source_id: &'a str,
    started_at: &'a str,
    title: &'a str,
    details: Option<&'a str>,
    project_path: Option<&'a str>,
    jira_issue: Option<&'a str>,
    company: Option<&'a str>,
    session_id: &'a str,
    raw_json: &'a str,
}

fn upsert_event(conn: &rusqlite::Connection, row: &EventRow<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO events (source, source_id, started_at, title, details,
                             project_path, jira_issue, company, session_id, raw_json)
         VALUES ('claude', ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(source, source_id) DO UPDATE SET
            title = excluded.title,
            details = excluded.details,
            project_path = excluded.project_path,
            jira_issue = excluded.jira_issue,
            company = COALESCE(events.company, excluded.company),
            session_id = COALESCE(events.session_id, excluded.session_id),
            raw_json = excluded.raw_json",
        params![
            row.source_id,
            row.started_at,
            row.title,
            row.details,
            row.project_path,
            row.jira_issue,
            row.company,
            row.session_id,
            row.raw_json,
        ],
    )?;
    Ok(())
}

fn run() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("read stdin")?;
    let payload: Payload = serde_json::from_str(&raw).context("parse hook JSON")?;

    let event = payload.event_name().to_string();
    let session_id = payload
        .session_id
        .clone()
        .unwrap_or_else(|| "no-session".into());
    let cwd = payload.cwd().map(str::to_string);
    let prompt = payload.prompt().map(str::to_string);

    let jira = first_jira_key(&[prompt.as_deref(), cwd.as_deref()]);

    let paths = Paths::resolve();
    let companies = load_companies(&paths.companies_yaml).unwrap_or_default();
    let company = classify(&companies, cwd.as_deref(), jira.as_deref());

    let now = now_utc_iso(0);
    let cutoff = now_utc_iso(-REAPER_TTL_SECONDS);

    let source_id = format!("{session_id}:{event}:{now}");
    let title = match prompt.as_deref() {
        Some(p) if !p.is_empty() => {
            let truncated: String = p.chars().take(80).collect();
            format!("{event} — {truncated}")
        }
        _ => event.clone(),
    };

    let conn = db::open(&paths.db)?;
    upsert_event(
        &conn,
        &EventRow {
            source_id: &source_id,
            started_at: &now,
            title: &title,
            details: payload.transcript_path.as_deref(),
            project_path: cwd.as_deref(),
            jira_issue: jira.as_deref(),
            company: company.as_deref(),
            session_id: &session_id,
            raw_json: &raw,
        },
    )?;
    open_session(&conn, &session_id, &now, cwd.as_deref())?;
    if let Some(reason) = close_reason_for(&event) {
        close_session(&conn, &session_id, &now, reason)?;
    }
    reap_stale(&conn, &cutoff)?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[worklog-hook] warn: {e:#}");
    }
    // Always exit 0.
}
