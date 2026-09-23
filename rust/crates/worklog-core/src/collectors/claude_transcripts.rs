//! Claude Code transcript collector.
//!
//! Claude Code keeps a per-session JSON-lines transcript at
//! `~/.claude/projects/<dir>/<sessionId>.jsonl` — one line per turn, each a
//! JSON object with `type` ("user"/"assistant"/...), `timestamp` (ISO
//! UTC), `cwd`, `sessionId`, `uuid`. This collector turns every real user
//! prompt (never a `tool_result` reply relayed back as a "user" line) into
//! one event, source `claude_turn`. Fixes the "afternoon is invisible"
//! defect: the `claude` hook only records session start/end, so a long
//! autonomous session with hundreds of turns could show as two events.
//!
//! PRIVACY: only the fact that a prompt happened is recorded — the prompt
//! text (`message.content`) is never read into an `Event` field. `title`
//! is always the literal `"prompt"`; `details` is always `None`.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

use crate::collectors::fish::repo_root_for;
use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Collect from the default transcripts root (`~/.claude/projects`).
/// Missing directory is not an error.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "claude_turn",
            ..Default::default()
        });
    };
    collect_from_dir(conn, &home.join(".claude/projects"), since, until)
}

pub fn collect_from_dir(
    conn: &Connection,
    dir: &Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "claude_turn",
        ..Default::default()
    };
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());

    let since_ts = since.and_time(NaiveTime::MIN).and_utc().timestamp();
    let until_ts = until.and_time(NaiveTime::MIN).and_utc().timestamp();

    let Ok(project_dirs) = std::fs::read_dir(dir) else {
        return Ok(report);
    };
    for project_entry in project_dirs.flatten() {
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&project_dir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(meta) = file_entry.metadata() else {
                continue;
            };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            let modified_ts = modified
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if modified_ts < since_ts || modified_ts >= until_ts {
                continue;
            }
            collect_file(
                conn,
                &path,
                since_ts,
                until_ts,
                home.as_deref(),
                &mut report,
            )?;
        }
    }
    Ok(report)
}

fn collect_file(
    conn: &Connection,
    path: &Path,
    since_ts: i64,
    until_ts: i64,
    home: Option<&str>,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(());
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        if !is_real_prompt(message) {
            continue;
        }
        let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) else {
            continue;
        };
        let ts_utc: DateTime<Utc> = parsed.with_timezone(&Utc);
        let epoch = ts_utc.timestamp();
        if epoch < since_ts || epoch >= until_ts {
            continue;
        }
        let Some(session_id) = value.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        let Some(uuid) = value.get("uuid").and_then(Value::as_str) else {
            continue;
        };
        let project_path = value
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(|cwd| repo_root_for(cwd, home));

        let ev = Event {
            id: None,
            source: "claude_turn".into(),
            source_id: format!("{session_id}:{uuid}"),
            started_at: ts_utc.to_rfc3339(),
            ended_at: None,
            duration_seconds: None,
            title: "prompt".into(),
            details: None,
            repo: None,
            project_path,
            jira_issue: None,
            session_id: Some(session_id.to_string()),
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    Ok(())
}

