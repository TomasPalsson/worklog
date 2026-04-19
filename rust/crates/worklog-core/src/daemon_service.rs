//! Cross-platform service installation for the worklog daemon.
//!
//! The long-lived `worklog daemon` process backs every write path out of
//! the web UI. Running it manually via `worklog daemon` in a terminal is
//! fine for testing, but the happy path is "log in, things just work" —
//! which means the OS supervisor (launchd on macOS, systemd --user on
//! Linux) owns the process and restarts it when it crashes.
//!
//! The shape deliberately mirrors `schedule.rs` so tests + CLI glue can
//! share the `ENV_SCHEDULE_HOME` env override. The differences vs
//! `schedule.rs`:
//!   * no interval — daemon runs continuously
//!   * `KeepAlive = true` (macOS) / `Restart = on-failure` (linux) so
//!     the supervisor treats a crash as "turn it back on"
//!
//! Tests redirect the real launchd/systemd paths via `WORKLOG_SCHEDULE_HOME`
//! so `cargo test` never invokes launchctl/systemctl against the user's
//! real session.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::schedule::{Platform, ENV_SCHEDULE_HOME};

/// launchd label + systemd unit stem. Stable across worklog versions so
/// reinstalls never leave orphaned units behind.
pub const LABEL: &str = "is.p5.worklog.daemon";

/// Snapshot returned by install / uninstall / status for rendering.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DaemonServiceStatus {
    pub platform: &'static str,
    pub installed: bool,
    pub command: Option<String>,
    pub unit_path: Option<PathBuf>,
    pub extra_paths: Vec<PathBuf>,
    pub notes: Vec<String>,
}

// ───────────────────────── public API ─────────────────────────

pub fn install(_command: &str) -> Result<DaemonServiceStatus> {
    anyhow::bail!("daemon_service::install not yet implemented")
}

pub fn uninstall() -> Result<DaemonServiceStatus> {
    anyhow::bail!("daemon_service::uninstall not yet implemented")
}

pub fn status() -> Result<DaemonServiceStatus> {
    anyhow::bail!("daemon_service::status not yet implemented")
}

/// Default command baked into the plist / service unit. Resolves the
/// `worklog` binary via $PATH — if the user relocates it later they can
/// pass `--command` to override.
pub fn default_command() -> String {
    if let Some(p) = which_ok("worklog") {
        format!("{} daemon", p.display())
    } else {
        "worklog daemon".to_owned()
    }
}

fn which_ok(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn service_home() -> Result<PathBuf> {
    if let Some(v) = std::env::var_os(ENV_SCHEDULE_HOME) {
        if !v.is_empty() {
            return Ok(PathBuf::from(v));
        }
    }
    dirs::home_dir().context("no home directory")
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

fn unsupported_status() -> DaemonServiceStatus {
    DaemonServiceStatus {
        platform: Platform::current().name(),
        installed: false,
        command: None,
        unit_path: None,
        extra_paths: vec![],
        notes: vec!["daemon supervisor not implemented for this platform — run `worklog daemon` in a tmux session".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    // Shared between schedule.rs tests and these — both redirect the same
    // env var. Avoiding interleaving keeps platform probes stable.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn redirect(dir: &Path) -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_SCHEDULE_HOME, dir);
        g
    }

    fn restore() {
        std::env::remove_var(ENV_SCHEDULE_HOME);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_install_writes_plist_with_keepalive() {
        // B11: macOS install produces a plist with KeepAlive=true,
        // RunAtLoad=true, and ProgramArguments pointing at the worklog
        // binary + `daemon`. These are the flags launchd needs to treat
        // this as a long-running supervised process (unlike the schedule
        // plist which is a periodic one-shot).
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let status = install("/usr/local/bin/worklog daemon").unwrap();
        assert!(status.installed);
        let plist = status.unit_path.clone().unwrap();
        assert!(plist.is_file(), "plist should exist at {}", plist.display());
        let body = std::fs::read_to_string(&plist).unwrap();
        assert!(body.contains("<key>KeepAlive</key>"), "missing KeepAlive");
        assert!(body.contains("<true/>"));
        assert!(body.contains("<key>RunAtLoad</key>"), "missing RunAtLoad");
        assert!(body.contains("/usr/local/bin/worklog daemon"));
        restore();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_install_writes_service_unit_with_restart() {
        // B12: linux install produces a systemd user unit with
        // Restart=on-failure and WantedBy=default.target. Note: NO
        // timer — the daemon is long-lived, not periodic.
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let status = install("/usr/local/bin/worklog daemon").unwrap();
        assert!(status.installed);
        let unit = status.unit_path.clone().unwrap();
        assert!(unit.is_file());
        let body = std::fs::read_to_string(&unit).unwrap();
        assert!(body.contains("Restart=on-failure"), "missing Restart line");
        assert!(body.contains("WantedBy=default.target"), "wrong target");
        assert!(body.contains("ExecStart=/usr/local/bin/worklog daemon"));
        restore();
    }

    #[test]
    fn install_is_idempotent() {
        // B13: calling install() twice is safe — the second call
        // atomic-overwrites and returns installed=true, no orphan unit.
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let first = install("worklog daemon").unwrap();
        let second = install("worklog daemon").unwrap();
        assert!(first.installed && second.installed);
        assert_eq!(first.unit_path, second.unit_path);
        restore();
    }

    #[test]
    fn uninstall_removes_unit_file() {
        // B14: after uninstall, the unit file is gone and status reports
        // installed=false.
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let _ = install("worklog daemon").unwrap();
        let status = uninstall().unwrap();
        assert!(!status.installed);
        if let Some(p) = status.unit_path {
            assert!(!p.exists(), "unit file should be gone after uninstall");
        }
        restore();
    }

    #[test]
    fn status_reports_installed_when_unit_exists() {
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let _ = install("worklog daemon").unwrap();
        let st = status().unwrap();
        assert!(st.installed);
        restore();
    }

    #[test]
    fn status_reports_not_installed_on_fresh_home() {
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let st = status().unwrap();
        assert!(!st.installed);
        restore();
    }

    #[test]
    fn default_command_is_stable_string() {
        // Regardless of whether `worklog` is on PATH, the command must
        // end in `" daemon"` so the service unit gets the right target.
        let cmd = default_command();
        assert!(cmd.ends_with(" daemon"), "got: {cmd}");
    }
}
