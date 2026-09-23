//! Git reflog collector.
//!
//! Reads `.git/logs/HEAD` directly for every repo directly under a set of
//! root directories (default `~/Desktop/Work` and `~/Desktop/Projects`).
//! Never invokes the `git` CLI. PRIVACY: only the reflog action and its
//! target ref are kept (e.g. `checkout <branch>`, `commit`, `merge
//! <branch>`) — the rest of the reflog message (which can embed a commit
//! subject line) is discarded.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, NaiveTime};
use rusqlite::Connection;
use std::path::Path;

use crate::models::Event;
use crate::repo;

use super::CollectReport;

/// Collect from the default roots. A missing root is not an error.
pub fn collect(conn: &Connection, since: NaiveDate, until: NaiveDate) -> Result<CollectReport> {
    let Some(home) = dirs::home_dir() else {
        return Ok(CollectReport {
            source: "git_reflog",
            ..Default::default()
        });
    };
    let roots = [home.join("Desktop/Work"), home.join("Desktop/Projects")];
    collect_from_roots(conn, &roots, since, until)
}

pub fn collect_from_roots(
    conn: &Connection,
    roots: &[std::path::PathBuf],
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectReport> {
    let mut report = CollectReport {
        source: "git_reflog",
        ..Default::default()
    };

    let since_ts = since.and_time(NaiveTime::MIN).and_utc().timestamp();
    let until_ts = until.and_time(NaiveTime::MIN).and_utc().timestamp();

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let repo_dir = entry.path();
            if !repo_dir.is_dir() {
                continue;
            }
            collect_repo(conn, &repo_dir, since_ts, until_ts, &mut report)?;
        }
    }

    Ok(report)
}

fn collect_repo(
    conn: &Connection,
    repo_dir: &Path,
    since_ts: i64,
    until_ts: i64,
    report: &mut CollectReport,
) -> Result<()> {
    let head_log = repo_dir.join(".git/logs/HEAD");
    let Ok(content) = std::fs::read_to_string(&head_log) else {
        return Ok(());
    };
    let repo_name = repo_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let project_path = repo_dir.to_string_lossy().into_owned();

    for line in content.lines() {
        let Some((meta, message)) = line.split_once('\t') else {
            continue;
        };
        let tokens: Vec<&str> = meta.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let new_sha = tokens[1];
        let Ok(epoch) = tokens[tokens.len() - 2].parse::<i64>() else {
            continue;
        };
        if epoch < since_ts || epoch >= until_ts {
            continue;
        }
        let Some(started_at) = DateTime::from_timestamp(epoch, 0).map(|dt| dt.to_rfc3339()) else {
            continue;
        };

        let ev = Event {
            id: None,
            source: "git_reflog".into(),
            source_id: format!("{repo_name}:{epoch}:{new_sha}"),
            started_at,
            ended_at: None,
            duration_seconds: None,
            title: title_for_reflog_message(message),
            details: None,
            repo: None,
            project_path: Some(project_path.clone()),
            jira_issue: None,
            session_id: None,
            tempo_worklog_id: None,
            raw_json: None,
        };
        repo::upsert_event(conn, &ev)?;
        report.events_written += 1;
    }

    Ok(())
}

/// Extract only the action + target ref from a reflog message, never the
/// full message (which for `commit`/`merge`/`pull` can embed a commit
/// subject line or free text).
fn title_for_reflog_message(message: &str) -> String {
    let message = message.trim();
    if let Some(rest) = message.strip_prefix("checkout:") {
        if let Some(idx) = rest.rfind(" to ") {
            let branch = rest[idx + 4..].trim();
            if !branch.is_empty() {
                return format!("checkout {branch}");
            }
        }
        return "checkout".to_string();
    }
    if let Some(rest) = message.strip_prefix("merge ") {
        let branch = rest.split(':').next().unwrap_or("").trim();
        if !branch.is_empty() {
            return format!("merge {branch}");
        }
        return "merge".to_string();
    }
    if message.starts_with("commit") {
        return "commit".to_string();
    }
    if message.starts_with("rebase") {
        return "rebase".to_string();
    }
    if message.starts_with("pull") {
        return "pull".to_string();
    }
    if message.starts_with("reset") {
        return "reset".to_string();
    }
    message
        .split(|c: char| c == ':' || c.is_whitespace())
        .next()
        .unwrap_or("other")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn write_reflog(repo_dir: &Path, lines: &str) {
        let git_dir = repo_dir.join(".git/logs");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), lines).unwrap();
    }

    #[test]
    fn parses_two_repos_and_filters_by_time_range() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Work");
        std::fs::create_dir_all(&root).unwrap();

        let repo_a = root.join("repo-a");
        std::fs::create_dir_all(&repo_a).unwrap();
        write_reflog(
            &repo_a,
            "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit (initial): first\n",
        );

        let repo_b = root.join("repo-b");
        std::fs::create_dir_all(&repo_b).unwrap();
        write_reflog(
            &repo_b,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc Tomas Palsson <t@example.com> 1800000000 +0000\tcommit: later\n",
        );

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        let report = collect_from_roots(&conn, &[root], since, until).unwrap();
        assert_eq!(report.events_written, 1);
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "commit");
        assert_eq!(
            events[0].project_path.as_deref(),
            Some(repo_a.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn titles_carry_no_commit_message_text() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Work");
        std::fs::create_dir_all(&root).unwrap();
        let repo_dir = root.join("repo-a");
        std::fs::create_dir_all(&repo_dir).unwrap();
        write_reflog(
            &repo_dir,
            "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcheckout: moving from main to secret-project-x\n",
        );

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        collect_from_roots(&conn, &[root], since, until).unwrap();
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events[0].title, "checkout secret-project-x");
        assert_eq!(events[0].details, None);
    }

    #[test]
    fn re_run_inserts_no_new_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Work");
        std::fs::create_dir_all(&root).unwrap();
        let repo_dir = root.join("repo-a");
        std::fs::create_dir_all(&repo_dir).unwrap();
        write_reflog(
            &repo_dir,
            "0000000000000000000000000000000000000000 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa Tomas Palsson <t@example.com> 1700000000 +0000\tcommit: x\n",
        );

        let conn = open_memory().unwrap();
        let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
        collect_from_roots(&conn, std::slice::from_ref(&root), since, until).unwrap();
        let report = collect_from_roots(&conn, &[root], since, until).unwrap();
        assert_eq!(report.events_written, 1, "upsert still counts as written");
        let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
        assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
    }
}
