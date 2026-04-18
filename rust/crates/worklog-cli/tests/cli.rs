//! End-to-end tests of the `worklog` binary. Each test points
//! WORKLOG_HOME at a fresh temp dir so we never touch the user's real data
//! or keychain.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("worklog").expect("binary built");
    c.env("WORKLOG_HOME", home.path());
    // Divert secrets into a file under the tempdir so tests never touch the
    // real keychain.
    c.env("WORKLOG_SECRETS_FILE", home.path().join("secrets.json"));
    // Redirect the Claude Code settings and the scheduler into the tempdir
    // so neither the user's ~/.claude nor ~/Library/LaunchAgents gets
    // touched by `cargo test`.
    c.env("CLAUDE_HOME", home.path().join("claude"));
    c.env("WORKLOG_SCHEDULE_HOME", home.path());
    // Shut up tracing during tests.
    c.env("RUST_LOG", "error");
    c
}

#[test]
fn version_prints_a_semver_line() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("worklog "));
}

#[test]
fn version_json_emits_structured_payload() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["--json", "version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"version\""))
        .stdout(predicate::str::contains("\"core\""));
}

#[test]
fn db_migrate_creates_database_file() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["db", "migrate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("db ready"));
    assert!(home.path().join("worklog.db").is_file());
}

#[test]
fn db_migrate_is_idempotent() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home).args(["db", "migrate"]).assert().success();
    assert!(home.path().join("worklog.db").is_file());
}

#[test]
fn db_info_errors_before_migrate() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["db", "info"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn db_info_reports_zero_counts_on_fresh_db() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["db", "info"])
        .assert()
        .success()
        .stdout(predicate::str::contains("events=0"))
        .stdout(predicate::str::contains("blocks=0"));
}

#[test]
fn db_path_prints_resolved_path() {
    let home = TempDir::new().unwrap();
    let expected = home.path().join("worklog.db");
    cmd(&home)
        .args(["db", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            expected.to_string_lossy().as_ref(),
        ));
}

#[test]
fn doctor_runs_without_db_and_lists_secrets() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("secrets"));
}

#[test]
fn doctor_json_has_top_level_fields() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["--json", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"home\""))
        .stdout(predicate::str::contains("\"db_path\""))
        .stdout(predicate::str::contains("\"secrets\""));
}

#[test]
fn secret_set_via_value_flag_and_get_roundtrip() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["secret", "set", "jira_email", "--value", "tomas@p5.is"])
        .assert()
        .success()
        .stdout(predicate::str::contains("saved to keychain"));

    cmd(&home)
        .args(["secret", "get", "jira_email"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tomas@p5.is"));
}

#[test]
fn secret_list_shows_state() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["secret", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("jira_email"));
}

#[test]
fn setup_non_interactive_creates_db_and_exits_clean() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["setup", "--non-interactive", "--skip-validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog setup"))
        .stdout(predicate::str::contains("database ready"));
    assert!(home.path().join("worklog.db").is_file());
}

#[test]
fn setup_is_idempotent() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["setup", "--non-interactive", "--skip-validate"])
        .assert()
        .success();
    cmd(&home)
        .args(["setup", "--non-interactive", "--skip-validate"])
        .assert()
        .success();
}

#[test]
fn hook_install_status_uninstall_roundtrip() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["hook", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed: no"));

    cmd(&home)
        .args(["hook", "install", "--command", "/tmp/fake-worklog hook run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hook installed"));

    cmd(&home)
        .args(["hook", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed: yes"))
        .stdout(predicate::str::contains("/tmp/fake-worklog hook run"));

    cmd(&home)
        .args(["hook", "uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hook removed"));

    cmd(&home)
        .args(["hook", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed: no"));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn schedule_install_status_uninstall_roundtrip() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["schedule", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed: no"));

    cmd(&home)
        .args([
            "schedule",
            "install",
            "--interval",
            "15m",
            "--command",
            "/usr/bin/true",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("schedule installed"));

    cmd(&home)
        .args(["schedule", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed: yes"))
        .stdout(predicate::str::contains("every 15m"));

    cmd(&home)
        .args(["schedule", "uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains("schedule removed"));
}

#[test]
fn schedule_install_rejects_subminute_interval() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["schedule", "install", "--interval", "30"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least 60 seconds"));
}

#[test]
fn secret_rm_reports_absent_cleanly() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["secret", "rm", "nonexistent_key"])
        .assert()
        .success();
}
