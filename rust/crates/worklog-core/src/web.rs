//! `worklog web` — orchestration for the dockerised Next.js UI.
//!
//! The web container bind-mounts the worklog data directory so it can
//! read the SQLite DB directly (bun:sqlite) and talk to the Rust daemon
//! over its unix socket. This module wraps `docker` / `docker compose`
//! and the rendered-out compose file so the CLI doesn't need to know
//! anything about Docker internals.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::paths::Paths;

pub const CONTAINER_NAME: &str = "worklog-web";
pub const IMAGE: &str = "worklog-web:latest";

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
            Err(format_docker_preflight_error(&o.status.to_string(), &stderr))
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
    anyhow::anyhow!(
        "`docker version` exited {status}: {}{hint}",
        stderr.trim()
    )
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
///   4. `<install prefix>/share/worklog/web` (packaged install)
///
/// Returns a canonical absolute path if found, else an error explaining
/// what to do.
pub fn resolve_web_context() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("WORKLOG_WEB_DIR") {
        let p = PathBuf::from(dir);
        if p.join("Dockerfile").is_file() {
            return std::fs::canonicalize(&p).context("canonicalising WORKLOG_WEB_DIR");
        }
        anyhow::bail!(
            "WORKLOG_WEB_DIR={} has no Dockerfile",
            p.display()
        );
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
    // Last-ditch FHS-style location. Users who `uv tool install` us and
    // want to run the web container should symlink their web/ here.
    let prefix = PathBuf::from("/usr/local/share/worklog/web");
    if prefix.join("Dockerfile").is_file() {
        return Ok(prefix);
    }
    anyhow::bail!(
        "couldn't find the worklog `web/` directory. Set WORKLOG_WEB_DIR to \
         the path of the web/ folder in the worklog repo, or cd into the repo \
         and re-run."
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
        assert!(body.contains("3333:3000"), "port mapping should be rendered");
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
        let got = resolve_web_context().unwrap();
        std::env::remove_var("WORKLOG_WEB_DIR");
        assert_eq!(got, std::fs::canonicalize(&web).unwrap());
    }

    #[test]
    fn resolve_web_context_errors_when_missing() {
        // Make sure env var and cwd don't rescue us.
        std::env::remove_var("WORKLOG_WEB_DIR");
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let err = resolve_web_context().unwrap_err();
        std::env::set_current_dir(prev).unwrap();
        let msg = format!("{err}");
        assert!(msg.contains("web/"), "error should mention web/: {msg}");
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
        assert!(msg.contains("Cannot connect"), "original stderr must be preserved");
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
        assert!(!msg.contains("start Docker Desktop"), "hint must not misfire");
        assert!(!msg.contains("colima"), "hint must not misfire");
    }
}
