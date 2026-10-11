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

use crate::schedule::{check_output, service_tool, Platform, ENV_SCHEDULE_HOME};

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

const HEALTH_ADDR: &str = "127.0.0.1:9323";

pub fn install(command: &str) -> Result<DaemonServiceStatus> {
    install_at(
        command,
        HEALTH_ADDR,
        std::time::Duration::from_secs(5),
        std::time::Duration::from_millis(200),
    )
}

fn install_at(
    command: &str,
    tcp: &str,
    budget: std::time::Duration,
    step: std::time::Duration,
) -> Result<DaemonServiceStatus> {
    install_probing(
        command,
        tcp,
        budget,
        step,
        std::time::Duration::from_millis(200),
    )
}

/// `probe` is the per-request /health timeout; tests that need a slow
/// answer to still count as an answer pass a longer one.
fn install_probing(
    command: &str,
    tcp: &str,
    budget: std::time::Duration,
    step: std::time::Duration,
    probe: std::time::Duration,
) -> Result<DaemonServiceStatus> {
    let tool = match Platform::current() {
        Platform::MacOS => "launchctl",
        Platform::Linux => "systemctl",
        Platform::Unsupported => return Ok(unsupported_status()),
    };
    let before = service_tool(tool).and_then(|_| pre_install_pid(tcp, probe));
    let mut old_gone = Ok(());
    if let Some(old) = before {
        // A timed-out probe still means something holds the port, so stop either way.
        match Platform::current() {
            Platform::MacOS => macos::stop(),
            _ => linux::stop(),
        }
        old_gone = await_healthy(tcp, |a| old_daemon_gone(a, old, probe), budget, step)
            .with_context(|| {
                format!(
                    "old daemon did not answer the stop request: /health on {tcp} still answered; \
                 a daemon started outside the service (e.g. `worklog daemon` in a terminal) \
                 must be stopped by hand, then re-run install"
                )
            });
    }
    let status = match Platform::current() {
        Platform::MacOS => macos::install(command)?,
        _ => linux::install(command)?,
    };
    old_gone?;
    if service_tool(tool).is_some() {
        let old = before.flatten();
        await_healthy(
            tcp,
            |a| health_pid(a, probe).is_some_and(|p| Some(p) != old),
            budget,
            step,
        )?;
    }
    Ok(status)
}

fn await_healthy(
    tcp: &str,
    probe: impl Fn(&str) -> bool,
    budget: std::time::Duration,
    step: std::time::Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    loop {
        if probe(tcp) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("daemon did not answer /health within {budget:?} on {tcp}");
        }
        std::thread::sleep(step);
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

/// What a post-upgrade restart attempt did. Surfaced to the CLI so
/// `worklog upgrade` can print "… · daemon restarted" (or the relevant
/// no-op branch).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartOutcome {
    /// The supervised service was running on the old binary; we asked
    /// launchd/systemd to cycle it and it came back (or at least the
    /// kick-off command returned successfully).
    Restarted,
    /// Service is installed but wasn't running (user had stopped it).
    /// No action taken — a re-start is the user's call.
    NotRunning,
    /// No service unit installed at all. Not an error.
    NotInstalled,
    /// Platform has no supervisor integration (Windows, other Unix).
    Unsupported,
}

/// Probe + restart entry point used by `worklog upgrade`. If the daemon
/// service is installed AND currently responding to `/health`, cycle it
/// so the new binary is the one being supervised. Anything else is a
/// no-op that reports why.
///
/// Implementation uses injectable probe + kick functions so tests can
/// assert each branch without a real supervisor. Production callers use
/// `restart_if_running()` which wires in the defaults.
pub fn restart_if_running(tcp: &str) -> Result<RestartOutcome> {
    restart_if_running_with(
        tcp,
        |addr| is_running(addr, std::time::Duration::from_millis(500)),
        kick_platform,
    )
}

/// Test seam — tests pass deterministic probe + kick closures.
pub fn restart_if_running_with(
    tcp: &str,
    probe: impl Fn(&str) -> bool,
    kick: impl Fn() -> Result<()>,
) -> Result<RestartOutcome> {
    if Platform::current() == Platform::Unsupported {
        return Ok(RestartOutcome::Unsupported);
    }
    let s = status()?;
    if !s.installed {
        return Ok(RestartOutcome::NotInstalled);
    }
    if !probe(tcp) {
        return Ok(RestartOutcome::NotRunning);
    }
    kick()?;
    Ok(RestartOutcome::Restarted)
}

