//! Minimal `.env` reader/writer for the handful of plain-text settings
//! that aren't secrets and aren't worth a full config format.
//!
//! Today this holds exactly one key — `WORKLOG_TZ` — written by the
//! settings panel and read back by [`crate::tz`] as a fallback when the
//! process environment doesn't set it. Secrets do NOT live here; they go
//! to the OS keychain via [`crate::secrets`]. The file is shared with the
//! Python-era credential `.env`, so `upsert` is careful to preserve every
//! other line it doesn't own.
//!
//! Path resolution mirrors `secrets::read_env_file`: the `WORKLOG_ENV_FILE`
//! override wins, otherwise `~/.config/worklog/.env`.

use std::path::PathBuf;

use anyhow::{Context, Result};

const ENV_FILE_PATH_OVERRIDE: &str = "WORKLOG_ENV_FILE";

/// Resolve the `.env` path. `None` only when there's no home directory
/// and no override — a degenerate environment we can't write into.
pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV_FILE_PATH_OVERRIDE) {
        return Some(PathBuf::from(p));
    }
    dirs::home_dir().map(|h| h.join(".config/worklog/.env"))
}

/// Read a single `NAME=value` entry, stripping surrounding quotes.
/// Returns `None` if the file or key is missing.
pub fn read(name: &str) -> Option<String> {
    let path = path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == name {
            return Some(strip_quotes(v.trim()).to_owned());
        }
    }
    None
}

/// Insert or replace `NAME=value`, leaving every other line untouched.
/// Passing an empty `value` removes the key entirely so the env-var
/// fallback chain (process env → this file → default) collapses cleanly.
/// Creates the file and parent directory if missing.
pub fn upsert(name: &str, value: &str) -> Result<()> {
    let path = path().context("no env-file path (no home directory)")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        let is_target = trimmed
            .split_once('=')
            .is_some_and(|(k, _)| k.trim() == name)
            && !trimmed.starts_with('#');
        if is_target {
            if value.is_empty() {
                // drop the line entirely
                continue;
            }
            out.push(format!("{name}={value}"));
            replaced = true;
        } else {
            out.push(line.to_owned());
        }
    }
    if !replaced && !value.is_empty() {
        out.push(format!("{name}={value}"));
    }

    let mut body = out.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// The single crate-wide guard for tests that manipulate the
/// process-global environment the env-file and the pruner read:
/// `WORKLOG_ENV_FILE`, `WORKLOG_PRUNE_ENABLED`, `WORKLOG_BILLING_*`.
///
/// One lock, deliberately. Per-module locks do not exclude each other,
/// and that is not a theoretical concern: `daemon.rs`'s settings tests
/// and this module's own tests both set `WORKLOG_ENV_FILE` and raced,
/// and separately a `WORKLOG_HOME` race once pointed a prune at the
/// owner's real database. Anything here that reaches `std::env` in a
/// test must take THIS lock.
///
/// Async-aware so `#[tokio::test]` callers can hold it across `.await`
/// without tripping `clippy::await_holding_lock`; synchronous `#[test]`
/// callers use `blocking_lock()`, which is safe outside a runtime.
#[cfg(test)]
pub(crate) static ENV_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // WORKLOG_ENV_FILE is process-global. Takes the crate-wide
    // `ENV_TEST_LOCK` rather than a private one, so these tests exclude
    // `daemon.rs`'s settings tests and `purge.rs`'s env tests too — they
    // all write the same variable.
    fn lock() -> tokio::sync::MutexGuard<'static, ()> {
        ENV_TEST_LOCK.blocking_lock()
    }

    #[test]
    fn upsert_then_read_round_trips() {
        let _g = lock();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".env");
        std::env::set_var(ENV_FILE_PATH_OVERRIDE, &f);

        assert_eq!(read("WORKLOG_TZ"), None);
        upsert("WORKLOG_TZ", "+01:00").unwrap();
        assert_eq!(read("WORKLOG_TZ").as_deref(), Some("+01:00"));

        std::env::remove_var(ENV_FILE_PATH_OVERRIDE);
    }

    #[test]
    fn upsert_replaces_without_touching_other_keys() {
        let _g = lock();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".env");
        std::fs::write(&f, "WORKLOG_JIRA_EMAIL=t@p5.is\nWORKLOG_TZ=UTC\n").unwrap();
        std::env::set_var(ENV_FILE_PATH_OVERRIDE, &f);

        upsert("WORKLOG_TZ", "-05:00").unwrap();
        let body = std::fs::read_to_string(&f).unwrap();
        assert!(
            body.contains("WORKLOG_JIRA_EMAIL=t@p5.is"),
            "other key preserved: {body}"
        );
        assert!(
            body.contains("WORKLOG_TZ=-05:00"),
            "target replaced: {body}"
        );
        assert_eq!(
            body.matches("WORKLOG_TZ=").count(),
            1,
            "no duplicate: {body}"
        );

        std::env::remove_var(ENV_FILE_PATH_OVERRIDE);
    }

    #[test]
    fn upsert_empty_value_removes_key() {
        let _g = lock();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".env");
        std::fs::write(&f, "WORKLOG_JIRA_EMAIL=t@p5.is\nWORKLOG_TZ=UTC\n").unwrap();
        std::env::set_var(ENV_FILE_PATH_OVERRIDE, &f);

        upsert("WORKLOG_TZ", "").unwrap();
        let body = std::fs::read_to_string(&f).unwrap();
        assert!(!body.contains("WORKLOG_TZ"), "key removed: {body}");
        assert!(
            body.contains("WORKLOG_JIRA_EMAIL"),
            "other key kept: {body}"
        );

        std::env::remove_var(ENV_FILE_PATH_OVERRIDE);
    }
}
