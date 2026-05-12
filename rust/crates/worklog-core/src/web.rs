//! `worklog web` — orchestration for the dockerised Next.js UI.
//!
//! The web container bind-mounts the worklog data directory so it can
//! read the SQLite DB directly (bun:sqlite) and talk to the Rust daemon
//! over its unix socket. This module wraps `docker` / `docker compose`
//! and the rendered-out compose file so the CLI doesn't need to know
//! anything about Docker internals.
//!
//! Submodule:
//! * [`fetch`] — download the `web/` tree from GitHub when the user
//!   isn't in a cloned repo. `resolve_web_context` fans out to the
//!   cache path first; `worklog web fetch` pre-warms it.

pub mod fetch;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::paths::Paths;

pub const CONTAINER_NAME: &str = "worklog-web";
pub const IMAGE: &str = "worklog-web:latest";

/// PID file for the host-bun web runner. Cohabits with the daemon
/// socket / DB so `worklog web` can find it without an extra env var.
pub fn bun_pid_path(paths: &Paths) -> PathBuf {
    paths.data_dir.join("web.pid")
}

pub fn bun_log_path(paths: &Paths) -> PathBuf {
    paths.log_dir.join("web.log")
}

/// Start the Next.js UI as a host-side `bun` process (no Docker).
///
/// `bun run start` requires a build, so we run `bun run build` first
/// when the standalone output isn't there. Spawns in the background,
/// writes a pid file, and returns once the port is responsive (or
/// after a 30s timeout).
pub fn bun_up(paths: &Paths, web_context: &Path, port: u16) -> Result<u32> {
    // Refuse to double-start.
    if let Some(pid) = read_bun_pid(paths) {
        if pid_alive(pid) {
            anyhow::bail!(
                "worklog web is already running (pid {pid}). \
                 Stop it with `worklog web down` first."
            );
        }
        // stale pidfile — clean it up before continuing.
        let _ = std::fs::remove_file(bun_pid_path(paths));
    }

    // Check bun is on PATH by invoking `bun --version`. Cheap and
    // doesn't pull in a new crate.
    let bun_ok = Command::new("bun")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !bun_ok {
        anyhow::bail!(
            "can't find `bun` on PATH. Install Bun (https://bun.sh) — \
             the web UI now runs directly on your machine, no Docker."
        );
    }

    // Run `bun install` if node_modules is missing — otherwise next-build
    // will explode in a way that's hard to diagnose for a first-time user.
    if !web_context.join("node_modules").is_dir() {
        let mut cmd = Command::new("bun");
        cmd.current_dir(web_context)
            .args(["install", "--frozen-lockfile"]);
        run_inherit(cmd).context("running `bun install`")?;
    }

    // Always rebuild on `up` — the production build caches aggressively
    // and a stale .next dir would silently serve the previous version
    // of the UI after any web/ edit. Next caches per-file under
    // .next/cache so the rebuild is incremental (~3-5s after the first).
    {
        let mut cmd = Command::new("bun");
        cmd.current_dir(web_context)
            .env("NEXT_TELEMETRY_DISABLED", "1")
            .args(["run", "build"]);
        run_inherit(cmd).context("running `bun run build`")?;
    }

    // Open the log file (truncate per up).
    std::fs::create_dir_all(&paths.log_dir).ok();
    let log = std::fs::File::create(bun_log_path(paths))
        .with_context(|| format!("creating {}", bun_log_path(paths).display()))?;
    let log_err = log.try_clone()?;

    let child = Command::new("bun")
        .current_dir(web_context)
        // bun runs `next start -p 3333` from package.json; pass PORT to
        // override, and set HOSTNAME so the server binds 127.0.0.1.
        .env("PORT", port.to_string())
        .env("HOSTNAME", "127.0.0.1")
        // The web client talks to the daemon over TCP (next start runs
        // under Node internally; Bun's unix-socket fetch shim doesn't
        // apply). Daemon already binds 127.0.0.1:9323 by default.
        .env("WORKLOG_DAEMON_URL", "http://127.0.0.1:9323")
        .env("NEXT_TELEMETRY_DISABLED", "1")
        .args(["run", "start"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .context("spawning `bun run start`")?;

    let pid = child.id();
    std::fs::write(bun_pid_path(paths), pid.to_string())
        .with_context(|| format!("writing {}", bun_pid_path(paths).display()))?;
    // Drop the child handle — we tracked it via pid file. The process
    // outlives this CLI invocation.
    std::mem::forget(child);

    // Wait up to 30s for the server to respond. Without this the user
    // hits the URL too early and sees an "ECONNREFUSED" page.
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 30 {
        if !pid_alive(pid) {
            anyhow::bail!(
                "bun exited while starting — tail {} for the error",
                bun_log_path(paths).display()
            );
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    anyhow::bail!(
        "bun started but port {port} didn't come up within 30s. \
         Check {} for clues.",
        bun_log_path(paths).display()
    )
}

/// Stop the bun web runner. No-op if no pid file or the process is
/// already dead.
pub fn bun_down(paths: &Paths) -> Result<()> {
    let Some(pid) = read_bun_pid(paths) else {
        return Ok(());
    };
    if pid_alive(pid) {
        // SIGTERM first; bun handles it gracefully.
        let _ = Command::new("kill").arg(pid.to_string()).status();
        // Give it 2 seconds, then SIGKILL if it's still around.
        for _ in 0..20 {
            if !pid_alive(pid) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if pid_alive(pid) {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        }
    }
    let _ = std::fs::remove_file(bun_pid_path(paths));
    Ok(())
}

/// Bun-runner status, mirrors the Docker `status()` shape so the CLI
/// can route either path through the same formatter.
pub fn bun_status(paths: &Paths) -> WebStatus {
    let pid = read_bun_pid(paths);
    let running = pid.map(pid_alive).unwrap_or(false);
    WebStatus {
        running,
        container: pid.map(|p| format!("bun pid {p}")),
        image: Some("bun (host)".into()),
        port: None,
        uptime: None,
    }
}

fn read_bun_pid(paths: &Paths) -> Option<u32> {
    std::fs::read_to_string(bun_pid_path(paths))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn pid_alive(pid: u32) -> bool {
    // `kill -0` checks process existence without sending a signal.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Embedded docker-compose.yml written out into the data dir on first
/// `worklog web up`. Kept in-source so a pure `cargo install` of the
/// binary still has everything it needs — no ambient web/ checkout
/// required at runtime.
pub const COMPOSE_TEMPLATE: &str = include_str!("../templates/docker-compose.yml");

/// Where we render the effective compose file. Lives beside the DB so
/// users who `docker compose ...` it directly see the same mounts.
pub fn compose_path(paths: &Paths) -> PathBuf {
    paths.data_dir.join("docker-compose.yml")
}

#[derive(Debug, Serialize)]
pub struct WebStatus {
    pub running: bool,
    pub container: Option<String>,
    pub image: Option<String>,
    pub port: Option<u16>,
    pub uptime: Option<String>,
}

/// Check whether docker is on PATH AND the daemon is reachable.
/// `docker version` exits non-zero when the client can't reach the
/// daemon — we capture stderr so the user gets the actual reason
/// ("Cannot connect to the Docker daemon. Is it running?") instead of
/// a generic "exited 1".
pub fn preflight_docker() -> Result<()> {
    let out = Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format_docker_preflight_error(
                &o.status.to_string(),
                &stderr,
            ))
        }
        Err(e) => anyhow::bail!(
            "can't find docker on PATH ({e}). Install Docker Desktop \
             or colima, then run `worklog web up` again."
        ),
    }
}

/// Render the user-facing error message for a failed `docker version`
/// invocation. Pure function so tests can exercise the stderr-hint
/// branch without mocking a subprocess.
pub(crate) fn format_docker_preflight_error(status: &str, stderr: &str) -> anyhow::Error {
    let hint = if stderr.to_lowercase().contains("cannot connect") {
        "\n   The Docker daemon isn't running — start Docker Desktop or `colima start`."
    } else {
        ""
    };
    anyhow::anyhow!("`docker version` exited {status}: {}{hint}", stderr.trim())
}

/// Render the compose template into the data dir, overwriting any
/// previous copy. The template references `${WORKLOG_HOME}` so the
/// mount always points at the user's real data dir.
pub fn render_compose(paths: &Paths, port: u16, web_context: &Path) -> Result<PathBuf> {
    let dest = compose_path(paths);
    let data_dir = paths
        .data_dir
        .to_str()
        .context("data dir path isn't valid UTF-8")?;
    let ctx_str = web_context
        .to_str()
        .context("web context path isn't valid UTF-8")?;
    let body = COMPOSE_TEMPLATE
        .replace("{{WORKLOG_DATA_DIR}}", data_dir)
        .replace("{{WEB_CONTEXT}}", ctx_str)
        .replace("{{PORT}}", &port.to_string());
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&dest, body).with_context(|| format!("writing {}", dest.display()))?;
    Ok(dest)
}

/// Find the `web/` directory that holds the Dockerfile. Resolution order:
///   1. `$WORKLOG_WEB_DIR` env override
///   2. `<cwd>/web` (most common: running from repo root)
///   3. walk up from cwd looking for a `web/Dockerfile`
///   4. `<paths.data_dir>/web` (cache populated by `web::fetch`)
///   5. `/usr/local/share/worklog/web` (FHS system install)
///   6. *auto-fetch* the archive from GitHub into `<paths.data_dir>/web`
///
/// Only step 6 hits the network; the other five are filesystem checks
/// that complete in microseconds.
pub fn resolve_web_context(paths: &Paths) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("WORKLOG_WEB_DIR") {
        let p = PathBuf::from(dir);
        if p.join("Dockerfile").is_file() {
            return std::fs::canonicalize(&p).context("canonicalising WORKLOG_WEB_DIR");
        }
        anyhow::bail!("WORKLOG_WEB_DIR={} has no Dockerfile", p.display());
    }
    let cwd = std::env::current_dir().context("getting cwd")?;
    let mut cur: Option<&Path> = Some(&cwd);
    while let Some(dir) = cur {
        let candidate = dir.join("web");
        if candidate.join("Dockerfile").is_file() {
            return std::fs::canonicalize(&candidate).context("canonicalising web/");
        }
        if dir.join("Dockerfile").is_file() && dir.file_name().is_some_and(|n| n == "web") {
            return std::fs::canonicalize(dir).context("canonicalising cwd");
        }
        cur = dir.parent();
    }
    // Cache populated by a previous `worklog web fetch` (or an earlier
    // auto-fetch). This is the warm path for users who installed via
    // install.sh and never cloned the repo.
    let cache = fetch::cache_dir(paths);
    if cache.join("Dockerfile").is_file() {
        return std::fs::canonicalize(&cache).context("canonicalising web cache");
    }
    // FHS location for packaged installs (if a distro decides to ship
    // the worklog web tree under /usr/local/share).
    let prefix = PathBuf::from("/usr/local/share/worklog/web");
    if prefix.join("Dockerfile").is_file() {
        return Ok(prefix);
    }
    // Nothing on disk — pull from GitHub. One network hit per install
    // or per upgrade, zero user effort.
    let version = env!("CARGO_PKG_VERSION");
    tracing::info!(version, "web: no local tree found, auto-fetching");
    fetch::fetch_to_cache(paths, version).context(
        "auto-fetching web/ from the github archive failed. \
         Set $WORKLOG_WEB_DIR to a local checkout of the worklog repo, \
         or run `worklog web fetch` manually once you have network.",
    )
}

/// `docker compose -f <path> up -d` — bring the service up.
pub fn compose_up(compose: &Path, pull: bool) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "-f"]).arg(compose);
    if pull {
        cmd.args(["pull"]);
        run_inherit(cmd)?;
        let mut cmd = Command::new("docker");
        cmd.args(["compose", "-f"]).arg(compose).args(["up", "-d"]);
        return run_inherit(cmd);
    }
    cmd.args(["up", "-d", "--build"]);
    run_inherit(cmd)
}