/// A "real" user prompt: plain string content, or a content array that
/// isn't made up entirely of `tool_result` blocks (a `tool_result`-only
/// array is Claude Code relaying a tool's output back as a synthetic
/// "user" turn, never something the owner typed).
fn is_real_prompt(message: &Value) -> bool {
    match message.get("content") {
        Some(Value::String(_)) => true,
        Some(Value::Array(items)) => {
            !items.is_empty()
                && !items
                    .iter()
                    .all(|item| item.get("type").and_then(Value::as_str) == Some("tool_result"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn write_transcript(dir: &Path, project: &str, session: &str, lines: &str) {
        let project_dir = dir.join(project);
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join(format!("{session}.jsonl")), lines).unwrap();
    }

    fn user_line(ts: &str, session: &str, uuid: &str, cwd: &str, content: &str) -> String {
        format!(
            r#"{{"type":"user","timestamp":"{ts}","sessionId":"{session}","uuid":"{uuid}","cwd":"{cwd}","message":{{"role":"user","content":{content}}}}}"#
        )
    }

    #[test]
    fn counts_a_real_string_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
        let cwd = format!("{home}/Desktop/Work/widget");
        let line = user_line("2026-04-18T09:00:00Z", "s1", "u1", &cwd, "\"fix the bug\"");
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 1);
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "claude_turn");
        assert_eq!(events[0].title, "prompt");
        assert_eq!(events[0].details, None);
        assert_eq!(events[0].source_id, "s1:u1");
        assert_eq!(events[0].session_id.as_deref(), Some("s1"));
        assert_eq!(events[0].project_path.as_deref(), Some(cwd.as_str()));
        assert_eq!(events[0].raw_json, None);
        assert_eq!(events[0].repo, None);
        assert_eq!(events[0].jira_issue, None);
        assert_eq!(events[0].tempo_worklog_id, None);
    }

    #[test]
    fn counts_a_prompt_with_text_content_items() {
        let tmp = tempfile::tempdir().unwrap();
        let line = user_line(
            "2026-04-18T09:00:00Z",
            "s1",
            "u1",
            "/home/x/Desktop/Work/widget",
            r#"[{"type":"text","text":"hello"}]"#,
        );
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 1);
    }

    #[test]
    fn skips_tool_result_only_user_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let line = user_line(
            "2026-04-18T09:00:00Z",
            "s1",
            "u1",
            "/home/x/Desktop/Work/widget",
            r#"[{"type":"tool_result","content":"some output"}]"#,
        );
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 0);
    }

    #[test]
    fn skips_assistant_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let line = r#"{"type":"assistant","timestamp":"2026-04-18T09:00:00Z","sessionId":"s1","uuid":"u1","cwd":"/home/x/Desktop/Work/widget","message":{"role":"assistant","content":[{"type":"text","text":"sure"}]}}"#;
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 0);
    }

    #[test]
    fn skips_lines_outside_the_time_window() {
        let tmp = tempfile::tempdir().unwrap();
        let line = user_line(
            "2020-01-01T09:00:00Z",
            "s1",
            "u1",
            "/home/x/Desktop/Work/widget",
            "\"old prompt\"",
        );
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 0);
    }

    #[test]
    fn worktree_cwd_collapses_to_repo_root() {
        let tmp = tempfile::tempdir().unwrap();
        let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
        let cwd = format!("{home}/Desktop/Work/vitinn-infra/.claude/worktrees/sandbox-runner");
        let line = user_line("2026-04-18T09:00:00Z", "s1", "u1", &cwd, "\"do the thing\"");
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(
            events[0].project_path.as_deref(),
            Some(format!("{home}/Desktop/Work/vitinn-infra").as_str())
        );
    }

    #[test]
    fn never_stores_prompt_text() {
        let tmp = tempfile::tempdir().unwrap();
        let line = user_line(
            "2026-04-18T09:00:00Z",
            "s1",
            "u1",
            "/home/x/Desktop/Work/widget",
            "\"the secret plan is XYZZY\"",
        );
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        let ev = &events[0];
        assert_eq!(ev.title, "prompt");
        assert_eq!(ev.details, None);
        assert_eq!(ev.raw_json, None);
        let haystack = format!(
            "{}{}{}{}",
            ev.title,
            ev.details.clone().unwrap_or_default(),
            ev.project_path.clone().unwrap_or_default(),
            ev.session_id.clone().unwrap_or_default()
        );
        assert!(!haystack.contains("XYZZY"));
        assert!(!haystack.contains("secret plan"));
    }

    #[test]
    fn re_run_inserts_no_new_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let line = user_line(
            "2026-04-18T09:00:00Z",
            "s1",
            "u1",
            "/home/x/Desktop/Work/widget",
            "\"fix the bug\"",
        );
        write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
        collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
        assert_eq!(report.events_written, 1, "upsert still counts as written");
        let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
        assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
    }
}
