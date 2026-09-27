use super::*;
use crate::clues_contract::{RawRecord, SECRET_PLACEHOLDER};
use crate::db::open_memory;

fn write_transcript(dir: &Path, project: &str, session: &str, lines: &str) {
    let project_dir = dir.join(project);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join(format!("{session}.jsonl")), lines).unwrap();
}

#[test]
fn claude_working_in_an_interactive_session_counts_once_per_minute() {
    // Claude works for 10 minutes between two prompts (replies every 30 s);
    // a headless `claude -p` run in the same folder must add nothing.
    let tmp = tempfile::tempdir().unwrap();
    let mut lines = Vec::new();
    for i in 0..20 {
        let (m, s) = (i / 2, (i % 2) * 30);
        lines.push(format!(
            r#"{{"type":"assistant","timestamp":"2026-04-18T09:{m:02}:{s:02}Z","sessionId":"s1","uuid":"a{i}","cwd":"/home/x/Desktop/Work/widget","entrypoint":"cli","message":{{"role":"assistant","content":[{{"type":"text","text":"working"}}]}}}}"#
        ));
        lines.push(format!(
            r#"{{"type":"assistant","timestamp":"2026-04-18T09:{m:02}:{s:02}Z","sessionId":"s2","uuid":"h{i}","cwd":"/home/x/Desktop/Work/widget","entrypoint":"sdk-cli","message":{{"role":"assistant","content":[{{"type":"text","text":"estimate"}}]}}}}"#
        ));
    }
    write_transcript(tmp.path(), "proj", "s1", &format!("{}\n", lines.join("\n")));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(
        report.events_written, 10,
        "one per minute, headless run ignored"
    );
    let leaked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE title <> 'claude working' OR details IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0, "no reply text is ever stored");
}