/// `docker compose -f <path> down` — stop and remove the service.
pub fn compose_down(compose: &Path) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "-f"]).arg(compose).args(["down"]);
    run_inherit(cmd)
}

/// `docker compose -f <path> logs -f --tail=<n>`
pub fn compose_logs(compose: &Path, tail: u32) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "-f"])
        .arg(compose)
        .args(["logs", "-f", &format!("--tail={tail}")]);
    run_inherit(cmd)
}

/// `docker compose -f <path> build`
pub fn compose_build(compose: &Path, pull: bool) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "-f"]).arg(compose).arg("build");
    if pull {
        cmd.arg("--pull");
    }
    run_inherit(cmd)
}

/// Status from `docker inspect`. Distinguishes three cases:
///   - container is running → `WebStatus { running: true, ... }`
///   - container doesn't exist / is stopped → `WebStatus { running: false, .. }`
///   - docker daemon itself unreachable → `Err(...)` so the CLI can tell
///     the user "start Docker Desktop" instead of the misleading
///     "not running, try `worklog web up`".
pub fn status() -> Result<WebStatus> {
    let out = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{.State.Running}}|{{.State.StartedAt}}|{{.Config.Image}}|{{index .NetworkSettings.Ports \"3000/tcp\" 0 \"HostPort\"}}",
            CONTAINER_NAME,
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let line = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let parts: Vec<&str> = line.split('|').collect();
            let running = parts.first().is_some_and(|s| *s == "true");
            Ok(WebStatus {
                running,
                container: Some(CONTAINER_NAME.to_string()),
                image: parts.get(2).map(|s| s.to_string()),
                port: parts.get(3).and_then(|s| s.parse().ok()),
                uptime: parts.get(1).map(|s| s.to_string()),
            })
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.to_lowercase().contains("cannot connect") {
                anyhow::bail!(
                    "docker inspect failed — the Docker daemon isn't running. \
                     Start Docker Desktop (or `colima start`) and retry."
                );
            }
            // "No such container" and similar — the container just isn't there.
            Ok(WebStatus {
                running: false,
                container: None,
                image: None,
                port: None,
                uptime: None,
            })
        }
        Err(e) => anyhow::bail!("spawning docker: {e}"),
    }
}

