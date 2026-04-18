//! Cross-platform scheduled collection.
//!
//! * macOS — writes a launchd plist to
//!   `~/Library/LaunchAgents/is.p5.worklog.collect.plist` with a
//!   `StartInterval` in seconds. Load/unload via `launchctl`.
//! * Linux — writes a systemd user unit pair (`worklog-collect.service` +
//!   `worklog-collect.timer`) under `~/.config/systemd/user/`. Enable/start
//!   via `systemctl --user`.
//! * Everything else — returns `Platform::Unsupported` so the CLI can
//!   print a helpful note without failing.
//!
//! All file writes go through `$SCHEDULE_HOME_OVERRIDE` in tests so the
//! real user scheduler is never touched by `cargo test`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Environment override used by tests to point the writers at a tmp dir.
pub const ENV_SCHEDULE_HOME: &str = "WORKLOG_SCHEDULE_HOME";

/// Label used by launchd and by systemd unit names. Kept stable so
/// re-running install on a different worklog version doesn't leave orphans.
pub const LABEL: &str = "is.p5.worklog.collect";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
    Unsupported,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unsupported
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::MacOS => "launchd",
            Self::Linux => "systemd",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Parsed schedule interval. Stored as seconds because both launchd and
/// systemd are happy with raw seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval(u32);

impl Interval {
    pub const FIVE_MIN: Self = Self(300);
    pub const FIFTEEN_MIN: Self = Self(900);
    pub const HOURLY: Self = Self(3600);
    pub const FOUR_HOURLY: Self = Self(14_400);
    pub const DAILY: Self = Self(86_400);

    pub fn as_seconds(self) -> u32 {
        self.0
    }

    /// Parse strings like `5m`, `15m`, `1h`, `4h`, `daily`, or raw seconds.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        let secs = match s.as_str() {
            "5m" | "5min" | "5minutes" => 300,
            "15m" | "15min" => 900,
            "30m" | "30min" => 1800,
            "1h" | "hourly" => 3600,
            "4h" => 14_400,
            "daily" | "1d" | "24h" => 86_400,
            _ => {
                if let Some(rest) = s.strip_suffix('m') {
                    rest.parse::<u32>()
                        .map(|n| n.saturating_mul(60))
                        .map_err(|e| anyhow::anyhow!("invalid minutes: {e}"))?
                } else if let Some(rest) = s.strip_suffix('h') {
                    rest.parse::<u32>()
                        .map(|n| n.saturating_mul(3600))
                        .map_err(|e| anyhow::anyhow!("invalid hours: {e}"))?
                } else {
                    s.parse::<u32>()
                        .map_err(|e| anyhow::anyhow!("not a duration: {s} ({e})"))?
                }
            }
        };
        if secs < 60 {
            anyhow::bail!("interval must be at least 60 seconds");
        }
        Ok(Self(secs))
    }

    pub fn human(self) -> String {
        match self.0 {
            300 => "every 5m".into(),
            900 => "every 15m".into(),
            1800 => "every 30m".into(),
            3600 => "every hour".into(),
            14_400 => "every 4h".into(),
            86_400 => "daily".into(),
            s if s % 3600 == 0 => format!("every {}h", s / 3600),
            s if s % 60 == 0 => format!("every {}m", s / 60),
            s => format!("every {}s", s),
        }
    }
}

/// Snapshot returned by install / uninstall / status for printing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScheduleStatus {
    pub platform: &'static str,
    pub installed: bool,
    pub interval_secs: Option<u32>,
    pub command: Option<String>,
    /// Primary file written (plist path or timer path).
    pub unit_path: Option<PathBuf>,
    pub extra_paths: Vec<PathBuf>,
    pub notes: Vec<String>,
}

// ───────────────────────── public API ─────────────────────────

pub fn install(interval: Interval, command: &str) -> Result<ScheduleStatus> {
    match Platform::current() {
        Platform::MacOS => macos::install(interval, command),
        Platform::Linux => linux::install(interval, command),
        Platform::Unsupported => Ok(unsupported_status()),
    }
}

