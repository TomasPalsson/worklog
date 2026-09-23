use super::*;
use crate::db::open_memory;

fn write_transcript(dir: &Path, project: &str, session: &str, lines: &str) {
    let project_dir = dir.join(project);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join(format!("{session}.jsonl")), lines).unwrap();
}

fn user_line(ts: &str, session: &str, uuid: &str, cwd: &str, content: &str) -> String {
    format!(
        r#"{{"type":"user","timestamp":"{ts}","sessionId":"{session}","uuid":"{uuid}","cwd":"{cwd}","message":{{"role":"user","content":{content}}}}}"#
    )
}

#[test]
fn counts_a_real_string_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let cwd = format!("{home}/Desktop/Work/widget");
    let line = user_line("2026-04-18T09:00:00Z", "s1", "u1", &cwd, "\"fix the bug\"");
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1);
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source, "claude_turn");
    assert_eq!(events[0].title, "prompt");
    assert_eq!(events[0].details, None);
    assert_eq!(events[0].source_id, "s1:u1");
    assert_eq!(events[0].session_id.as_deref(), Some("s1"));
    assert_eq!(events[0].project_path.as_deref(), Some(cwd.as_str()));
    assert_eq!(events[0].raw_json, None);
    assert_eq!(events[0].repo, None);
    assert_eq!(events[0].jira_issue, None);
    assert_eq!(events[0].tempo_worklog_id, None);
}

#[test]
fn counts_a_prompt_with_text_content_items() {
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2026-04-18T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        r#"[{"type":"text","text":"hello"}]"#,
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1);
}

#[test]
fn skips_tool_result_only_user_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2026-04-18T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        r#"[{"type":"tool_result","content":"some output"}]"#,
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 0);
}

#[test]
fn counts_only_lines_the_owner_typed() {
    let tmp = tempfile::tempdir().unwrap();
    let base = |extra: &str, uuid: &str, content: &str| {
        format!(
            r#"{{"type":"user","timestamp":"2026-04-18T09:00:00Z","sessionId":"s1","uuid":"{uuid}","cwd":"/home/x/Desktop/Work/widget"{extra},"message":{{"role":"user","content":{content}}}}}"#
        )
    };
    let lines = [
        base(
            r#","origin":{"kind":"human"},"promptSource":"typed""#,
            "typed",
            r#""fix the bug""#,
        ),
        base(
            r#","origin":{"kind":"task-notification"}"#,
            "notif",
            r#""<task-notification>done</task-notification>""#,
        ),
        base(
            r#","entrypoint":"sdk-cli","origin":{"kind":"human"}"#,
            "headless",
            r#""estimate this block""#,
        ),
        base(r#","isMeta":true"#, "meta", r#""Caveat: local command""#),
        base("", "old-tag", r#""<command-name>/clear</command-name>""#),
        base("", "old-plain", r#""plain prompt from an older version""#),
    ];
    write_transcript(tmp.path(), "proj", "s1", &format!("{}\n", lines.join("\n")));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(
        report.events_written, 2,
        "only the typed prompt and the old plain prompt count"
    );
    let ids: Vec<String> = conn
        .prepare("SELECT source_id FROM events ORDER BY source_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        ids,
        vec!["s1:old-plain".to_string(), "s1:typed".to_string()]
    );
}

#[test]
fn skips_assistant_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let line = r#"{"type":"assistant","timestamp":"2026-04-18T09:00:00Z","sessionId":"s1","uuid":"u1","cwd":"/home/x/Desktop/Work/widget","message":{"role":"assistant","content":[{"type":"text","text":"sure"}]}}"#;
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 0);
}

#[test]
fn skips_lines_outside_the_time_window() {
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2020-01-01T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        "\"old prompt\"",
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 0);
}

#[test]
fn worktree_cwd_collapses_to_repo_root() {
    let tmp = tempfile::tempdir().unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let cwd = format!("{home}/Desktop/Work/vitinn-infra/.claude/worktrees/sandbox-runner");
    let line = user_line("2026-04-18T09:00:00Z", "s1", "u1", &cwd, "\"do the thing\"");
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events[0].project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/vitinn-infra").as_str())
    );
}

#[test]
fn never_stores_prompt_text() {
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2026-04-18T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        "\"the secret plan is XYZZY\"",
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ev = &events[0];
    assert_eq!(ev.title, "prompt");
    assert_eq!(ev.details, None);
    assert_eq!(ev.raw_json, None);
    let haystack = format!(
        "{}{}{}{}",
        ev.title,
        ev.details.clone().unwrap_or_default(),
        ev.project_path.clone().unwrap_or_default(),
        ev.session_id.clone().unwrap_or_default()
    );
    assert!(!haystack.contains("XYZZY"));
    assert!(!haystack.contains("secret plan"));
}

#[test]
fn re_run_inserts_no_new_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2026-04-18T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        "\"fix the bug\"",
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1, "upsert still counts as written");
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1, "dedupe on (source, source_id)");
}
