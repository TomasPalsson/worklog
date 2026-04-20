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
    // Insulate tests from the developer's real ~/.config/worklog/.env.
    c.env("WORKLOG_ENV_FILE", home.path().join("absent.env"));
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
fn collect_skips_sources_when_credentials_missing() {
    // No secrets set — wizard never run, .env pointed at absent file.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["collect", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("jira skipped"))
        .stdout(predicate::str::contains("github skipped"));
}

#[test]
fn sync_errors_without_db() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["sync", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("db not initialized"));
}

#[test]
fn sync_dry_run_reports_zero_when_no_blocks() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["sync", "--day", "2026-04-18", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 synced"))
        .stdout(predicate::str::contains("2026-04-18"));
}

#[test]
fn infer_on_empty_day_prints_zero_blocks() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["infer", "--day", "2026-04-18"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 blocks"))
        .stdout(predicate::str::contains("0 min"));
}

#[test]
fn estimate_errors_without_db() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["estimate", "--day", "2026-04-18"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn hook_run_writes_claude_event_from_stdin() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .arg("hook-run")
        .write_stdin(r#"{"hook_event_name":"UserPromptSubmit","session_id":"S1","cwd":"/tmp/x","user_prompt":"see PROJ-42"}"#)
        .assert()
        .success();
    // Read back via the worklog db.
    cmd(&home)
        .args(["db", "info"])
        .assert()
        .success()
        .stdout(predicate::str::contains("events=1"))
        .stdout(predicate::str::contains("sessions=1"));
}

#[test]
fn hook_run_exits_zero_on_malformed_stdin() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .arg("hook-run")
        .write_stdin("not json at all")
        .assert()
        .success(); // Never blocks Claude.
}

#[test]
fn secret_rm_reports_absent_cleanly() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["secret", "rm", "nonexistent_key"])
        .assert()
        .success();
}

// ─────────────────────────── `worklog day` ───────────────────────────

#[test]
fn day_no_serve_runs_full_pipeline_on_empty_day() {
    // No creds → collectors skip; no blocks → infer=0, estimate=0. The test
    // asserts that every stage heading appears and the command exits 0
    // without waiting on Docker or opening a browser.
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["day", "--day", "2026-04-18", "--no-serve"])
        .assert()
        .success()
        .stdout(predicate::str::contains("collecting"))
        .stdout(predicate::str::contains("inferring"))
        .stdout(predicate::str::contains("estimating"))
        // Web UI must be suppressed by --no-serve.
        .stdout(predicate::str::contains("review UI").not())
        // 0 blocks on an empty day — mirrors `infer`'s own output.
        .stdout(predicate::str::contains("0 blocks"));
}

#[test]
fn day_continues_past_collect_failures() {
    // With no credentials we expect each collector to surface a skip line
    // AND the flow to reach infer+estimate anyway — matches the Python
    // `[yellow]!` behaviour (report, don't abort).
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["day", "--day", "2026-04-18", "--no-serve"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("inferring"))
        .stdout(predicate::str::contains("estimating"));
}

// ─────────── help + version exit codes (DisplayHelp/DisplayVersion) ───────────

#[test]
fn top_level_help_flag_exits_zero() {
    // `worklog --help` is not an error. Clap's DisplayHelp flows through
    // try_parse_from as Err(...) by default; we special-case it in run_with
    // so the binary returns 0 and the help text goes to stdout, matching
    // every other well-behaved CLI.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Personal worklog"))
        .stdout(predicate::str::contains("day"))
        .stdout(predicate::str::contains("web"));
}

#[test]
fn top_level_help_subcommand_exits_zero() {
    // `worklog help` (the subcommand — clap synthesises it) should behave
    // identically to `--help`, including exit 0.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Personal worklog"));
}

#[test]
fn version_flag_exits_zero() {
    // `--version` uses ErrorKind::DisplayVersion which is a separate code
    // path from DisplayHelp; test it explicitly so neither regresses.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog "));
}

#[test]
fn subcommand_help_flag_exits_zero() {
    // `worklog day --help` — sub-command help.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["day", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--no-serve"));
}