fn run_inherit(mut cmd: Command) -> Result<()> {
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("spawning docker")?;
    if !status.success() {
        anyhow::bail!("docker exited {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths_in(dir: &TempDir) -> Paths {
        let root = dir.path().to_path_buf();
        Paths {
            root: root.clone(),
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            db: root.join("data/worklog.db"),
            socket: root.join("data/api.sock"),
            config: root.join("config/config.toml"),
            env_file: root.join("config/.env"),
            bin_dir: root.join("bin"),
            releases: root.join("releases"),
            log_dir: root.join("log"),
        }
    }

    #[test]
    fn render_compose_substitutes_placeholders() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(&tmp);
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let web_ctx = tmp.path().join("web");
        std::fs::create_dir_all(&web_ctx).unwrap();
        let dest = render_compose(&paths, 3333, &web_ctx).unwrap();
        let body = std::fs::read_to_string(&dest).unwrap();
        assert!(
            body.contains(paths.data_dir.to_str().unwrap()),
            "compose should reference the data dir: {body}"
        );
        assert!(
            body.contains("3333:3000"),
            "port mapping should be rendered"
        );
        assert!(
            body.contains(web_ctx.to_str().unwrap()),
            "compose should reference the web build context"
        );
        assert!(
            !body.contains("{{WORKLOG_DATA_DIR}}"),
            "template placeholder should be replaced"
        );
        assert!(
            !body.contains("{{WEB_CONTEXT}}"),
            "web-context placeholder should be replaced"
        );
    }

    #[test]
    fn resolve_web_context_honours_env_var() {
        let tmp = TempDir::new().unwrap();
        let web = tmp.path().join("mysite");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("Dockerfile"), "FROM scratch\n").unwrap();
        std::env::set_var("WORKLOG_WEB_DIR", &web);
        let paths = paths_in(&tmp);
        let got = resolve_web_context(&paths).unwrap();
        std::env::remove_var("WORKLOG_WEB_DIR");
        assert_eq!(got, std::fs::canonicalize(&web).unwrap());
    }

    #[test]
    fn resolve_web_context_falls_back_to_cache_when_populated() {
        // New behaviour: a populated cache dir rescues a user who isn't
        // in the repo and hasn't set WORKLOG_WEB_DIR.
        std::env::remove_var("WORKLOG_WEB_DIR");
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let paths = paths_in(&tmp);
        // Pre-populate the cache path to simulate a prior fetch.
        let cache = fetch::cache_dir(&paths);
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("Dockerfile"), "FROM bun").unwrap();

        let got = resolve_web_context(&paths).unwrap();
        std::env::set_current_dir(prev).unwrap();
        assert_eq!(got, std::fs::canonicalize(&cache).unwrap());
    }

    #[test]
    fn compose_path_is_inside_data_dir() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(&tmp);
        let p = compose_path(&paths);
        assert!(p.starts_with(&paths.data_dir));
        assert!(p.ends_with("docker-compose.yml"));
    }

    #[test]
    fn preflight_error_includes_start_hint_when_daemon_off() {
        // `docker version` with the daemon stopped prints "Cannot connect
        // to the Docker daemon" to stderr. Our error message must include
        // the actionable hint instead of just "exited 1".
        let err = format_docker_preflight_error(
            "exit status: 1",
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
        );
        let msg = format!("{err}");
        assert!(
            msg.contains("start Docker Desktop") || msg.contains("colima"),
            "missing actionable hint: {msg}"
        );
        assert!(
            msg.contains("Cannot connect"),
            "original stderr must be preserved"
        );
    }

    #[test]
    fn preflight_error_without_connect_keyword_has_no_hint() {
        // A different docker failure (e.g. permission denied on the
        // socket) shouldn't get the "start Docker Desktop" hint — it'd
        // be misleading. The "permission denied" message doesn't
        // contain "cannot connect" so the hint branch should not fire.
        let err = format_docker_preflight_error(
            "exit status: 126",
            "permission denied while trying to access the Docker socket",
        );
        let msg = format!("{err}");
        assert!(
            !msg.contains("start Docker Desktop"),
            "hint must not misfire"
        );
        assert!(!msg.contains("colima"), "hint must not misfire");
    }
}
