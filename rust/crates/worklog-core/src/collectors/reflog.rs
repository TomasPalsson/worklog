//! Git reflog collector.
//!
//! Reads `.git/logs/HEAD` directly for every repo directly under a set of
//! root directories (default `~/Desktop/Work` and `~/Desktop/Projects`),
//! plus every worktree's `.git/worktrees/<name>/logs/HEAD` and every
//! submodule's `.git/modules/<path>/logs/HEAD` (recursively, for nested
//! submodules) — all attributed to the main repo's root, since a
//! worktree/submodule checkout is still the same project for billing
//! purposes. Never invokes the `git` CLI. PRIVACY: only the reflog action
//! and its target ref are kept (e.g. `checkout <branch>`, `commit`, `merge
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
    let git_dir = repo_dir.join(".git");
    if git_dir.is_file() {
        // A worktree checkout's `.git` is a file pointing at the main
        // repo's gitdir, not a directory of its own — its reflog is read
        // from the main repo's `.git/worktrees/<name>/logs/HEAD` below, so
        // scanning it here would just be a redundant (and gitdir-relative)
        // re-read of the same log.
        return Ok(());
    }
    let project_path = repo_dir.to_string_lossy().into_owned();

    collect_log_file(
        conn,
        &git_dir.join("logs/HEAD"),
        &project_path,
        "",
        since_ts,
        until_ts,
        report,
    )?;

    if let Ok(entries) = std::fs::read_dir(git_dir.join("worktrees")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            collect_log_file(
                conn,
                &entry.path().join("logs/HEAD"),
                &project_path,
                &format!(":wt-{name}"),
                since_ts,
                until_ts,
                report,
            )?;
        }
    }

    let modules_dir = git_dir.join("modules");
    for (name, head_log) in find_module_logs(&modules_dir, &modules_dir) {
        collect_log_file(
            conn,
            &head_log,
            &project_path,
            &format!(":sm-{name}"),
            since_ts,
            until_ts,
            report,
        )?;
    }

    Ok(())
}

/// Recursively find every `logs/HEAD` under `dir` (a `.git/modules` tree —
/// submodules can nest arbitrarily, a submodule's own submodules living
/// under its `modules/<name>/modules/...`), paired with the submodule's
/// path relative to `modules_root` (e.g. `tools/code-interpreter`) for use
/// as a unique `source_id` suffix.
fn find_module_logs(dir: &Path, modules_root: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut logs = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return logs;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let head_log = path.join("logs/HEAD");
        if head_log.is_file() {
            let name = path
                .strip_prefix(modules_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            logs.push((name, head_log));
            // A submodule's own submodules live under its gitdir's
            // `modules/<name>`, not by continuing to walk its refs/objects.
            logs.extend(find_module_logs(&path.join("modules"), modules_root));
        } else {
            // Not yet a submodule gitdir — just an intermediate path
            // segment (e.g. `tools` in a `tools/code-interpreter`
            // submodule path) — keep descending to find the real one.
            logs.extend(find_module_logs(&path, modules_root));
        }
    }
    logs
}

/// Parse one reflog file (`logs/HEAD` for the main repo, a worktree, or a
/// submodule) and upsert every entry in `[since_ts, until_ts)`, all
/// attributed to the main repo root `project_path`. `id_suffix` keeps
/// worktree/submodule entries from colliding with the main log's
/// `source_id` for the same epoch + sha.
fn collect_log_file(
    conn: &Connection,
    head_log: &Path,
    project_path: &str,
    id_suffix: &str,
    since_ts: i64,
    until_ts: i64,
    report: &mut CollectReport,
) -> Result<()> {
    let Ok(content) = std::fs::read_to_string(head_log) else {
        return Ok(());
    };
    let repo_name = Path::new(project_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

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
            source_id: format!("{repo_name}{id_suffix}:{epoch}:{new_sha}"),
            started_at,
            ended_at: None,
            duration_seconds: None,
            title: title_for_reflog_message(message),
            details: None,
            repo: None,
            project_path: Some(project_path.to_owned()),
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

// Tests live in reflog_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "reflog_test.rs"]
mod tests;