#[test]
fn reads_a_transcript_touched_after_the_day_it_covers() {
    // A long session keeps writing past midnight: the file's mtime is after
    // `until`, but its earlier lines still belong to the day.
    let tmp = tempfile::tempdir().unwrap();
    let line = user_line(
        "2026-04-18T15:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        r#""keep going""#,
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1);
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
fn assistant_line_counts_as_claude_working_not_a_prompt() {
    // Behaviour change (owner, 2026-09-24): Claude working in an interactive
    // session is time on that project; it is recorded as "claude working",
    // never as a prompt, and never with its text.
    let tmp = tempfile::tempdir().unwrap();
    let line = r#"{"type":"assistant","timestamp":"2026-04-18T09:00:00Z","sessionId":"s1","uuid":"u1","cwd":"/home/x/Desktop/Work/widget","message":{"role":"assistant","content":[{"type":"text","text":"sure"}]}}"#;
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1);
    let (title, details): (String, Option<String>) = conn
        .query_row("SELECT title, details FROM events", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(title, "claude working");
    assert_eq!(details, None);
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

#[test]
fn claude_working_records_branch_tools_and_edited_files_never_commands() {
    // What the owner needs to recognise a stretch of Claude working: the
    // branch, which tools ran and which files changed. Never the command.
    let tmp = tempfile::tempdir().unwrap();
    let a = r#"{"type":"assistant","timestamp":"2026-04-18T09:00:05Z","sessionId":"s1","uuid":"a1","cwd":"/home/x/Desktop/Work/widget","gitBranch":"fix-login","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"SECRET=hunter2 make deploy"}}]}}"#;
    let b = r#"{"type":"assistant","timestamp":"2026-04-18T09:00:40Z","sessionId":"s1","uuid":"a2","cwd":"/home/x/Desktop/Work/widget","gitBranch":"fix-login","message":{"role":"assistant","content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/home/x/Desktop/Work/widget/src/login.rs","old_string":"x","new_string":"y"}},{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]}}"#;
    write_transcript(tmp.path(), "proj", "s1", &format!("{a}\n{b}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let details: Option<String> = conn
        .query_row(
            "SELECT details FROM events WHERE source = 'claude_work'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let d = details.expect("a working minute says what happened");
    assert!(d.contains("fix-login"), "{d}");
    assert!(d.contains("Bash ×2"), "{d}");
    assert!(d.contains("Edit"), "{d}");
    assert!(d.contains("src/login.rs"), "{d}");
    let all: String = conn
        .query_row(
            "SELECT group_concat(coalesce(details,'') || title) FROM events",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for secret in ["hunter2", "make deploy", "cargo test"] {
        assert!(!all.contains(secret), "command text leaked: {secret}");
    }
}

#[test]
fn a_session_copied_into_a_second_file_counts_once() {
    // Resuming a session writes its history into a new file with the same
    // line uuids; that is one Claude working, not two.
    let tmp = tempfile::tempdir().unwrap();
    let line = |session: &str| {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-04-18T09:00:00Z","sessionId":"{session}","uuid":"same","cwd":"/home/x/Desktop/Work/widget","message":{{"role":"assistant","content":[{{"type":"text","text":"x"}}]}}}}"#
        )
    };
    write_transcript(tmp.path(), "p1", "s1", &format!("{}\n", line("s1")));
    write_transcript(tmp.path(), "p2", "s2", &format!("{}\n", line("s2")));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE source = 'claude_work'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn claude_prompt_raw_json_stores_scrubbed_text() {
    // FR-14/D-04: prompt text is now stored, secret-scrubbed, in raw_json.
    let tmp = tempfile::tempdir().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    let line = user_line(
        "2026-04-18T09:00:00Z",
        "s1",
        "u1",
        "/home/x/Desktop/Work/widget",
        &format!("\"fix the bug using {token}\""),
    );
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let raw = events[0]
        .raw_json
        .as_deref()
        .expect("prompt raw_json stored");
    let record: RawRecord = serde_json::from_str(raw).unwrap();
    match record {
        RawRecord::ClaudePrompt { session_id, text } => {
            assert_eq!(session_id, "s1");
            assert_eq!(text, format!("fix the bug using {SECRET_PLACEHOLDER}"));
            assert!(!text.contains(&token));
        }
        other => panic!("expected ClaudePrompt, got {other:?}"),
    }
    assert_eq!(events[0].title, "prompt");
    assert_eq!(events[0].details, None);
}

#[test]
fn claude_tool_use_and_its_result_store_one_claude_tool_row() {
    // FR-14/FR-15: one NEW row per tool_use, input + paired output stored.
    let tmp = tempfile::tempdir().unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let cwd = format!("{home}/Desktop/Work/widget");
    let file_path = format!("{cwd}/src/main.rs");
    let tool_use = serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-04-18T09:00:05Z",
        "sessionId": "s1",
        "uuid": "a1",
        "cwd": cwd,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": file_path}}
        ]}
    })
    .to_string();
    let tool_result = serde_json::json!({
        "type": "user",
        "timestamp": "2026-04-18T09:00:06Z",
        "sessionId": "s1",
        "uuid": "r1",
        "cwd": cwd,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "content": "file contents here"}
        ]}
    })
    .to_string();
    write_transcript(
        tmp.path(),
        "proj",
        "s1",
        &format!("{tool_use}\n{tool_result}\n"),
    );

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let tool_rows: Vec<_> = events
        .iter()
        .filter(|e| e.source == "claude_tool")
        .collect();
    assert_eq!(
        tool_rows.len(),
        1,
        "one row per tool_use, not per tool_result"
    );
    let ev = tool_rows[0];
    assert_eq!(ev.source_id, "s1:toolu_1");
    assert_eq!(ev.title, "Read");
    assert_eq!(ev.details, None);
    assert_eq!(ev.session_id.as_deref(), Some("s1"));
    assert_eq!(ev.project_path.as_deref(), Some(cwd.as_str()));
    let record: RawRecord = serde_json::from_str(ev.raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::ClaudeTool {
            session_id,
            tool,
            input,
            output,
            output_cut_bytes,
            files,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(tool, "Read");
            assert_eq!(input, serde_json::json!({"file_path": file_path}));
            assert_eq!(output.as_deref(), Some("file contents here"));
            assert_eq!(output_cut_bytes, 0);
            assert_eq!(files, vec![file_path.clone()]);
        }
        other => panic!("expected ClaudeTool, got {other:?}"),
    }
}

#[test]
fn claude_tool_output_over_cap_keeps_first_2kb_bytes() {
    // FR-15/D-04: a 40 KB output is capped to the first 2 KB, byte-exact.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    let big = "x".repeat(40 * 1024);
    let tool_use = serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-04-18T09:00:05Z",
        "sessionId": "s1",
        "uuid": "a1",
        "cwd": cwd,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "cat big.log"}}
        ]}
    })
    .to_string();
    let tool_result = serde_json::json!({
        "type": "user",
        "timestamp": "2026-04-18T09:00:06Z",
        "sessionId": "s1",
        "uuid": "r1",
        "cwd": cwd,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "content": big}
        ]}
    })
    .to_string();
    write_transcript(
        tmp.path(),
        "proj",
        "s1",
        &format!("{tool_use}\n{tool_result}\n"),
    );

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ev = events.iter().find(|e| e.source == "claude_tool").unwrap();
    let record: RawRecord = serde_json::from_str(ev.raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::ClaudeTool {
            output,
            output_cut_bytes,
            ..
        } => {
            let out = output.expect("output kept");
            assert_eq!(out.len(), 2048);
            assert_eq!(out, "x".repeat(2048));
            assert_eq!(output_cut_bytes, 40 * 1024 - 2048);
        }
        other => panic!("expected ClaudeTool, got {other:?}"),
    }
}

