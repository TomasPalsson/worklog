//! Live daemon integration test. Spawns `worklog daemon` in a subprocess
//! pointed at a tempdir, then hits `/health` + `/blocks/:day` with
//! `curl --unix-socket`. The daemon is killed on drop so nothing leaks.

use assert_cmd::Command as AssertCmd;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Guard that kills + waits the daemon child so stray processes can't
/// survive a test-case panic.
struct DaemonGuard(Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("worklog")
}

fn wait_for_socket(path: &std::path::Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            // Quick probe: a dry connect to make sure the server is
            // actually answering, not just that the socket file exists.
            let out = Command::new("curl")
                .args([
                    "-sS",
                    "--unix-socket",
                    path.to_str().unwrap(),
                    "http://localhost/health",
                ])
                .output();
            if let Ok(o) = out {
                if o.status.success() && !o.stdout.is_empty() {
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn spawn_daemon(home: &TempDir) -> (DaemonGuard, std::path::PathBuf) {
    // Migrate first so the daemon has a db.
    AssertCmd::cargo_bin("worklog")
        .unwrap()
        .env("WORKLOG_HOME", home.path())
        .env("WORKLOG_SECRETS_FILE", home.path().join("secrets.json"))
        .env("CLAUDE_HOME", home.path().join("claude"))
        .env("WORKLOG_SCHEDULE_HOME", home.path())
        .env("WORKLOG_ENV_FILE", home.path().join("absent.env"))
        .args(["db", "migrate"])
        .assert()
        .success();

    // Seed one block so `/blocks/:day` has something to return.
    let conn = rusqlite::Connection::open(home.path().join("worklog.db")).unwrap();
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800)",
        [],
    )
    .unwrap();
    drop(conn);

    let socket = home.path().join("api.sock");
    let child = Command::new(bin())
        .env("WORKLOG_HOME", home.path())
        .env("WORKLOG_SECRETS_FILE", home.path().join("secrets.json"))
        .env("CLAUDE_HOME", home.path().join("claude"))
        .env("WORKLOG_SCHEDULE_HOME", home.path())
        .env("WORKLOG_ENV_FILE", home.path().join("absent.env"))
        .env("RUST_LOG", "error")
        .args(["daemon", "--socket", socket.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");

    if !wait_for_socket(&socket, Duration::from_secs(5)) {
        panic!("daemon socket never became ready");
    }
    (DaemonGuard(child), socket)
}

fn curl(socket: &std::path::Path, method: &str, path: &str, body: Option<&str>) -> String {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "--unix-socket",
        socket.to_str().unwrap(),
        "-X",
        method,
        &format!("http://localhost{path}"),
    ]);
    if let Some(b) = body {
        cmd.args(["-H", "content-type: application/json", "-d", b]);
    }
    let out = cmd.output().expect("curl");
    assert!(
        out.status.success(),
        "curl failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn health_endpoint_reports_ok() {
    let home = TempDir::new().unwrap();
    let (_g, socket) = spawn_daemon(&home);
    let body = curl(&socket, "GET", "/health", None);
    assert!(body.contains("\"ok\""));
    assert!(body.contains("\"version\""));
}

#[test]
fn blocks_endpoint_lists_day() {
    let home = TempDir::new().unwrap();
    let (_g, socket) = spawn_daemon(&home);
    let body = curl(&socket, "GET", "/blocks/2026-04-18", None);
    assert!(body.contains("\"duration_seconds\":1800"));
}

#[test]
fn assign_ticket_and_description_persist_across_calls() {
    let home = TempDir::new().unwrap();
    let (_g, socket) = spawn_daemon(&home);
    curl(
        &socket,
        "POST",
        "/blocks/1/ticket",
        Some(r#"{"jira_issue":"PROJ-9"}"#),
    );
    curl(
        &socket,
        "POST",
        "/blocks/1/description",
        Some(r#"{"description":"Audit auth flow"}"#),
    );
    let body = curl(&socket, "GET", "/blocks/2026-04-18", None);
    assert!(body.contains("\"PROJ-9\""));
    assert!(body.contains("Audit auth flow"));
}

#[test]
fn delete_endpoint_removes_block() {
    let home = TempDir::new().unwrap();
    let (_g, socket) = spawn_daemon(&home);
    curl(&socket, "POST", "/blocks/1/delete", None);
    let body = curl(&socket, "GET", "/blocks/2026-04-18", None);
    assert_eq!(body.trim(), "[]");
}