pub fn uninstall() -> Result<ScheduleStatus> {
    match Platform::current() {
        Platform::MacOS => macos::uninstall(),
        Platform::Linux => linux::uninstall(),
        Platform::Unsupported => Ok(unsupported_status()),
    }
}

pub fn status() -> Result<ScheduleStatus> {
    match Platform::current() {
        Platform::MacOS => macos::status(),
        Platform::Linux => linux::status(),
        Platform::Unsupported => Ok(unsupported_status()),
    }
}

fn unsupported_status() -> ScheduleStatus {
    ScheduleStatus {
        platform: Platform::current().name(),
        installed: false,
        interval_secs: None,
        command: None,
        unit_path: None,
        extra_paths: vec![],
        notes: vec!["scheduler not implemented for this platform — cron by hand".into()],
    }
}

/// Resolve the root directory where unit files live. Tests set
/// `$WORKLOG_SCHEDULE_HOME=<tmp>` to redirect writes.
fn scheduler_home() -> Result<PathBuf> {
    if let Some(v) = std::env::var_os(ENV_SCHEDULE_HOME) {
        if !v.is_empty() {
            return Ok(PathBuf::from(v));
        }
    }
    dirs::home_dir().context("no home directory")
}

/// Default command the scheduler runs. In Stage 1 this is still
/// `worklog collect all` (the Python collectors); Stage 2 swaps it for a
/// native Rust entrypoint without touching the writer here.
pub fn default_command() -> String {
    if let Some(p) = which_ok("worklog") {
        format!("{} collect all", p.display())
    } else {
        "worklog collect all".to_owned()
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

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

// ───────────────────────── macOS (launchd) ─────────────────────────

mod macos {
    use super::*;

    fn plist_path() -> Result<PathBuf> {
        Ok(scheduler_home()?
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn log_paths() -> Result<(PathBuf, PathBuf)> {
        let home = scheduler_home()?;
        let dir = home.join(".local/share/worklog/logs");
        Ok((dir.join("schedule.out.log"), dir.join("schedule.err.log")))
    }

    pub fn install(interval: Interval, command: &str) -> Result<ScheduleStatus> {
        let path = plist_path()?;
        let (stdout, stderr) = log_paths()?;
        // Quote the command for plist — we pass it through /bin/sh -c so the
        // command can be anything the user pasted.
        let plist = plist_xml(interval.as_seconds(), command, &stdout, &stderr);
        atomic_write(&path, &plist)?;
        // Create log dir so launchd can write on first fire.
        if let Some(parent) = stdout.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Best-effort launchctl load. Skip under tests (no real launchd for
        // the tmp label) by checking $WORKLOG_SCHEDULE_HOME — tests set it.
        if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", path.to_string_lossy().as_ref()])
                .status();
            let _ = std::process::Command::new("launchctl")
                .args(["load", "-w", path.to_string_lossy().as_ref()])
                .status();
        }

        Ok(ScheduleStatus {
            platform: "launchd",
            installed: true,
            interval_secs: Some(interval.as_seconds()),
            command: Some(command.to_owned()),
            unit_path: Some(path),
            extra_paths: vec![stdout, stderr],
            notes: vec![format!("runs {} via launchd", interval.human())],
        })
    }

    pub fn uninstall() -> Result<ScheduleStatus> {
        let path = plist_path()?;
        if path.exists() {
            if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", path.to_string_lossy().as_ref()])
                    .status();
            }
            std::fs::remove_file(&path).with_context(|| format!("rm {}", path.display()))?;
        }
        Ok(ScheduleStatus {
            platform: "launchd",
            installed: false,
            interval_secs: None,
            command: None,
            unit_path: Some(path),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    pub fn status() -> Result<ScheduleStatus> {
        let path = plist_path()?;
        if !path.exists() {
            return Ok(ScheduleStatus {
                platform: "launchd",
                installed: false,
                interval_secs: None,
                command: None,
                unit_path: Some(path),
                extra_paths: vec![],
                notes: vec![],
            });
        }
        let body = std::fs::read_to_string(&path)?;
        let interval = parse_plist_interval(&body);
        let command = parse_plist_command(&body);
        Ok(ScheduleStatus {
            platform: "launchd",
            installed: true,
            interval_secs: interval,
            command,
            unit_path: Some(path),
            extra_paths: vec![],
            notes: vec![],
        })
    }

    fn parse_plist_interval(body: &str) -> Option<u32> {
        let key = body.find("<key>StartInterval</key>")?;
        let rest = &body[key..];
        let int_start = rest.find("<integer>")? + "<integer>".len();
        let int_end = rest[int_start..].find("</integer>")?;
        rest[int_start..int_start + int_end].trim().parse().ok()
    }

    fn parse_plist_command(body: &str) -> Option<String> {
        let args_idx = body.find("<key>ProgramArguments</key>")?;
        let arr_rest = &body[args_idx..];
        let arr_start = arr_rest.find("<array>")?;
        let arr_end = arr_rest.find("</array>")?;
        let arr = &arr_rest[arr_start..arr_end];
        // The last <string>…</string> inside the array holds the actual cmd
        // (the `-c` wrapper preceding is boilerplate).
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

    fn plist_xml(interval_secs: u32, cmd: &str, stdout: &Path, stderr: &Path) -> String {
        // Always run the command through `sh -c` so pipes / env substitution
        // do the expected thing. XML-escape the command string.
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
  <key>StartInterval</key><integer>{interval_secs}</integer>

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
            interval_secs = interval_secs,
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
        Ok(scheduler_home()?.join(".config/systemd/user"))
    }
    fn service_path() -> Result<PathBuf> {
        Ok(unit_dir()?.join("worklog-collect.service"))
    }
    fn timer_path() -> Result<PathBuf> {
        Ok(unit_dir()?.join("worklog-collect.timer"))
    }

    pub fn install(interval: Interval, command: &str) -> Result<ScheduleStatus> {
        let svc = service_path()?;
        let tmr = timer_path()?;
        atomic_write(&svc, &service_unit(command))?;
        atomic_write(&tmr, &timer_unit(interval.as_seconds()))?;

        let under_test = std::env::var_os(ENV_SCHEDULE_HOME).is_some();
        if !under_test {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "worklog-collect.timer"])
                .status();
        }

        Ok(ScheduleStatus {
            platform: "systemd",
            installed: true,
            interval_secs: Some(interval.as_seconds()),
            command: Some(command.to_owned()),
            unit_path: Some(tmr),
            extra_paths: vec![svc],
            notes: vec![format!("runs {} via systemd --user", interval.human())],
        })
    }

    pub fn uninstall() -> Result<ScheduleStatus> {
        let svc = service_path()?;
        let tmr = timer_path()?;
        if std::env::var_os(ENV_SCHEDULE_HOME).is_none() {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", "--now", "worklog-collect.timer"])
                .status();
        }
        for p in [&svc, &tmr] {
            if p.exists() {
                std::fs::remove_file(p).ok();
            }
        }
        Ok(ScheduleStatus {
            platform: "systemd",
            installed: false,
            interval_secs: None,
            command: None,
            unit_path: Some(tmr),
            extra_paths: vec![svc],
            notes: vec![],
        })
    }

    pub fn status() -> Result<ScheduleStatus> {
        let svc = service_path()?;
        let tmr = timer_path()?;
        if !tmr.exists() {
            return Ok(ScheduleStatus {
                platform: "systemd",
                installed: false,
                interval_secs: None,
                command: None,
                unit_path: Some(tmr),
                extra_paths: vec![svc],
                notes: vec![],
            });
        }
        let tbody = std::fs::read_to_string(&tmr)?;
        let sbody = std::fs::read_to_string(&svc).unwrap_or_default();
        let interval_secs = tbody
            .lines()
            .find_map(|l| l.trim().strip_prefix("OnUnitActiveSec="))
            .and_then(|v| v.strip_suffix("s"))
            .and_then(|v| v.parse().ok());
        let command = sbody
            .lines()
            .find_map(|l| l.trim().strip_prefix("ExecStart=").map(|v| v.to_owned()));
        Ok(ScheduleStatus {
            platform: "systemd",
            installed: true,
            interval_secs,
            command,
            unit_path: Some(tmr),
            extra_paths: vec![svc],
            notes: vec![],
        })
    }

    fn service_unit(cmd: &str) -> String {
        format!(
            "[Unit]\n\
             Description=worklog scheduled collector\n\
             After=network-online.target\n\n\
             [Service]\n\
             Type=oneshot\n\
             ExecStart={cmd}\n"
        )
    }

    fn timer_unit(secs: u32) -> String {
        format!(
            "[Unit]\n\
             Description=worklog collector — every {secs}s\n\n\
             [Timer]\n\
             OnBootSec=2min\n\
             OnUnitActiveSec={secs}s\n\
             Persistent=true\n\n\
             [Install]\n\
             WantedBy=timers.target\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    // Serialise all tests that touch $WORKLOG_SCHEDULE_HOME.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn redirect(dir: &Path) -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_SCHEDULE_HOME, dir);
        g
    }
    fn restore() {
        std::env::remove_var(ENV_SCHEDULE_HOME);
    }

    #[test]
    fn interval_parses_human_strings() {
        assert_eq!(Interval::parse("15m").unwrap().as_seconds(), 900);
        assert_eq!(Interval::parse("1h").unwrap().as_seconds(), 3600);
        assert_eq!(Interval::parse("4h").unwrap().as_seconds(), 14_400);
        assert_eq!(Interval::parse("daily").unwrap().as_seconds(), 86_400);
        assert_eq!(Interval::parse("900").unwrap().as_seconds(), 900);
        assert_eq!(Interval::parse("30m").unwrap().as_seconds(), 1800);
    }

    #[test]
    fn interval_rejects_too_short() {
        assert!(Interval::parse("30").is_err());
        assert!(Interval::parse("0").is_err());
    }

    #[test]
    fn human_label_is_stable() {
        assert_eq!(Interval::FIFTEEN_MIN.human(), "every 15m");
        assert_eq!(Interval::HOURLY.human(), "every hour");
        assert_eq!(Interval::DAILY.human(), "daily");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_install_writes_plist_and_uninstall_removes_it() {
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let cmd = "/opt/homebrew/bin/worklog collect all";
        let s = install(Interval::FIFTEEN_MIN, cmd).unwrap();
        assert!(s.installed);
        let plist = s.unit_path.clone().unwrap();
        assert!(plist.is_file());

        let body = std::fs::read_to_string(&plist).unwrap();
        assert!(body.contains("<integer>900</integer>"));
        assert!(body.contains(cmd));
        assert!(body.contains(&format!("<string>{LABEL}</string>")));

        // status should read it back.
        let st = status().unwrap();
        assert!(st.installed);
        assert_eq!(st.interval_secs, Some(900));
        assert_eq!(st.command.as_deref(), Some(cmd));

        let s2 = uninstall().unwrap();
        assert!(!s2.installed);
        assert!(!plist.exists());
        restore();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_install_writes_service_and_timer() {
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let cmd = "/usr/local/bin/worklog collect all";
        let s = install(Interval::HOURLY, cmd).unwrap();
        assert!(s.installed);
        let timer = s.unit_path.clone().unwrap();
        let service = s.extra_paths[0].clone();
        assert!(timer.is_file());
        assert!(service.is_file());
        let tbody = std::fs::read_to_string(&timer).unwrap();
        assert!(tbody.contains("OnUnitActiveSec=3600s"));
        let sbody = std::fs::read_to_string(&service).unwrap();
        assert!(sbody.contains(&format!("ExecStart={cmd}")));

        let st = status().unwrap();
        assert!(st.installed);
        assert_eq!(st.interval_secs, Some(3600));

        uninstall().unwrap();
        assert!(!timer.exists());
        assert!(!service.exists());
        restore();
    }

    #[test]
    fn status_on_fresh_tmp_is_uninstalled() {
        let tmp = tempdir().unwrap();
        let _g = redirect(tmp.path());
        let s = status().unwrap();
        assert!(!s.installed);
        restore();
    }
}
