//! Integration tests: run the hook as a subprocess, feed JSON on stdin,
//! verify SQLite side-effects. Red-phase tests — impl does not exist yet.

use rusqlite::Connection;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn binary_path() -> PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for [[bin]] targets in integration tests.
    PathBuf::from(env!("CARGO_BIN_EXE_worklog-hook"))
}

fn seed_schema(db: &Path) {
    let schema = include_str!("../../../src/worklog/schema.sql");
    let conn = Connection::open(db).expect("open db");
    conn.execute_batch(schema).expect("apply schema");
}

fn run_hook(db_path: &Path, config_dir: &Path, payload: &str) -> std::process::Output {
    let mut child = Command::new(binary_path())
        .env("WORKLOG_DB_PATH", db_path)
        .env("WORKLOG_CONFIG_DIR", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait")
}

#[test]
fn session_start_inserts_event_and_session() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("worklog.db");
    seed_schema(&db);
    let payload = r#"{
        "hook_event_name": "SessionStart",
        "session_id": "sess-rust-1",
        "cwd": "/tmp/proj",
        "transcript_path": "/tmp/t.jsonl",
        "source": "startup"
    }"#;

    let out = run_hook(&db, tmp.path(), payload);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "hook must not print to stdout");

    let conn = Connection::open(&db).unwrap();
    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = 'sess-rust-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(event_count, 1);

    let session_ended_at: Option<String> = conn
        .query_row(
            "SELECT ended_at FROM sessions WHERE session_id = 'sess-rust-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        session_ended_at.is_none(),
        "SessionStart should leave ended_at NULL"
    );
}

#[test]
fn stop_closes_session() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("worklog.db");
    seed_schema(&db);

    run_hook(
        &db,
        tmp.path(),
        r#"{"hook_event_name":"SessionStart","session_id":"s-stop","cwd":"/t","source":"startup"}"#,
    );
    let out = run_hook(
        &db,
        tmp.path(),
        r#"{"hook_event_name":"Stop","session_id":"s-stop","cwd":"/t"}"#,
    );
    assert_eq!(out.status.code(), Some(0));

    let conn = Connection::open(&db).unwrap();
    let end_source: String = conn
        .query_row(
            "SELECT end_source FROM sessions WHERE session_id = 's-stop'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(end_source, "stop");
}

#[test]
fn malformed_json_exits_zero_with_stderr() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("worklog.db");
    seed_schema(&db);

    let out = run_hook(&db, tmp.path(), "this is not JSON");
    assert_eq!(
        out.status.code(),
        Some(0),
        "hook MUST exit 0 even on bad input"
    );
    assert!(
        !out.stderr.is_empty(),
        "bad input should produce stderr warning"
    );
}

#[test]
fn missing_database_file_is_created() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("nonexistent.db");
    // Do NOT seed schema — hook must CREATE TABLE IF NOT EXISTS

    let out = run_hook(
        &db,
        tmp.path(),
        r#"{"hook_event_name":"SessionStart","session_id":"auto-create","cwd":"/t","source":"startup"}"#,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(db.exists(), "DB file should have been created");

    let conn = Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn jira_key_extracted_from_prompt() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("worklog.db");
    seed_schema(&db);

    run_hook(
        &db,
        tmp.path(),
        r#"{
            "hook_event_name":"UserPromptSubmit",
            "session_id":"jira-1",
            "cwd":"/t",
            "prompt":"Fix the bug in ACME-123 that Alice reported"
        }"#,
    );

    let conn = Connection::open(&db).unwrap();
    let jira: Option<String> = conn
        .query_row(
            "SELECT jira_issue FROM events WHERE session_id = 'jira-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(jira.as_deref(), Some("ACME-123"));
}

#[test]
fn concurrent_writes_both_succeed() {
    // WAL + busy_timeout should let two invocations serialize without errors.
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("worklog.db");
    seed_schema(&db);

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let db = db.clone();
            let cfg = tmp.path().to_path_buf();
            std::thread::spawn(move || {
                let payload = format!(
                    r#"{{"hook_event_name":"UserPromptSubmit","session_id":"c-{i}","cwd":"/t"}}"#
                );
                run_hook(&db, &cfg, &payload)
            })
        })
        .collect();
    for h in handles {
        let out = h.join().unwrap();
        assert_eq!(out.status.code(), Some(0));
    }
    let conn = Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE source = 'claude'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 5);
}