#[test]
fn web_without_subcommand_exits_zero_and_prints_help() {
    // `worklog web` (no sub) — clap auto-renders usage + subcommands for
    // groups with required sub. Must exit 0 like `--help`.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["web"])
        .assert()
        .success()
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("down"));
}

// ─────────────────────── `serve` and `upgrade` aliases ───────────────────────

#[test]
fn serve_alias_exists_and_delegates_to_web_up() {
    // `worklog serve` is a compat alias for `worklog web up`. We don't
    // want the dev server to actually spin up in tests, so just verify
    // `serve --help` resolves — proves the variant exists.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["serve", "--help"])
        .assert()
        .success()
        // Help should mention the port flag to confirm it's aliased to web up.
        .stdout(predicate::str::contains("--port"));
}

#[test]
fn upgrade_alias_exists_and_delegates_to_self_update() {
    // `worklog upgrade` is a compat alias for `worklog self-update`.
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["upgrade", "--help"])
        .assert()
        .success()
        // self-update's --check flag is the canary we assert on.
        .stdout(predicate::str::contains("--check"));
}

// ─────────────── estimator provider surface (v0.7 — Phase 5) ───────────────

/// `worklog doctor --json` must carry an `"estimator"` block so the
/// user (or a monitoring script) can see which provider is wired up
/// without re-deriving it from env + secrets. Defaults to
/// `claude_subprocess` on a fresh install — it's the back-compat path.
#[test]
fn doctor_json_reports_estimator_provider_block() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args(["--json", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"estimator\""))
        .stdout(predicate::str::contains("\"provider\""))
        .stdout(predicate::str::contains("claude_subprocess"));
}

/// When the user selects LiteLLM via env, doctor should report
/// `provider: litellm` and carry the configured base_url + model.
/// Reachability will be false here (the test URL points at an unused
/// port); we assert the field is present, not the bool value.
#[test]
fn doctor_json_reports_litellm_provider_when_selected() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args([
            "secret",
            "set",
            "litellm_base_url",
            "--value",
            "http://127.0.0.1:1",
        ])
        .assert()
        .success();
    cmd(&home)
        .args([
            "secret",
            "set",
            "litellm_model",
            "--value",
            "anthropic/claude-haiku-4-5",
        ])
        .assert()
        .success();
    cmd(&home)
        .env("WORKLOG_ESTIMATOR_PROVIDER", "litellm")
        // --probe adds the live reachability check; without it doctor
        // stays fast + offline-safe.
        .args(["--json", "doctor", "--probe"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"provider\": \"litellm\""))
        .stdout(predicate::str::contains("\"base_url\""))
        .stdout(predicate::str::contains("\"reachable\""))
        .stdout(predicate::str::contains(
            "\"model\": \"anthropic/claude-haiku-4-5\"",
        ));
}

/// The default `worklog doctor` (no --probe) does NOT hit the network.
/// Scripted/monitoring callers invoke this path thousands of times a
/// day; a 3s HTTP tax would be a UX regression. `reachable` must be
/// absent from the JSON when probing is skipped.
#[test]
fn doctor_json_skips_probe_by_default() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["db", "migrate"]).assert().success();
    cmd(&home)
        .args([
            "secret",
            "set",
            "litellm_base_url",
            "--value",
            "http://127.0.0.1:1",
        ])
        .assert()
        .success();
    cmd(&home)
        .env("WORKLOG_ESTIMATOR_PROVIDER", "litellm")
        .args(["--json", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"provider\": \"litellm\""))
        // reachable skipped → `skip_serializing_if` drops the field.
        .stdout(predicate::str::contains("\"reachable\"").not());
}

/// `worklog estimate --help` needs a `long_about` that names the
/// WORKLOG_ESTIMATOR_PROVIDER env var — otherwise users who set it
/// (per the README) have no in-CLI way to discover the valid values.
#[test]
fn estimate_help_documents_provider_env_var() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["estimate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("WORKLOG_ESTIMATOR_PROVIDER"))
        .stdout(predicate::str::contains("litellm"));
}
