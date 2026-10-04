use super::*;
use crate::db::open_memory;
use chrono::TimeZone;
use serde_json::json;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 18, 9, 30, 0).unwrap()
}

#[test]
fn session_start_inserts_event_and_session_row() {
    let conn = open_memory().unwrap();
    let payload = json!({
        "hook_event_name": "SessionStart",
        "session_id": "abc-123",
        "cwd": "/Users/tomas/project",
        "user_prompt": "look at PROJ-42"
    });
    handle(&conn, &payload, now()).unwrap();

    // Event written with source=claude and jira key extracted.
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source, "claude");
    assert_eq!(events[0].jira_issue.as_deref(), Some("PROJ-42"));
    assert_eq!(events[0].session_id.as_deref(), Some("abc-123"));
    assert!(events[0].title.starts_with("SessionStart —"));

    // Sessions row exists with event_count = 1.
    let count: i64 = conn
        .query_row(
            "SELECT event_count FROM sessions WHERE session_id = ?1",
            rusqlite::params!["abc-123"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn stop_event_closes_session() {
    let conn = open_memory().unwrap();
    handle(
        &conn,
        &json!({
            "hook_event_name": "SessionStart",
            "session_id": "x",
            "cwd": "/p"
        }),
        now(),
    )
    .unwrap();
    handle(
        &conn,
        &json!({
            "hook_event_name": "Stop",
            "session_id": "x"
        }),
        now() + chrono::Duration::minutes(15),
    )
    .unwrap();

    let (ended_at, end_source): (String, String) = conn
        .query_row(
            "SELECT ended_at, end_source FROM sessions WHERE session_id = ?1",
            rusqlite::params!["x"],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(ended_at.starts_with("2026-04-18T09:45"));
    assert_eq!(end_source, "stop");
}

#[test]
fn dedupe_is_keyed_on_session_event_timestamp() {
    let conn = open_memory().unwrap();
    let p = json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s1" });
    handle(&conn, &p, now()).unwrap();
    handle(&conn, &p, now()).unwrap(); // same timestamp → same source_id → upsert, not duplicate
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn handle_does_not_panic_on_missing_fields() {
    let conn = open_memory().unwrap();
    // Only a session id — everything else is missing.
    let p = json!({ "session_id": "bare" });
    handle(&conn, &p, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "unknown");
}

// ───────────────────── prompt capture (v0.4) ─────────────────────

#[test]
fn captures_full_prompt_up_to_cap_in_details() {
    // B1: a 200-char prompt is well below the 4KiB cap → must land
    // verbatim in event.details. The estimator reads `details`, so
    // anything the user typed in their Claude prompt is now visible
    // to the summariser (previously dropped after the 80-char title).
    let conn = open_memory().unwrap();
    let prompt = "a".repeat(200);
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s1",
        "user_prompt": prompt,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].details.as_deref(),
        Some("a".repeat(200).as_str()),
        "full 200-char prompt should round-trip into event.details"
    );
}

#[test]
fn truncates_prompts_over_cap_with_explicit_marker() {
    // B2: anything over 4096 chars gets sliced to the cap + a
    // `…<truncated N chars>` suffix so readers (and Claude, when it
    // re-reads this in the estimator) know the payload was cut.
    let conn = open_memory().unwrap();
    let prompt = "x".repeat(10_000);
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s2",
        "user_prompt": prompt,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let details = events[0]
        .details
        .as_deref()
        .expect("details should be populated from the prompt");
    assert!(
        details.starts_with(&"x".repeat(4096)),
        "first 4096 chars must be preserved verbatim"
    );
    assert!(
        details.contains("truncated"),
        "truncation marker must be present so downstream readers know"
    );
    assert!(
        details.chars().count() < 10_000,
        "payload must actually be shorter than the original"
    );
}

#[test]
fn no_prompt_event_stores_no_details() {
    // A SessionStart/Stop fires without a prompt. We used to store the
    // transcript path in `details`; that path points at the full
    // session and is not work-intent signal, so `details` is now left
    // empty for these events.
    let conn = open_memory().unwrap();
    let payload = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s3",
        "transcript_path": "/tmp/transcript-abc.jsonl",
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events[0].details, None,
        "no prompt → no details (transcript path is not stored)"
    );
}

#[test]
fn hook_payload_stored_and_scrubbed() {
    // D-03/A6: the full hook payload is now stored (scrubbed) as
    // `RawRecord::Hook` so the Details view can show every tool call a
    // session made; the token must still never reach the DB.
    let conn = open_memory().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "session_id": "s5",
        "tool_name": "Bash",
        "tool_input": { "command": format!("curl -H 'Authorization: Bearer {token}' https://x") },
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let raw_json = events[0]
        .raw_json
        .as_deref()
        .expect("raw_json must be populated from the hook payload");
    assert!(
        !raw_json.contains(&token),
        "token must never reach raw_json"
    );
    let record: crate::clues_contract::RawRecord = serde_json::from_str(raw_json).unwrap();
    match record {
        crate::clues_contract::RawRecord::Hook { event, payload } => {
            assert_eq!(event, "PostToolUse");
            let command = payload["tool_input"]["command"].as_str().unwrap();
            assert!(command.contains("[secret]"));
            assert!(!command.contains(&token));
        }
        other => panic!("expected RawRecord::Hook, got {other:?}"),
    }
}

#[test]
fn hook_collapses_worktree_cwd_to_project_root() {
    // A11/A12: collapse `/.claude/worktrees/<name>` the same way the
    // transcripts collector does, or the same repo splits into two
    // project folders depending on which source saw it.
    let conn = open_memory().unwrap();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let cwd = format!("{home}/Desktop/Work/repo/.claude/worktrees/feat-x");
    let payload = json!({
        "hook_event_name": "SessionStart",
        "session_id": "wt1",
        "cwd": cwd,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events[0].project_path.as_deref(),
        Some(format!("{home}/Desktop/Work/repo").as_str())
    );
}

#[test]
fn task_notification_prompt_is_redacted_before_storage() {
    // Claude Code delivers background-agent completions as a prompt;
    // the <result> holds code. `redact_code` runs at capture time so
    // only the one-line summary is ever written to the DB.
    let conn = open_memory().unwrap();
    let notif = "<task-notification><summary>Agent review done</summary>\
         <result>def f(): return SECRET_KEY</result></task-notification>";
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s6",
        "user_prompt": notif,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let details = events[0].details.as_deref().unwrap_or("");
    assert!(details.contains("Agent review done"));
    assert!(
        !details.contains("SECRET_KEY"),
        "result code must not be stored"
    );
}

#[test]
fn cap_prompt_is_char_safe_for_multi_byte_unicode() {
    // If the cap clipped on byte boundaries we'd corrupt emoji /
    // non-ASCII at the boundary. Feed a payload that crosses the
    // cap with multi-byte chars and assert the stored string is
    // still valid UTF-8 and of the expected char length.
    let conn = open_memory().unwrap();
    // "日" is 3 bytes, 1 char. 5000 of them > 4096 chars but < 4096
    // bytes if we were byte-counting (we're not).
    let prompt = "日".repeat(5000);
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s4",
        "user_prompt": prompt,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let details = events[0].details.as_deref().unwrap();
    // First 4096 chars must be "日"*4096. Count chars, not bytes.
    let prefix_chars = details.chars().take(4096).count();
    assert_eq!(
        prefix_chars, 4096,
        "cap must slice on char boundaries, not byte boundaries"
    );
    assert!(
        details.chars().take(4096).all(|c| c == '日'),
        "the first 4096 chars should all be the original char"
    );
}

#[test]
fn prompt_secret_never_reaches_title_details_or_raw_json() {
    // FR-11/D-03: every stored value is scrubbed, and the prompt is
    // scrubbed BEFORE the 80-char title cut so the cut can never split a
    // token into a shape the scrubber misses.
    let conn = open_memory().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    let prompt = format!("deploy with {token} please");
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s5",
        "user_prompt": prompt,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ev = &events[0];
    assert!(!ev.title.contains(&token), "title leaked: {}", ev.title);
    assert!(
        !ev.details.as_deref().unwrap_or("").contains(&token),
        "details leaked: {:?}",
        ev.details
    );
    assert!(
        !ev.raw_json.as_deref().unwrap_or("").contains(&token),
        "raw_json leaked: {:?}",
        ev.raw_json
    );
}

#[test]
fn prompt_scrubbed_before_title_cut_never_leaks_a_partial_token() {
    // The 80-char title cut must never slice a live token in half —
    // scrubbing has to run on the whole prompt BEFORE `title_for` takes
    // its snippet, or a partial prefix (not a full-token regex match)
    // slips through untouched.
    let conn = open_memory().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    // A space (not a word char) separates the padding from the token so
    // the whole-token regex's `\b` boundary still matches it — only the
    // 80-char cut point sits inside the token.
    let padding = format!("{} ", "x".repeat(74));
    let prompt = format!("{padding}{token} please");
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s6",
        "user_prompt": prompt,
    });
    handle(&conn, &payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let title = &events[0].title;
    assert!(
        !title.contains("ghp_"),
        "even a partial token prefix must never leak into the title: {title}"
    );
}

fn repo_on_branch(branch: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", branch])
        .arg(tmp.path())
        .status()
        .unwrap();
    assert!(status.success());
    tmp
}

fn jira_issue_for(payload: &Value) -> Option<String> {
    let conn = open_memory().unwrap();
    handle(&conn, payload, now()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    events[0].jira_issue.clone()
}

#[test]
fn branch_key_is_used_when_prompt_and_path_have_none() {
    let tmp = repo_on_branch("PROJ-7-x");
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "b1",
        "cwd": tmp.path().to_str().unwrap(),
        "prompt": "fix the thing"
    });
    assert_eq!(jira_issue_for(&payload).as_deref(), Some("PROJ-7"));
}

#[test]
fn prompt_key_beats_branch_key() {
    let tmp = repo_on_branch("PROJ-7-x");
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "b2",
        "cwd": tmp.path().to_str().unwrap(),
        "prompt": "look at PROJ-42"
    });
    assert_eq!(jira_issue_for(&payload).as_deref(), Some("PROJ-42"));
}

#[test]
fn path_key_beats_branch_key() {
    let tmp = repo_on_branch("PROJ-7-x");
    let nested = tmp.path().join("PROJ-9");
    std::fs::create_dir(&nested).unwrap();
    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "b3",
        "cwd": nested.to_str().unwrap(),
        "prompt": "fix the thing"
    });
    assert_eq!(jira_issue_for(&payload).as_deref(), Some("PROJ-9"));
}
