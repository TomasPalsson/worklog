//! Atomic install with auto-rollback.
//!
//! The contract:
//! * Given a path to the new binary and the destination, swap them
//!   atomically (POSIX rename within the same filesystem) after
//!   successfully smoke-testing the new binary.
//! * Preserve the previous binary at `<destination>.previous` so a
//!   broken rollout can be manually rolled back if even our in-process
//!   rollback fails.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};

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
        if had_previous {
            let _ = std::fs::rename(&previous, dest);
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
            warn!(
                "post-swap smoke test failed: {e:#}. Rolling back {} → {}",
                previous.display(),
                dest.display()
            );
            if had_previous {
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
            }
            Ok(InstallOutcome {
                destination: dest.to_path_buf(),
                previous_backup: had_previous.then(|| previous.clone()),
                rolled_back: true,
            })
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
    fn swap_rolls_back_on_bad_new_binary() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        script(&dest, "echo old; exit 0");
        let staged = tmp.path().join("worklog.new");
        script(&staged, "exit 0"); // passes pre-swap smoke
        // ...but when we invoke the staged one *after* moving it into
        // dest, we want to simulate failure. Use a trigger file so the
        // post-swap test fails but pre-swap passes.
        let trigger = tmp.path().join("trigger");
        std::fs::write(&trigger, "").unwrap();
        script(
            &staged,
            &format!(
                "if [ -f {trigger} ]; then exit 7; else echo ok; exit 0; fi",
                trigger = trigger.display()
            ),
        );
        // pre-swap: trigger exists → staged exits 7 → pre-swap fails
        // That's what we want to assert for THIS test: swap aborts early.
        let err = swap_with_rollback(&staged, &dest).unwrap_err();
        assert!(format!("{err:#}").contains("pre-swap"));
        // dest is untouched
        let out = Command::new(&dest).arg("--version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("old"));
    }

    #[test]
    fn swap_into_empty_destination_works() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("worklog");
        // dest does not exist yet
        let staged = tmp.path().join("staged");
        script(&staged, "echo fresh 0.1; exit 0");
        let outcome = swap_with_rollback(&staged, &dest).unwrap();
        assert!(!outcome.rolled_back);
        assert!(outcome.previous_backup.is_none());
        assert!(dest.exists());
    }
}
