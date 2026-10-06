//! Starts, watches and restarts the Verdict helper so it runs with the
//! daemon instead of in a terminal (spec 017, FR-01..FR-06).
//!
//! [`Supervisor`] is the pure state machine; [`RealHost`] launches
//! `uv run`. A process-wide instance serves the daemon routes.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::paths::Paths;
use crate::routing_contract::{
    CLASSIFIER_ADDR, VERDICT_GIT_REV, VERDICT_GIT_URL, VERDICT_MODEL_REPO, VERDICT_MODEL_REVISION,
};
use crate::verdict::SERVER_SCRIPT;
use crate::verdict_contract::{
    VerdictState, HEALTH_INTERVAL_SECS, RESTART_LIMIT, RESTART_WINDOW_SECS, UV_CANDIDATES,
    VERDICT_ENABLED_KEY,
};

/// Seconds after a launch during which a silent helper is "starting", not
/// "not answering" (journey 2).
const START_GRACE_SECS: u64 = 90;

pub enum LaunchError {
    NeedsUv,
    Failed(String),
}

pub trait Proc: Send {
    /// `Some(reason)` once the process has exited.
    fn exited(&mut self) -> Option<String>;
    fn kill(&mut self);
}

pub trait Host {
    /// Seconds on any monotonic-enough clock.
    fn now(&self) -> u64;
    fn launch(&mut self) -> Result<Box<dyn Proc>, LaunchError>;
    fn answering(&self) -> bool;
}

pub struct Supervisor<H: Host> {
    host: H,
    enabled: bool,
    proc: Option<Box<dyn Proc>>,
    started_at: u64,
    /// Launch times of restarts after a crash, inside the window.
    restarts: Vec<u64>,
    failed: Option<VerdictState>,
}

impl<H: Host> Supervisor<H> {
    /// A supervisor that is off until [`Self::set_enabled`] says otherwise.
    pub fn new(host: H) -> Self {
        Self {
            host,
            enabled: false,
            proc: None,
            started_at: 0,
            restarts: Vec::new(),
            failed: None,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.kill();
            self.failed = None;
        } else if self.proc.is_none() {
            self.restarts.clear();
            self.launch();
        }
    }

    /// Relaunch after "stopped" with a fresh restart budget.
    pub fn retry(&mut self) {
        if self.enabled && matches!(self.failed, Some(VerdictState::Stopped { .. })) {
            self.restarts.clear();
            self.launch();
        }
    }

    /// One health check: restart a crashed process unless the budget is spent.
    pub fn tick(&mut self) {
        let Some(proc) = self.proc.as_mut() else {
            return;
        };
        let Some(error) = proc.exited() else {
            return;
        };
        self.proc = None;
        let now = self.host.now();
        self.restarts
            .retain(|t| now.saturating_sub(*t) < RESTART_WINDOW_SECS);
        if self.restarts.len() as u32 >= RESTART_LIMIT {
            self.failed = Some(VerdictState::Stopped { error });
            return;
        }
        self.restarts.push(now);
        self.launch();
    }

    pub fn state(&self) -> VerdictState {
        if !self.enabled {
            return VerdictState::Off;
        }
        if let Some(failed) = &self.failed {
            return failed.clone();
        }
        if self.proc.is_none() {
            return VerdictState::NotAnswering;
        }
        if self.host.answering() {
            VerdictState::Running
        } else if self.host.now().saturating_sub(self.started_at) < START_GRACE_SECS {
            VerdictState::Starting
        } else {
            VerdictState::NotAnswering
        }
    }

    pub fn shutdown(&mut self) {
        self.kill();
    }

    fn kill(&mut self) {
        if let Some(mut p) = self.proc.take() {
            p.kill();
        }
    }

    fn launch(&mut self) {
        match self.host.launch() {
            Ok(p) => {
                self.proc = Some(p);
                self.started_at = self.host.now();
                self.failed = None;
            }
            Err(LaunchError::NeedsUv) => self.failed = Some(VerdictState::NeedsUv),
            Err(LaunchError::Failed(error)) => self.failed = Some(VerdictState::Stopped { error }),
        }
    }
}

/// Missing or unrecognised means on (FR-06).
fn parse_enabled(raw: Option<&str>) -> bool {
    raw != Some("off")
}

/// `uv run` args to launch the helper script pinned to the Verdict git
/// revision + huggingface_hub, so a test can assert the pins never
/// drift to a loose package name.
pub fn uv_args(script_path: &Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "--with".to_string(),
        format!("{VERDICT_GIT_URL}@{VERDICT_GIT_REV}"),
        "--with".to_string(),
        "huggingface_hub".to_string(),
        "python".to_string(),
        script_path.display().to_string(),
    ]
}