/// Real restart command per platform. Skipped under tests (env override).
fn kick_platform() -> Result<()> {
    // `cfg!(test)`: updater's run_update tests reach here without the env
    // override and used to kickstart the developer's real daemon.
    if cfg!(test) || std::env::var_os(ENV_SCHEDULE_HOME).is_some() {
        // Test mode — never actually touch the real user supervisor.
        return Ok(());
    }
    match Platform::current() {
        Platform::MacOS => {
            // Prefer `kickstart -k` over unload+load: it tells launchd
            // to cycle the existing job without us having to know the
            // plist path. `gui/<uid>/<label>` is the per-user domain.
            let uid = get_uid();
            let target = format!("gui/{uid}/{LABEL}");
            std::process::Command::new("launchctl")
                .args(["kickstart", "-k", &target])
                .output()
                .context("launchctl kickstart")?;
        }
        Platform::Linux => {
            std::process::Command::new("systemctl")
                .args(["--user", "restart", "worklog-daemon.service"])
                .output()
                .context("systemctl --user restart")?;
        }
        Platform::Unsupported => {}
    }
    Ok(())
}

/// Effective UID via libc-free probe. launchd accepts `$(id -u)` or a
/// literal integer — we read env first ($UID is exported in most
/// shells) and fall back to the user's home-dir stat on miss.
fn get_uid() -> String {
    if let Ok(s) = std::env::var("UID") {
        if !s.is_empty() {
            return s;
        }
    }
    // `id -u` is always on PATH on macOS + linux.
    if let Ok(out) = std::process::Command::new("id").arg("-u").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !s.is_empty() {
                return s;
            }
        }
    }
    // Last-ditch: "501" is the default macOS primary-user UID. A wrong
    // UID here just means the kickstart command fails — we already
    // swallow that via .output().
    "501".into()
}

/// Blocking probe that hits `/health` on the daemon's TCP port. Returns
/// true on HTTP 2xx within `timeout`, false on any other outcome
/// (connect refused, slow, wrong status, whatever). No retries — callers
/// own the retry loop.
pub fn is_running(tcp: &str, timeout: std::time::Duration) -> bool {
    health_body(tcp, timeout).is_some()
}

fn get_health(
    tcp: &str,
    timeout: std::time::Duration,
) -> reqwest::Result<reqwest::blocking::Response> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()?
        .get(format!("http://{tcp}/health"))
        .send()
}

/// `/health` response body on HTTP 2xx, `None` otherwise.
fn health_body(tcp: &str, timeout: std::time::Duration) -> Option<String> {
    let r = get_health(tcp, timeout).ok()?;
    r.status()
        .is_success()
        .then(|| r.text().unwrap_or_default())
}

/// The `pid` in a 2xx `/health` body; the outer `Option` is "answered".
fn pid_of(r: reqwest::blocking::Response) -> Option<Option<u64>> {
    r.status().is_success().then(|| {
        let v: serde_json::Value = serde_json::from_str(&r.text().unwrap_or_default()).ok()?;
        v["pid"].as_u64()
    })
}

fn health_pid(tcp: &str, timeout: std::time::Duration) -> Option<u64> {
    pid_of(get_health(tcp, timeout).ok()?)?
}

/// `Some(pid)` when something holds the port: it answered (with its pid, if
/// it reports one) or timed out (`None`). `None` when nothing is there.
fn pre_install_pid(tcp: &str, timeout: std::time::Duration) -> Option<Option<u64>> {
    match get_health(tcp, timeout) {
        Err(e) => e.is_timeout().then_some(None),
        Ok(r) => pid_of(r),
    }
}

/// A timeout is not proof: a loaded old daemon is slow, not gone. With no
/// known old pid, any successful answer could be the old daemon.
fn old_daemon_gone(tcp: &str, old: Option<u64>, timeout: std::time::Duration) -> bool {
    match get_health(tcp, timeout) {
        Err(e) => !e.is_timeout(),
        Ok(r) => pid_of(r).is_none_or(|p| old.is_some() && p != old),
    }
}

/// Probe /health; if the daemon isn't up, install the service unit (if
/// needed) and poll until it comes up OR we hit the budget. Returns
/// immediately if the daemon is already running.
///
/// This is the "just work" glue wired into `worklog day` and
/// `worklog web up`. Installation is idempotent so calling it on every
/// invocation is cheap — the plist write + rename is a no-op when the
/// file content matches.
pub fn ensure_running(tcp: &str) -> Result<DaemonEnsureOutcome> {
    ensure_running_at(
        tcp,
        std::time::Duration::from_secs(5),
        std::time::Duration::from_millis(200),
    )
}

