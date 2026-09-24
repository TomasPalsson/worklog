use super::*;
use crate::db::open_memory;
use std::io::Write;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

fn write_fixture(contents: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    f
}

#[test]
fn program_name_skips_env_assignment_prefix() {
    assert_eq!(
        program_name("AWS_SECRET_ACCESS_KEY=sk-live-superSecret123 aws configure"),
        "aws"
    );
}

#[test]
fn program_name_skips_env_wrapper_and_its_assignments() {
    assert_eq!(program_name("env FOO=bar BAZ=qux cargo test"), "cargo");
}

#[test]
fn program_name_skips_sudo_flags() {
    assert_eq!(program_name("sudo -E npm i"), "npm");
}

#[test]
fn program_name_takes_basename_of_path() {
    assert_eq!(program_name("~/.local/bin/worklog day"), "worklog");
}

#[test]
fn program_name_env_assignment_alone_is_shell() {
    assert_eq!(program_name("FOO=bar"), "shell");
}

#[test]
fn program_name_rejects_invalid_characters() {
    assert_eq!(program_name("'weird$(name)' x"), "shell");
}

#[test]
fn env_assignment_secret_never_reaches_stored_event() {
    let fixture = write_fixture(
        "- cmd: AWS_SECRET_ACCESS_KEY=sk-live-superSecret123 aws configure\n  when: 1700000000\n",
    );
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events[0].title, "aws");
    let haystack = format!(
        "{}{}{}",
        events[0].title,
        events[0].details.clone().unwrap_or_default(),
        events[0].project_path.clone().unwrap_or_default()
    );
    assert!(!haystack.contains("sk-live"));
}

#[test]
fn parses_entries_and_filters_by_time_range() {
    let fixture =
        write_fixture("- cmd: ls -la\n  when: 1700000000\n- cmd: echo hi\n  when: 1800000000\n");
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    let report = collect_from_path(&conn, fixture.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1);
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "ls");
}

#[test]
fn never_stores_command_text() {
    let fixture = write_fixture("- cmd: git commit -m secret-plan\n  when: 1700000000\n");
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events[0].title, "git");
    assert_eq!(events[0].details, None);
}

#[test]
fn cd_tracking_attributes_a_later_command_to_the_repo() {
    let repo_dir = format!("{}/Desktop/Work/widget", home());
    let fixture = write_fixture(&format!(
        "- cmd: cd {repo_dir}\n  when: 1700000000\n- cmd: cargo test\n  when: 1700000100\n",
    ));
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    let cargo_ev = events.iter().find(|e| e.title == "cargo").unwrap();
    assert_eq!(cargo_ev.project_path.as_deref(), Some(repo_dir.as_str()));
}

#[test]
fn escaped_newline_cd_then_git_does_not_leak_command_into_project_path() {
    let home = home();
    let fixture = write_fixture(&format!(
        "- cmd: cd {home}/Desktop/Work/vitinn-infra\\ngit switch feature-branch\n  when: 1700000000\n"
    ));
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(
        ev.project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/vitinn-infra").as_str())
    );
    let haystack = format!(
        "{}{}{}",
        ev.title,
        ev.details.clone().unwrap_or_default(),
        ev.project_path.clone().unwrap_or_default()
    );
    assert!(!haystack.contains("git switch"));
    assert!(!haystack.contains("&&"));
}

#[test]
fn cd_and_next_command_joined_by_and_and_does_not_leak_command_into_project_path() {
    let home = home();
    let fixture = write_fixture(&format!(
        "- cmd: cd {home}/Desktop/Work/LibreChat && claude --resume f82b0366-1234\n  when: 1700000000\n"
    ));
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(
        ev.project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/LibreChat").as_str())
    );
    let haystack = format!(
        "{}{}{}",
        ev.title,
        ev.details.clone().unwrap_or_default(),
        ev.project_path.clone().unwrap_or_default()
    );
    assert!(!haystack.contains("claude"));
    assert!(!haystack.contains("--resume"));
    assert!(!haystack.contains("&&"));
}

#[test]
fn cd_quoted_target_resolves_to_repo_root() {
    let home = home();
    let fixture = write_fixture(&format!(
        "- cmd: cd \"{home}/Desktop/Work/quotedproj\"\n  when: 1700000000\n- cmd: cargo test\n  when: 1700000100\n"
    ));
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    let cargo_ev = events.iter().find(|e| e.title == "cargo").unwrap();
    assert_eq!(
        cargo_ev.project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/quotedproj").as_str())
    );
}

#[test]
fn cd_into_nested_subdirectory_collapses_to_repo_root() {
    let home = home();
    let fixture = write_fixture(&format!(
        "- cmd: cd {home}/Desktop/Work/vitinn-infra/src/module\n  when: 1700000000\n- cmd: cargo build\n  when: 1700000100\n"
    ));
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    let cargo_ev = events.iter().find(|e| e.title == "cargo").unwrap();
    assert_eq!(
        cargo_ev.project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/vitinn-infra").as_str())
    );
}

#[test]
fn re_run_inserts_no_new_rows() {
    let fixture = write_fixture("- cmd: ls\n  when: 1700000000\n");
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2023, 12, 1).unwrap();
    collect_from_path(&conn, fixture.path(), since, until).unwrap();
    let report = collect_from_path(&conn, fixture.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1, "upsert still counts as written");
    let events = repo::load_day_events(&conn, "2023-11-14").unwrap();
    assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
}
