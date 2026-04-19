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

pub fn install(command: &str) -> Result<DaemonServiceStatus> {
    match Platform::current() {
        Platform::MacOS => macos::install(command),
        Platform::Linux => linux::install(command),
        Platform::Unsupported => Ok(unsupported_status()),
    }
}

pub fn uninstall() -> Result<DaemonServiceStatus> {
    match Platform::current() {
        Platform::MacOS => macos::uninstall(),
        Platform::Linux => linux::uninstall(),
        Platform::Unsupported => Ok(unsupported_status()),
    }
}

pub fn status() -> Result<DaemonServiceStatus> {
    match Platform::current() {
        Platform::MacOS => macos::status(),
        Platform::Linux => linux::status(),
        Platform::Unsupported => Ok(unsupported_status()),
    }
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

// ───────────────────────── macOS (launchd) ─────────────────────────

mod macos {
    use super::*;

    fn plist_path() -> Result<PathBuf> {
        Ok(service_home()?
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn log_paths() -> Result<(PathBuf, PathBuf)> {
        let dir = service_home()?.join(".local/share/worklog/logs");
        Ok((dir.join("daemon.out.log"), dir.join("daemon.err.log")))
    }

    pub fn install(command: &str) -> Result<DaemonServiceStatus> {
        let path = plist_path()?;
        let (stdout, stderr) = log_paths()?;
        let plist = plist_xml(command, &stdout, &stderr);
        atomic_write(&path, &plist)?;
        if let Some(parent) = stdout.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Best-effort launchctl. Skip under tests (env override set).
        if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", path.to_string_lossy().as_ref()])
                .status();
            let _ = std::process::Command::new("launchctl")
                .args(["load", "-w", path.to_string_lossy().as_ref()])
                .status();
        }

        Ok(DaemonServiceStatus {
            platform: "launchd",
            installed: true,
            command: Some(command.to_owned()),
            unit_path: Some(path),
            extra_paths: vec![stdout, stderr],
            notes: vec!["starts at login; launchd restarts on crash".into()],
        })
    }

    pub fn uninstall() -> Result<DaemonServiceStatus> {
        let path = plist_path()?;
        if path.exists() {
            if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", path.to_string_lossy().as_ref()])
                    .status();
            }
            std::fs::remove_file(&path).with_context(|| format!("rm {}", path.display()))?;
        }
        Ok(DaemonServiceStatus {
            platform: "launchd",
            installed: false,
            command: None,
            unit_path: Some(path),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    pub fn status() -> Result<DaemonServiceStatus> {
        let path = plist_path()?;
        if !path.exists() {
            return Ok(DaemonServiceStatus {
                platform: "launchd",
                installed: false,
                command: None,
                unit_path: Some(path),
                extra_paths: vec![],
                notes: vec![],
            });
        }
        let body = std::fs::read_to_string(&path)?;
        let command = parse_plist_command(&body);
        Ok(DaemonServiceStatus {
            platform: "launchd",
            installed: true,
            command,
            unit_path: Some(path),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    fn parse_plist_command(body: &str) -> Option<String> {
        // The ProgramArguments array ends with the shell command we passed
        // through `/bin/sh -c`. Grab the last <string>…</string> in the
        // array.
        let args_idx = body.find("<key>ProgramArguments</key>")?;
        let arr_rest = &body[args_idx..];
        let arr_start = arr_rest.find("<array>")?;
        let arr_end = arr_rest.find("</array>")?;
        let arr = &arr_rest[arr_start..arr_end];
        let mut last = None;
        let mut cursor = 0;
        while let Some(open) = arr[cursor..].find("<string>") {
            let start = cursor + open + "<string>".len();
            let end = start + arr[start..].find("</string>")?;
            last = Some(arr[start..end].to_owned());
            cursor = end + "</string>".len();
        }
        last
    }

    fn plist_xml(cmd: &str, stdout: &Path, stderr: &Path) -> String {
        let escaped = xml_escape(cmd);
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>

  <key>ProgramArguments</key>
  <array>
    <string>/bin/sh</string>
    <string>-c</string>
    <string>{cmd}</string>
  </array>

  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Background</string>

  <key>StandardOutPath</key><string>{stdout}</string>
  <key>StandardErrorPath</key><string>{stderr}</string>

  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin</string>
  </dict>
</dict>
</plist>
"#,
            label = LABEL,
            cmd = escaped,
            stdout = stdout.display(),
            stderr = stderr.display(),
        )
    }

    fn xml_escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            match ch {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                c => out.push(c),
            }
        }
        out
    }
}

// ───────────────────────── Linux (systemd --user) ─────────────────────────

mod linux {
    use super::*;

    fn unit_dir() -> Result<PathBuf> {
        Ok(service_home()?.join(".config/systemd/user"))
    }

    fn service_path() -> Result<PathBuf> {
        Ok(unit_dir()?.join("worklog-daemon.service"))
    }

    pub fn install(command: &str) -> Result<DaemonServiceStatus> {
        let svc = service_path()?;
        atomic_write(&svc, &service_unit(command))?;

        if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "worklog-daemon.service"])
                .status();
        }

        Ok(DaemonServiceStatus {
            platform: "systemd",
            installed: true,
            command: Some(command.to_owned()),
            unit_path: Some(svc),
            extra_paths: vec![],
            notes: vec!["starts at login; systemd restarts on crash".into()],
        })
    }

    pub fn uninstall() -> Result<DaemonServiceStatus> {
        let svc = service_path()?;
        if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", "--now", "worklog-daemon.service"])
                .status();
        }
        if svc.exists() {
            std::fs::remove_file(&svc).ok();
        }
        Ok(DaemonServiceStatus {
            platform: "systemd",
            installed: false,
            command: None,
            unit_path: Some(svc),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    pub fn status() -> Result<DaemonServiceStatus> {
        let svc = service_path()?;
        if !svc.exists() {
            return Ok(DaemonServiceStatus {
                platform: "systemd",
                installed: false,
                command: None,
                unit_path: Some(svc),
                extra_paths: vec![],
                notes: vec![],
            });
        }
        let body = std::fs::read_to_string(&svc)?;
        let command = body
            .lines()
            .find_map(|l| l.trim().strip_prefix("ExecStart=").map(|v| v.to_owned()));
        Ok(DaemonServiceStatus {
            platform: "systemd",
            installed: true,
            command,
            unit_path: Some(svc),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    fn service_unit(cmd: &str) -> String {
        format!(
            "[Unit]\n\
             Description=worklog daemon (HTTP IPC backing the web UI)\n\
             After=network-online.target\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={cmd}\n\
             Restart=on-failure\n\
             RestartSec=5s\n\
             Environment=PATH=/usr/local/bin:/usr/bin:/bin\n\n\
             [Install]\n\
             WantedBy=default.target\n"
        )
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