fn ensure_running_at(
    tcp: &str,
    budget: std::time::Duration,
    step: std::time::Duration,
) -> Result<DaemonEnsureOutcome> {
    if is_running(tcp, std::time::Duration::from_millis(500)) {
        return Ok(DaemonEnsureOutcome::AlreadyUp);
    }
    if !status()?.installed {
        install_at(&default_command(), tcp, budget, step)?;
        // install_at skips its own wait with no service tool; a no-op when it ran.
        await_healthy(
            tcp,
            |a| is_running(a, std::time::Duration::from_millis(200)),
            budget,
            step,
        )?;
        return Ok(DaemonEnsureOutcome::Installed);
    }
    // Installed but not responding — launchd / systemd should already be
    // restarting it; wait, and surface the error if it stays down.
    await_healthy(
        tcp,
        |a| is_running(a, std::time::Duration::from_millis(200)),
        budget,
        step,
    )?;
    Ok(DaemonEnsureOutcome::Restarted)
}

/// What `ensure_running` actually did. Drives the spinner / log line the
/// caller prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DaemonEnsureOutcome {
    AlreadyUp,
    Installed,
    Restarted,
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

    pub fn stop() {
        if let (Some(tool), Ok(path)) = (service_tool("launchctl"), plist_path()) {
            let _ = std::process::Command::new(tool)
                .args(["unload", path.to_string_lossy().as_ref()])
                .output();
        }
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
        // `.output()` captures stderr so first-time installs don't spam
        // the terminal with "Unload failed: 5: Input/output error" —
        // launchctl complains when unload targets a service that wasn't
        // loaded yet, but that's expected on a fresh install and users
        // shouldn't see the noise.
        if let Some(tool) = service_tool("launchctl") {
            let _ = std::process::Command::new(&tool)
                .args(["unload", path.to_string_lossy().as_ref()])
                .output();
            check_output(
                "launchctl load",
                std::process::Command::new(&tool)
                    .args(["load", "-w", path.to_string_lossy().as_ref()])
                    .output(),
            )?;
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
                    .output();
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
    <string>{path}</string>
  </dict>
</dict>
</plist>
"#,
            label = LABEL,
            cmd = escaped,
            stdout = stdout.display(),
            stderr = stderr.display(),
            path = xml_escape(&super::service_env_path()),
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

/// PATH baked into the generated launchd plist / systemd unit.
///
/// A launchd agent (and a systemd --user unit) inherits almost nothing from
/// your shell, so a system-only PATH silently breaks anything the daemon
/// shells out to. The estimator spawns the `claude` CLI, which installs to
/// `~/.local/bin` — absent from the PATH this used to hardcode. The effect
/// was that **every** estimate triggered from the web UI failed with
/// ``spawning `claude` ``, while the identical `worklog estimate` worked in a
/// terminal, because a login shell has `~/.local/bin` and the daemon did not.
///
/// User bin dirs come first so a locally-installed tool wins over an older
/// system copy, matching what a login shell would resolve. Named
/// `service_env_path` rather than `service_path` because the platform
/// modules already use that name for the unit *file* path.
pub fn service_env_path() -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // Where `claude`, `worklog` and friends actually land.
        for rel in [".local/bin", ".claude/local", ".cargo/bin", ".bun/bin"] {
            parts.push(home.join(rel).to_string_lossy().into_owned());
        }
    }
    for sys in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        parts.push((*sys).to_owned());
    }
    parts.join(":")
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

    pub fn stop() {
        if let Some(tool) = service_tool("systemctl") {
            let _ = std::process::Command::new(tool)
                .args(["--user", "stop", "worklog-daemon.service"])
                .output();
        }
    }

    pub fn install(command: &str) -> Result<DaemonServiceStatus> {
        let svc = service_path()?;
        atomic_write(&svc, &service_unit(command))?;

        if let Some(tool) = service_tool("systemctl") {
            let _ = std::process::Command::new(&tool)
                .args(["--user", "daemon-reload"])
                .status();
            check_output(
                "systemctl enable",
                std::process::Command::new(&tool)
                    .args(["--user", "enable", "--now", "worklog-daemon.service"])
                    .output(),
            )?;
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
        // Same reasoning as the launchd plist: the unit's PATH must include
        // the user's bin dirs or the estimator can't spawn `claude`.
        let path = super::service_env_path();
        format!(
            "[Unit]\n\
             Description=worklog daemon (HTTP IPC backing the web UI)\n\
             After=network-online.target\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={cmd}\n\
             Restart=on-failure\n\
             RestartSec=5s\n\
             Environment=PATH={path}\n\n\
             [Install]\n\
             WantedBy=default.target\n"
        )
    }
}

#[path = "daemon_service_test.rs"]
#[cfg(test)]
mod tests;
