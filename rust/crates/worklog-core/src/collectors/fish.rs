//! Fish shell history collector.
//!
//! Reads `~/.local/share/fish/fish_history` (format: repeating blocks of
//! `- cmd: <text>`, `  when: <epoch>`, optional `  paths:` + indented
//! path lines). PRIVACY: the command text is only used in-process to
//! track `cd` and pick a program name — it is never written to the DB.
//! `title` is the first whitespace token of the command; `details` is
//! always `None`.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime};
use rusqlite::Connection;

use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Collect from the default fish history location. Missing file is not
/// an error — most machines running this collector won't use fish.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "shell",
            ..Default::default()
        });
    };
    let path = home.join(".local/share/fish/fish_history");
    collect_from_path(conn, &path, since, until)
}

pub fn collect_from_path(
    conn: &Connection,
    path: &std::path::Path,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "shell",
        ..Default::default()
    };

    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(report);
    };

    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    let since_ts = since.and_time(NaiveTime::MIN).and_utc().timestamp();
    let until_ts = until.and_time(NaiveTime::MIN).and_utc().timestamp();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut entry_index: usize = 0;
    let mut cwd: Option<String> = None;

    while i < lines.len() {
        let Some(cmd) = lines[i].strip_prefix("- cmd: ") else {
            i += 1;
            continue;
        };
        let cmd = cmd.to_string();
        let idx = entry_index;
        entry_index += 1;
        i += 1;

        let mut when: Option<i64> = None;
        if let Some(w) = lines.get(i).and_then(|l| l.strip_prefix("  when: ")) {
            when = w.trim().parse::<i64>().ok();
            i += 1;
        }
        if lines.get(i).map(|l| l.trim_start()) == Some("paths:") {
            i += 1;
            while lines.get(i).is_some_and(|l| l.starts_with("    - ")) {
                i += 1;
            }
        }

        let Some(when) = when else { continue };

        let first_token = cmd.split_whitespace().next().unwrap_or("");
        if first_token == "cd" {
            let arg = cmd["cd".len()..].trim();
            cwd = resolve_cwd(cwd.as_deref(), arg, home.as_deref());
        }

        if when < since_ts || when >= until_ts {
            continue;
        }

        let Some(started_at) = DateTime::from_timestamp(when, 0).map(|dt| dt.to_rfc3339()) else {
            continue;
        };
        let project_path = cwd
            .as_deref()
            .and_then(|c| repo_root_for(c, home.as_deref()));

        let ev = Event {
            id: None,
            source: "shell".into(),
            source_id: format!("{when}:{idx}"),
            started_at,
            ended_at: None,
            duration_seconds: None,
            title: first_token.to_string(),
            details: None,
            repo: None,
            project_path,
            jira_issue: None,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    Ok(report)
}

/// Resolve a `cd` argument (absolute, `~`-relative, or relative to
/// `cwd`) into a new absolute cwd. `None` when relative and no base is
/// known yet.
fn resolve_cwd(cwd: Option<&str>, arg: &str, home: Option<&str>) -> Option<String> {
    let target = if arg.is_empty() {
        home.map(str::to_owned)
    } else if let Some(rest) = arg.strip_prefix('~') {
        home.map(|h| format!("{h}{rest}"))
    } else if let Some(rest) = arg.strip_prefix('/') {
        Some(format!("/{rest}"))
    } else {
        cwd.map(|base| format!("{base}/{arg}"))
    };
    target.map(|t| normalize_path(&t))
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The repo root under `~/Desktop/Work/<key>` or `~/Desktop/Projects/<key>`
/// for `path`, collapsing `/.claude/worktrees/*` scaffolding first
/// (mirrors `billing::work_folder_for_path`'s worktree collapse).
fn repo_root_for(path: &str, home: Option<&str>) -> Option<String> {
    let home = home?;
    let base = match path.find("/.claude/") {
        Some(i) => &path[..i],
        None => path,
    };
    for root_name in ["Desktop/Work", "Desktop/Projects"] {
        let prefix = format!("{home}/{root_name}");
        let Some(rest) = base.strip_prefix(&prefix) else {
            continue;
        };
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            continue;
        }
        let first = rest.split('/').next().unwrap_or(rest);
        if !first.is_empty() {
            return Some(format!("{prefix}/{first}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use std::io::Write;

    fn home() -> String {
        dirs::home_dir().unwrap().to_string_lossy().into_owned()
    }

    fn write_fixture(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_entries_and_filters_by_time_range() {
        let fixture = write_fixture(
            "- cmd: ls -la\n  when: 1700000000\n- cmd: echo hi\n  when: 1800000000\n",
        );
        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        let report = collect_from_path(&conn, fixture.path(), since, until).unwrap();
        assert_eq!(report.events_written, 1);
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "ls");
    }

    #[test]
    fn never_stores_command_text() {
        let fixture = write_fixture("- cmd: git commit -m secret-plan\n  when: 1700000000\n");
        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        collect_from_path(&conn, fixture.path(), since, until).unwrap();
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events[0].title, "git");
        assert_eq!(events[0].details, None);
    }

    #[test]
    fn cd_tracking_attributes_a_later_command_to_the_repo() {
        let repo_dir = format!("{}/Desktop/Work/widget", home());
        let fixture = write_fixture(&format!(
            "- cmd: cd {repo_dir}\n  when: 1700000000\n- cmd: cargo test\n  when: 1700000100\n",
        ));
        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        collect_from_path(&conn, fixture.path(), since, until).unwrap();
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        let cargo_ev = events.iter().find(|e| e.title == "cargo").unwrap();
        assert_eq!(cargo_ev.project_path.as_deref(), Some(repo_dir.as_str()));
    }

    #[test]
    fn re_run_inserts_no_new_rows() {
        let fixture = write_fixture("- cmd: ls\n  when: 1700000000\n");
        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        collect_from_path(&conn, fixture.path(), since, until).unwrap();
        let report = collect_from_path(&conn, fixture.path(), since, until).unwrap();
        assert_eq!(report.events_written, 1, "upsert still counts as written");
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
    }
}
