//! Atomic install with auto-rollback.
//!
//! The contract:
//! * Given a path to the new binary and the destination, swap them
//!   atomically (POSIX rename within the same filesystem) after
//!   successfully smoke-testing the new binary.
//! * Preserve the previous binary at `<destination>.previous` so a
//!   broken rollout can be manually rolled back if even our in-process
//!   rollback fails.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use tracing::{info, warn};

/// RAII handle for the self-update lockfile. Drops the fd → flock
/// released automatically.
#[derive(Debug)]
pub struct UpdateLock {
    // Keep the File alive for the guard's lifetime — flock releases on
    // fd close per POSIX.
    #[allow(dead_code)]
    file: File,
}

/// Acquire an exclusive, non-blocking advisory lock on
/// `<work_dir>/update.lock`. Returns the guard on success. If another
/// process already holds the lock we bail with an informative error
/// so the user knows an update is already in flight.
pub fn acquire_update_lock(work_dir: &Path) -> Result<UpdateLock> {
    let lock_path = work_dir.join("update.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    // Non-blocking flock via `fs4`. Ok(()) → we got the lock.
    // WouldBlock → another process holds it; surface a loud error so
    // the user knows why we refused rather than quietly racing.
    match file.try_lock_exclusive() {
        Ok(()) => Ok(UpdateLock { file }),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => anyhow::bail!(
            "another worklog self-update is already running \
             (lock held on {}). Wait for it to finish or `rm` the \
             lockfile if it's stale.",
            lock_path.display()
        ),
        Err(e) => Err(e).with_context(|| format!("locking {}", lock_path.display())),
    }
}

/// Result of the install + smoke test. Returned so the CLI can report
/// what happened (especially whether rollback triggered).
#[derive(Debug)]
pub struct InstallOutcome {
    pub destination: PathBuf,
    pub previous_backup: Option<PathBuf>,
    pub rolled_back: bool,
}

/// Swap the binary at `dest` with the one at `staged`, running a smoke
/// test via `--version` before and after. `staged` must be on the same
/// filesystem as `dest` so the rename is atomic.
///
/// Steps:
/// 1. Mark `staged` executable (0755).
/// 2. Run `staged --version` — anything non-zero aborts the install.
/// 3. Rename `dest → dest.previous` (best-effort; fresh installs won't
///    have one).
/// 4. Rename `staged → dest`. If anything below fails, restore `.previous`.
/// 5. Run `dest --version`. If that fails, restore `.previous`.
pub fn swap_with_rollback(staged: &Path, dest: &Path) -> Result<InstallOutcome> {
    ensure_executable(staged)?;
    smoke_test(staged).with_context(|| format!("pre-swap smoke test of {}", staged.display()))?;

    let previous = dest.with_extension("previous");
    let had_previous = dest.exists();
    if had_previous {
        let _ = std::fs::remove_file(&previous);
        std::fs::rename(dest, &previous).with_context(|| {
            format!(
                "backing up {} → {}",
                dest.display(),
                previous.display()
            )
        })?;
    }

    if let Err(e) = std::fs::rename(staged, dest) {
        // The primary rename failed (likely cross-filesystem staging,
        // or a permissions flip mid-run). Try to put the previous binary
        // back where it was — but if THAT fails too, the user is now
        // staring at an empty `dest` and needs explicit instructions
        // for manual recovery from `.previous`.
        if had_previous {
            if let Err(rb) = std::fs::rename(&previous, dest) {
                return Err(e).with_context(|| {
                    format!(
                        "renaming {} → {} failed; rollback of {} → {} also failed: {rb}. \
                         The live binary is absent; restore manually from {}.",
                        staged.display(),
                        dest.display(),
                        previous.display(),
                        dest.display(),
                        previous.display(),
                    )
                });
            }
        }
        return Err(e).with_context(|| format!("renaming {} → {}", staged.display(), dest.display()));
    }

    // Post-swap smoke — if the new binary can't report its version, roll
    // back and bail so the user is never left with a broken worklog.
    match smoke_test(dest) {
        Ok(()) => {
            info!("install succeeded at {}", dest.display());
            Ok(InstallOutcome {
                destination: dest.to_path_buf(),
                previous_backup: had_previous.then(|| previous.clone()),
                rolled_back: false,
            })
        }
        Err(e) => {
            if had_previous {
                warn!(
                    "post-swap smoke test failed: {e:#}. Rolling back {} → {}",
                    previous.display(),
                    dest.display()
                );
                // Best-effort rollback. If this fails too, the user has
                // .previous they can manually restore.
                std::fs::rename(&previous, dest).with_context(|| {
                    format!(
                        "rollback: restoring {} → {} (the previous binary \
                         is still there — move it into place manually)",
                        previous.display(),
                        dest.display()
                    )
                })?;
                Ok(InstallOutcome {
                    destination: dest.to_path_buf(),
                    previous_backup: Some(previous),
                    rolled_back: true,
                })
            } else {
                // Fresh install (no .previous to restore). The broken
                // binary is sitting at dest right now — remove it so the
                // user isn't left with a non-functional worklog, and
                // surface the failure as an error rather than pretending
                // we "rolled back" to something that never existed.
                warn!(
                    "post-swap smoke test failed on fresh install: {e:#}. \
                     Removing broken binary at {}.",
                    dest.display()
                );
                let rm_err = std::fs::remove_file(dest).err();
                Err(e)
                    .with_context(|| {
                        match rm_err {
                            Some(rm) => format!(
                                "post-swap smoke test failed; additionally, \
                                 removing the broken binary at {} failed: {rm}",
                                dest.display()
                            ),
                            None => format!(
                                "post-swap smoke test failed; no previous \
                                 binary to restore, the broken download at \
                                 {} has been removed",
                                dest.display()
                            ),
                        }
                    })
            }
        }
    }
}

/// Run `<binary> --version` with a short timeout. Returns an error if the
/// process exits non-zero, can't be spawned, or doesn't finish in time.
pub fn smoke_test(binary: &Path) -> Result<()> {
    let start = std::time::Instant::now();
    let mut child = Command::new(binary)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {}", binary.display()))?;
    let timeout = Duration::from_secs(10);
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => anyhow::bail!("{} --version exited {}", binary.display(), status),
            None if start.elapsed() > timeout => {
                let _ = child.kill();
                anyhow::bail!("{} --version did not return within {timeout:?}", binary.display());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn ensure_executable(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Write a shell script that prints `--version` output and exits 0.
    fn script(path: &Path, body: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "{body}").unwrap();
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn smoke_test_accepts_zero_exit() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("ok");
        script(&p, "echo 1.0; exit 0");
        smoke_test(&p).unwrap();
    }

    #[test]
    fn smoke_test_rejects_non_zero_exit() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad");
        script(&p, "exit 3");
        let err = smoke_test(&p).unwrap_err();
        assert!(format!("{err:#}").contains("exited"));
    }

    #[test]
    fn swap_installs_and_smoke_tests() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        script(&dest, "echo old 0.1.0");
        let staged = tmp.path().join("worklog.new");
        script(&staged, "echo new 0.2.0");

        let outcome = swap_with_rollback(&staged, &dest).unwrap();
        assert!(!outcome.rolled_back);
        assert!(outcome.previous_backup.is_some());
        // The live binary now prints "new …"
        let out = Command::new(&dest).arg("--version").output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("new"), "got {stdout}");
        // And the backup still runs the old one
        let prev = outcome.previous_backup.unwrap();
        let prev_out = Command::new(&prev).arg("--version").output().unwrap();
        let prev_stdout = String::from_utf8_lossy(&prev_out.stdout);
        assert!(prev_stdout.contains("old"), "got {prev_stdout}");
    }

    #[test]
    fn acquire_update_lock_is_exclusive() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let first = acquire_update_lock(dir).unwrap();
        // Second acquire from the same process/fd-table must fail
        // immediately — flock is advisory but OS-level.
        let err = acquire_update_lock(dir).unwrap_err();
        assert!(
            format!("{err:#}").contains("already running"),
            "expected concurrent-run error, got: {err:#}"
        );
        drop(first);
        // After dropping the first, the lock releases and a new
        // acquire succeeds — proves it's not a permanent file.
        let _second = acquire_update_lock(dir).unwrap();
    }

    #[test]
    fn swap_aborts_when_pre_swap_smoke_fails() {
        // If the staged binary itself can't run, the swap must never
        // happen — the live binary stays intact.
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        script(&dest, "echo old; exit 0");
        let staged = tmp.path().join("worklog.new");
        script(&staged, "exit 7");

        let err = swap_with_rollback(&staged, &dest).unwrap_err();
        assert!(format!("{err:#}").contains("pre-swap"));
        let out = Command::new(&dest).arg("--version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("old"));
    }

    #[test]
    fn swap_rolls_back_when_post_swap_smoke_fails() {
        // The real rollback path: pre-swap passes, rename happens, then
        // the *new* binary fails its post-swap smoke test. We must
        // restore dest from .previous.
        //
        // Trick: the staged script branches on its invocation path ($0).
        // When invoked at its staged path (contains "new"), exits 0 so
        // pre-swap passes. When invoked from dest after rename, exits
        // non-zero so post-swap fails.
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        script(&dest, "echo old 0.1; exit 0");
        let staged = tmp.path().join("worklog.new");
        script(
            &staged,
            r#"case "$0" in *.new) echo pre; exit 0;; *) exit 9;; esac"#,
        );

        let outcome = swap_with_rollback(&staged, &dest).unwrap();
        assert!(outcome.rolled_back, "post-swap failure must report rolled_back");
        assert!(outcome.previous_backup.is_some());
        // The old binary must still run at dest.
        let out = Command::new(&dest).arg("--version").output().unwrap();
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("old"),
            "dest should have been restored from .previous; got {out:?}"
        );
    }

    #[test]
    fn fresh_install_post_swap_failure_returns_err_and_cleans_up() {
        // Regression for the C4 bug: on a fresh install (no .previous to
        // restore) + post-swap failure, the function used to return
        // Ok(rolled_back: true) while leaving the broken binary at dest.
        // The CLI would then lie and say "rolled back". Now it must
        // Err, and dest should NOT contain a broken binary.
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog"); // does not exist
        let staged = tmp.path().join("staged");
        script(
            &staged,
            r#"case "$0" in *staged) echo pre; exit 0;; *) exit 9;; esac"#,
        );

        let err = swap_with_rollback(&staged, &dest).unwrap_err();
        assert!(
            format!("{err:#}").contains("post-swap"),
            "error should mention post-swap failure: {err:#}"
        );
        // Broken binary must not be left at dest.
        assert!(
            !dest.exists(),
            "a broken binary must not be left at dest on fresh install"
        );
    }

    #[test]
    fn swap_into_empty_destination_works() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        let staged = tmp.path().join("staged");
        script(&staged, "echo fresh 0.1; exit 0");
        let outcome = swap_with_rollback(&staged, &dest).unwrap();
        assert!(!outcome.rolled_back);
        assert!(outcome.previous_backup.is_none());
        assert!(dest.exists());
    }
}