#[test]
fn claude_tool_input_secret_is_scrubbed_to_placeholder() {
    // D-03: a secret in a tool's input never reaches worklog.db.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    let token = format!("ghp_{}", "a".repeat(36));
    let tool_use = serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-04-18T09:00:05Z",
        "sessionId": "s1",
        "uuid": "a1",
        "cwd": cwd,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": token}}
        ]}
    })
    .to_string();
    write_transcript(tmp.path(), "proj", "s1", &format!("{tool_use}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ev = events.iter().find(|e| e.source == "claude_tool").unwrap();
    let record: RawRecord = serde_json::from_str(ev.raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::ClaudeTool { input, .. } => {
            assert_eq!(
                input.get("command").and_then(|v| v.as_str()),
                Some(SECRET_PLACEHOLDER)
            );
        }
        other => panic!("expected ClaudeTool, got {other:?}"),
    }
}

#[test]
fn claude_work_rows_unchanged_by_claude_tool_capture() {
    // The per-minute "claude working" summary must not change shape just
    // because tool_use items now also produce their own claude_tool rows.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    let a = serde_json::json!({
        "type": "assistant", "timestamp": "2026-04-18T09:00:05Z", "sessionId": "s1", "uuid": "a1",
        "cwd": cwd, "gitBranch": "fix-login",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "SECRET=hunter2 make deploy"}}
        ]}
    }).to_string();
    let b = serde_json::json!({
        "type": "assistant", "timestamp": "2026-04-18T09:00:40Z", "sessionId": "s1", "uuid": "a2",
        "cwd": cwd, "gitBranch": "fix-login",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t2", "name": "Edit", "input": {"file_path": format!("{cwd}/src/login.rs"), "old_string": "x", "new_string": "y"}},
            {"type": "tool_use", "id": "t3", "name": "Bash", "input": {"command": "cargo test"}}
        ]}
    }).to_string();
    write_transcript(tmp.path(), "proj", "s1", &format!("{a}\n{b}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();

    let work_rows: Vec<_> = events
        .iter()
        .filter(|e| e.source == "claude_work")
        .collect();
    assert_eq!(work_rows.len(), 1, "still one working minute");
    assert_eq!(work_rows[0].title, "claude working");
    let d = work_rows[0].details.clone().expect("summary present");
    assert!(d.contains("fix-login"), "{d}");
    assert!(d.contains("Bash ×2"), "{d}");
    assert!(d.contains("Edit"), "{d}");
    assert!(d.contains("src/login.rs"), "{d}");

    let tool_rows: Vec<_> = events
        .iter()
        .filter(|e| e.source == "claude_tool")
        .collect();
    assert_eq!(tool_rows.len(), 3, "one claude_tool row per tool_use id");
}

#[test]
fn claude_tool_sidechain_tool_calls_produce_no_row() {
    // D-05/FR-17: sidechain (helper) activity is out of scope here (T011)
    // and must never surface as a claude_tool row from this collector.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    let line = serde_json::json!({
        "type": "assistant", "timestamp": "2026-04-18T09:00:05Z", "sessionId": "s1", "uuid": "a1",
        "cwd": cwd, "isSidechain": true,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "echo hi"}}
        ]}
    })
    .to_string();
    write_transcript(tmp.path(), "proj", "s1", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events.iter().filter(|e| e.source == "claude_tool").count(),
        0
    );
}

#[test]
fn each_files_commit_lands_before_the_next_files_read_starts() {
    // A transaction spanning the whole multi-file scan holds SQLite's write
    // lock for as long as the scan takes; each file must instead commit
    // (and become visible to a second connection) before the next file's
    // turn — proven here by a hook fired right after each per-file commit.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    write_transcript(
        tmp.path(),
        "proj",
        "s1",
        &format!(
            "{}\n",
            user_line("2026-04-18T09:00:00Z", "s1", "u1", cwd, "\"one\"")
        ),
    );
    write_transcript(
        tmp.path(),
        "proj",
        "s2",
        &format!(
            "{}\n",
            user_line("2026-04-18T09:05:00Z", "s2", "u2", cwd, "\"two\"")
        ),
    );

    let db_path = tmp.path().join("worklog.db");
    let conn = crate::db::open(&db_path).unwrap();
    let second = rusqlite::Connection::open(&db_path).unwrap();
    let seen_counts = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorder = seen_counts.clone();
    claude_transcript_cache::for_test_set_after_file_commit(move || {
        let n: i64 = second
            .query_row(
                "SELECT COUNT(*) FROM events WHERE source = 'claude_turn'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        recorder.borrow_mut().push(n);
    });

    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let counts = seen_counts.borrow();
    assert_eq!(*counts, vec![1, 2], "each file's commit must land, and be visible to another connection, before the next file's commit — not only once the whole scan finishes");
}