/// First existing `uv`: [`UV_CANDIDATES`] in order, then `PATH`.
pub fn find_uv_in(
    home: &Path,
    path_var: Option<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let candidates = UV_CANDIDATES.iter().map(|c| match c.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(c),
    });
    let on_path = path_var
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("uv"));
    candidates.chain(on_path).find(|p| exists(p))
}

pub fn find_uv() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    find_uv_in(&home, std::env::var_os("PATH"), |p| p.is_file())
}

/// `uv run` of the helper script with its model pins, ready to spawn.
pub fn command(uv: &Path, script: &Path, data_dir: &Path) -> Command {
    let mut cmd = Command::new(uv);
    cmd.args(uv_args(script))
        .env("WORKLOG_VERDICT_MODEL_DIR", data_dir.join("verdict-model"))
        .env("WORKLOG_VERDICT_MODEL_REPO", VERDICT_MODEL_REPO)
        .env("WORKLOG_VERDICT_MODEL_REVISION", VERDICT_MODEL_REVISION);
    cmd
}

fn last_line(log: &str) -> &str {
    log.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

struct RealProc {
    child: Child,
    log: PathBuf,
}

impl Proc for RealProc {
    fn exited(&mut self) -> Option<String> {
        let status = self.child.try_wait().ok().flatten()?;
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        Some(format!("exited {status}: {}", last_line(&log)))
    }

    fn kill(&mut self) {
        // The child leads its own process group; killing the group takes
        // the python grandchild down too when `uv run` does not exec.
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.child.id())])
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct RealHost {
    paths: Paths,
}

impl Host for RealHost {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    fn launch(&mut self) -> Result<Box<dyn Proc>, LaunchError> {
        let uv = find_uv().ok_or(LaunchError::NeedsUv)?;
        let failed = |e: anyhow::Error| LaunchError::Failed(format!("{e:#}"));
        let data_dir = &self.paths.data_dir;
        let script = data_dir.join("verdict_server.py");
        let log = data_dir.join("verdict.log");
        let spawn = || -> Result<Child> {
            self.paths.ensure()?;
            std::fs::write(&script, SERVER_SCRIPT)
                .with_context(|| format!("writing {}", script.display()))?;
            let stderr = std::fs::File::create(&log)
                .with_context(|| format!("creating {}", log.display()))?;
            command(&uv, &script, data_dir)
                .stdout(Stdio::null())
                .stderr(stderr)
                .process_group(0)
                .spawn()
                .context("spawning `uv run`")
        };
        let child = spawn().map_err(failed)?;
        Ok(Box::new(RealProc { child, log }))
    }

    fn answering(&self) -> bool {
        crate::daemon_service::is_running(CLASSIFIER_ADDR, Duration::from_secs(2))
    }
}

static SUPERVISOR: Mutex<Option<Supervisor<RealHost>>> = Mutex::new(None);

fn with_supervisor<T>(f: impl FnOnce(&mut Supervisor<RealHost>) -> T) -> Option<T> {
    let mut guard = SUPERVISOR.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_mut().map(f)
}

/// Start Verdict if the Owner has it on, then health-check it every
/// [`HEALTH_INTERVAL_SECS`] until the returned task is aborted.
pub fn spawn() -> Result<tokio::task::JoinHandle<()>> {
    let host = RealHost {
        paths: Paths::resolve()?,
    };
    let mut supervisor = Supervisor::new(host);
    supervisor.set_enabled(parse_enabled(
        crate::envfile::read(VERDICT_ENABLED_KEY).as_deref(),
    ));
    *SUPERVISOR.lock().unwrap_or_else(|e| e.into_inner()) = Some(supervisor);
    Ok(tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(HEALTH_INTERVAL_SECS)).await;
            let _ = tokio::task::spawn_blocking(|| with_supervisor(Supervisor::tick)).await;
        }
    }))
}

/// Blocks on a 2 s probe when the process is alive; call off the async threads.
pub fn status() -> VerdictState {
    with_supervisor(|s| s.state()).unwrap_or(VerdictState::Off)
}

/// Persist the switch, then apply it to the running supervisor.
pub fn set_enabled(on: bool) -> Result<VerdictState> {
    crate::envfile::upsert(VERDICT_ENABLED_KEY, if on { "on" } else { "off" })?;
    with_supervisor(|s| s.set_enabled(on));
    Ok(status())
}

pub fn retry() -> VerdictState {
    with_supervisor(Supervisor::retry);
    status()
}

/// Kill the helper so it does not outlive the daemon.
pub fn shutdown() {
    with_supervisor(Supervisor::shutdown);
}

#[path = "verdict_supervisor_test.rs"]
#[cfg(test)]
mod tests;
