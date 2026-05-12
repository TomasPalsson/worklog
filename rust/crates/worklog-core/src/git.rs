//! Per-block git sidecar — list commits that landed inside a block's
//! time window.
//!
//! Pure read path. Shells out to `git -C <cwd> log --since=... --until=...`
//! and returns nothing on any failure (corrupt repo, missing binary,
//! timeout, deleted cwd). The daemon route is purely additive — the UI
//! treats `[]` the same whether the lookup failed or the window simply
//! contained no commits, so soft failures are the right default.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// One commit inside a block's window. The web UI renders these as
/// rows; `github_url` is omitted when the repo's `origin` doesn't point
/// at GitHub (no link button shown).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitEntry {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author_email: String,
    /// ISO-8601 with offset (`%cI` from git).
    pub committed_at: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_url: Option<String>,
}

const TIMEOUT: Duration = Duration::from_secs(10);

// Field/record separators inside the `git log --pretty=format:` template.
// We split on these later to recover structured fields without worrying
// about subjects that contain commas or quotes.
const FS: char = '\x1f';
const RS: char = '\x1e';

/// List commits that landed under `cwd` between `since` and `until`
/// (both ISO-8601 UTC, inclusive endpoints per `git log`'s semantics).
///
/// Returns `Ok(vec![])` for any soft failure: cwd is not a git repo,
/// git binary missing, command timed out, or no commits in the window.
/// Each soft failure logs a `tracing::warn!` once.
pub async fn git_log_in_window(cwd: &Path, since: &str, until: &str) -> Result<Vec<CommitEntry>> {
    let cwd_owned: PathBuf = cwd.to_path_buf();
    let since_owned = since.to_owned();
    let until_owned = until.to_owned();

    // spawn_blocking + tokio::time::timeout: the child process itself
    // isn't cancellable from the outer future, but on timeout we drop
    // the JoinHandle and stop waiting. The blocking task continues until
    // git returns; that's fine — git log on a single repo is bounded.
    let join =
        tokio::task::spawn_blocking(move || run_git_log(&cwd_owned, &since_owned, &until_owned));

    match tokio::time::timeout(TIMEOUT, join).await {
        Ok(Ok(Ok(out))) => Ok(out),
        Ok(Ok(Err(e))) => {
            warn!(cwd = %cwd.display(), error = %e, "git log failed — returning empty");
            Ok(vec![])
        }
        Ok(Err(e)) => {
            warn!(cwd = %cwd.display(), error = %e, "git log task panicked");
            Ok(vec![])
        }
        Err(_) => {
            warn!(cwd = %cwd.display(), "git log timed out after {:?}", TIMEOUT);
            Ok(vec![])
        }
    }
}

/// Synchronous body — shells out and parses. Lives in its own function
/// so tests can exercise the integration path directly via a runtime.
fn run_git_log(cwd: &Path, since: &str, until: &str) -> Result<Vec<CommitEntry>> {
    if !cwd.is_dir() {
        return Ok(vec![]);
    }

    let pretty = format!("{RS}%H{FS}%h{FS}%s{FS}%aE{FS}%cI");

    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("log")
        .arg(format!("--since={since}"))
        .arg(format!("--until={until}"))
        .arg(format!("--pretty=format:{pretty}"))
        .arg("--shortstat")
        .arg("--no-color")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Err(anyhow::anyhow!("git invocation failed: {e}"));
        }
    };

    if !output.status.success() {
        // Most common cause: not a git repository. Treat as empty; the
        // caller logs.
        return Err(anyhow::anyhow!(
            "git log non-zero exit: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = parse_log_output(&stdout);

    if !entries.is_empty() {
        if let Some(remote) = git_remote_origin(cwd) {
            for e in &mut entries {
                e.github_url = derive_github_url(&remote, &e.sha);
            }
        }
    }

    Ok(entries)
}

fn git_remote_origin(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Parse the raw stdout from `git log --pretty=format:<RS>...<FS>...
/// --shortstat`. Public-in-crate so unit tests don't have to spin up a
/// real git repo just to assert parsing.
pub(crate) fn parse_log_output(stdout: &str) -> Vec<CommitEntry> {
    let mut out = Vec::new();
    for chunk in stdout.split(RS) {
        if chunk.trim().is_empty() {
            continue;
        }
        // First line of the chunk holds the FS-separated fields. The
        // remainder may contain the shortstat line (` N files changed,
        // X insertions(+), Y deletions(-)`) — git omits it for empty
        // merge commits.
        let (head, tail) = match chunk.split_once('\n') {
            Some((h, t)) => (h, t),
            None => (chunk, ""),
        };
        let fields: Vec<&str> = head.splitn(5, FS).collect();
        if fields.len() != 5 {
            continue;
        }
        let (files_changed, insertions, deletions) = parse_shortstat(tail);
        out.push(CommitEntry {
            sha: fields[0].to_owned(),
            short_sha: fields[1].to_owned(),
            subject: fields[2].to_owned(),
            author_email: fields[3].to_owned(),
            committed_at: fields[4].to_owned(),
            files_changed,
            insertions,
            deletions,
            github_url: None,
        });
    }
    out
}

fn parse_shortstat(tail: &str) -> (u32, u32, u32) {
    let line = tail
        .lines()
        .find(|l| l.contains("file") && l.contains("changed"))
        .unwrap_or("");
    let files = extract_count(line, "file");
    let ins = extract_count(line, "insertion");
    let dels = extract_count(line, "deletion");
    (files, ins, dels)
}

// Scan back from `needle` for the most recent run of digits.
fn extract_count(line: &str, needle: &str) -> u32 {
    let Some(idx) = line.find(needle) else {
        return 0;
    };
    let prefix = &line[..idx];
    let digits: String = prefix
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let digits: String = digits.chars().rev().collect();
    digits.parse().unwrap_or(0)
}

/// Build a GitHub commit URL from a remote URL string, or `None` if the
/// remote isn't a GitHub repo. Supports:
/// - `git@github.com:OWNER/REPO(.git)?`
/// - `https://github.com/OWNER/REPO(.git)?`
/// - `ssh://git@github.com/OWNER/REPO(.git)?`
pub(crate) fn derive_github_url(remote_url: &str, sha: &str) -> Option<String> {
    let trimmed = remote_url.trim();
    let owner_repo = trimmed
        .strip_prefix("git@github.com:")
        .or_else(|| trimmed.strip_prefix("https://github.com/"))
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))?
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let mut parts = owner_repo.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}/commit/{sha}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_github_url_handles_ssh_form() {
        let url = derive_github_url("git@github.com:TomasPalsson/worklog.git", "abc123").unwrap();
        assert_eq!(url, "https://github.com/TomasPalsson/worklog/commit/abc123");
    }

    #[test]
    fn derive_github_url_handles_https_form() {
        let url = derive_github_url("https://github.com/TomasPalsson/worklog", "deadbeef").unwrap();
        assert_eq!(
            url,
            "https://github.com/TomasPalsson/worklog/commit/deadbeef"
        );
    }

    #[test]
    fn derive_github_url_strips_trailing_dot_git() {
        let url = derive_github_url("https://github.com/foo/bar.git", "f00").unwrap();
        assert_eq!(url, "https://github.com/foo/bar/commit/f00");
    }

    #[test]
    fn derive_github_url_handles_ssh_protocol_form() {
        let url = derive_github_url("ssh://git@github.com/foo/bar.git", "abc").unwrap();
        assert_eq!(url, "https://github.com/foo/bar/commit/abc");
    }

    #[test]
    fn derive_github_url_returns_none_for_other_hosts() {
        assert!(derive_github_url("git@gitlab.com:foo/bar.git", "abc").is_none());
        assert!(derive_github_url("https://gitlab.com/foo/bar", "abc").is_none());
        assert!(derive_github_url("file:///tmp/some/repo", "abc").is_none());
    }

    #[test]
    fn derive_github_url_returns_none_for_malformed_input() {
        assert!(derive_github_url("git@github.com:no-repo", "abc").is_none());
        assert!(derive_github_url("", "abc").is_none());
    }

    #[test]
    fn parse_log_output_handles_single_commit() {
        let raw = format!(
            "{RS}aaaa1111{FS}aaaa{FS}fix: tighten validation{FS}me@example.com{FS}2026-05-12T10:00:00+00:00\n \
             3 files changed, 12 insertions(+), 4 deletions(-)\n"
        );
        let entries = parse_log_output(&raw);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.sha, "aaaa1111");
        assert_eq!(e.short_sha, "aaaa");
        assert_eq!(e.subject, "fix: tighten validation");
        assert_eq!(e.author_email, "me@example.com");
        assert_eq!(e.committed_at, "2026-05-12T10:00:00+00:00");
        assert_eq!(e.files_changed, 3);
        assert_eq!(e.insertions, 12);
        assert_eq!(e.deletions, 4);
        assert_eq!(e.github_url, None);
    }

    #[test]
    fn parse_log_output_handles_multiple_commits() {
        let raw = format!(
            "{RS}sha1{FS}s1{FS}first{FS}a@x{FS}2026-05-12T10:00:00+00:00\n \
             1 file changed, 1 insertion(+)\n\
             {RS}sha2{FS}s2{FS}second{FS}b@x{FS}2026-05-12T11:00:00+00:00\n \
             2 files changed, 4 deletions(-)\n"
        );
        let entries = parse_log_output(&raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].insertions, 1);
        assert_eq!(entries[0].deletions, 0);
        assert_eq!(entries[1].files_changed, 2);
        assert_eq!(entries[1].insertions, 0);
        assert_eq!(entries[1].deletions, 4);
    }

    #[test]
    fn parse_log_output_handles_missing_shortstat() {
        // Empty merges produce no shortstat line.
        let raw = format!("{RS}sha1{FS}s1{FS}merge nothing{FS}a@x{FS}2026-05-12T10:00:00+00:00\n");
        let entries = parse_log_output(&raw);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].files_changed, 0);
        assert_eq!(entries[0].insertions, 0);
        assert_eq!(entries[0].deletions, 0);
    }

    #[test]
    fn parse_log_output_returns_empty_for_blank_input() {
        assert!(parse_log_output("").is_empty());
        assert!(parse_log_output("\n\n").is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn git_log_in_window_returns_empty_for_non_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let out = git_log_in_window(tmp.path(), "2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z")
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn git_log_in_window_returns_empty_for_missing_cwd() {
        let path = std::path::Path::new("/tmp/definitely-does-not-exist-xyz-worklog");
        let out = git_log_in_window(path, "2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z")
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn git_log_in_window_lists_commits_inside_window() {
        // Skip when no git binary is on the test runner (e.g. minimal CI).
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        seed_repo(tmp.path());

        let out = git_log_in_window(tmp.path(), "2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z")
            .await
            .unwrap();

        assert_eq!(out.len(), 2, "two commits expected, got {out:?}");
        // Default git log order is newest-first.
        assert_eq!(out[0].subject, "second");
        assert_eq!(out[1].subject, "first");
        // shortstat parsing actually reflects file changes:
        assert!(out[0].files_changed >= 1);
        assert!(out[0].insertions >= 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn git_log_in_window_attaches_github_url_when_origin_is_github() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        seed_repo(tmp.path());
        // Add a github-shaped origin so derive_github_url fires.
        let _ = Command::new("git")
            .arg("-C")
            .arg(tmp.path())
            .args(["remote", "add", "origin", "git@github.com:foo/bar.git"])
            .output()
            .unwrap();

        let out = git_log_in_window(tmp.path(), "2026-01-01T00:00:00Z", "2026-12-31T23:59:59Z")
            .await
            .unwrap();
        assert!(!out.is_empty());
        let url = out[0].github_url.as_deref().unwrap();
        assert!(url.starts_with("https://github.com/foo/bar/commit/"));
    }

    fn seed_repo(path: &Path) {
        run(path, &["init", "-q", "-b", "main"]);
        run(path, &["config", "user.email", "test@example.com"]);
        run(path, &["config", "user.name", "Tester"]);
        run(path, &["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join("a.txt"), "alpha\n").unwrap();
        run(path, &["add", "a.txt"]);
        run_with_date(
            path,
            &["commit", "-q", "-m", "first"],
            "2026-05-10T10:00:00Z",
        );
        std::fs::write(path.join("a.txt"), "alpha\nbeta\n").unwrap();
        run(path, &["add", "a.txt"]);
        run_with_date(
            path,
            &["commit", "-q", "-m", "second"],
            "2026-05-11T10:00:00Z",
        );
    }

    fn run(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn run_with_date(cwd: &Path, args: &[&str], date: &str) {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
