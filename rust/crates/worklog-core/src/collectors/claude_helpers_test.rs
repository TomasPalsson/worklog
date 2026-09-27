use super::*;
use crate::clues_contract::{
    RawRecord, SECRET_PLACEHOLDER, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_MESSAGE,
};
use crate::collectors::claude_transcripts::{collect_from_dir, collect_from_dirs, is_owner_typed};
use crate::db::open_memory;
use chrono::NaiveDate;

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn wide_window() -> (NaiveDate, NaiveDate) {
    (
        NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2100, 1, 1).unwrap(),
    )
}

fn agent_line(agent_id: &str, session_id: &str, uuid: &str, cwd: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-04-18T09:00:05Z",
        "sessionId": session_id,
        "uuid": uuid,
        "agentId": agent_id,
        "cwd": cwd,
        "message": {"role": "assistant", "content": [{"type": "text", "text": "working"}]}
    })
    .to_string()
}

fn helper_events(conn: &rusqlite::Connection) -> Vec<crate::models::Event> {
    repo::load_day_events(conn, "2026-04-18")
        .unwrap()
        .into_iter()
        .filter(|e| e.source == SOURCE_CLAUDE_HELPER)
        .collect()
}

#[test]
fn claude_helpers_subagent_minutes_link_to_parent_session() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(tmp.path(), "proj/s1.jsonl", "");
    write_file(
        tmp.path(),
        "proj/s1/subagents/agent-a1.jsonl",
        &format!(
            "{}\n",
            agent_line("a1", "s1", "h1", "/home/x/Desktop/Work/widget")
        ),
    );
    write_file(
        tmp.path(),
        "proj/s1/subagents/agent-a1.meta.json",
        r#"{"agentType":"code-reviewer","description":"review the diff","toolUseId":"tu1"}"#,
    );

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let rows = helper_events(&conn);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id.as_deref(), Some("s1"));
    assert!(rows[0].source_id.starts_with("a1:m"));
    assert_eq!(rows[0].title, "code-reviewer: review the diff");
    let record: RawRecord = serde_json::from_str(rows[0].raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::Helper {
            parent_session_id,
            helper_kind,
            ..
        } => {
            assert_eq!(parent_session_id, "s1");
            assert_eq!(helper_kind, HelperKind::Subagent);
        }
        other => panic!("expected Helper, got {other:?}"),
    }
}

#[test]
fn claude_helpers_two_teammates_one_cwd_never_mix_parents() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/agents";
    write_file(tmp.path(), "proj/sA.jsonl", "");
    write_file(
        tmp.path(),
        "proj/sA/subagents/agent-teamA.jsonl",
        &format!("{}\n", agent_line("teamA", "sA", "hA", cwd)),
    );
    write_file(
        tmp.path(),
        "proj/sA/subagents/agent-teamA.meta.json",
        r#"{"agentType":"teammate","description":"alpha","taskKind":"in_process_teammate"}"#,
    );
    write_file(tmp.path(), "proj/sB.jsonl", "");
    write_file(
        tmp.path(),
        "proj/sB/subagents/agent-teamB.jsonl",
        &format!("{}\n", agent_line("teamB", "sB", "hB", cwd)),
    );
    write_file(
        tmp.path(),
        "proj/sB/subagents/agent-teamB.meta.json",
        r#"{"agentType":"teammate","description":"bravo","taskKind":"in_process_teammate"}"#,
    );

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let mut rows = helper_events(&conn);
    assert_eq!(rows.len(), 2, "one row per teammate");
    rows.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    assert!(rows[0].source_id.starts_with("teamA:m"));
    assert_eq!(rows[0].session_id.as_deref(), Some("sA"));
    assert!(rows[1].source_id.starts_with("teamB:m"));
    assert_eq!(rows[1].session_id.as_deref(), Some("sB"));
    for row in &rows {
        let record: RawRecord = serde_json::from_str(row.raw_json.as_deref().unwrap()).unwrap();
        match record {
            RawRecord::Helper {
                parent_session_id,
                helper_kind,
                ..
            } => {
                assert_eq!(helper_kind, HelperKind::Teammate);
                assert_eq!(Some(parent_session_id.as_str()), row.session_id.as_deref());
            }
            other => panic!("expected Helper, got {other:?}"),
        }
    }
}

#[test]
fn claude_helpers_workflow_task_subagent_file_picked_up() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    write_file(tmp.path(), "proj/sC.jsonl", "");
    write_file(
        tmp.path(),
        "proj/sC/subagents/workflows/wf_1/agent-w1.jsonl",
        &format!("{}\n", agent_line("w1", "sC", "hW", cwd)),
    );
    write_file(
        tmp.path(),
        "proj/sC/subagents/workflows/wf_1/agent-w1.meta.json",
        r#"{"agentType":"workflow-subagent"}"#,
    );
    // journal.jsonl never becomes a helper row, even though it would if the
    // "agent-*.jsonl" filter were dropped.
    write_file(
        tmp.path(),
        "proj/sC/subagents/workflows/wf_1/journal.jsonl",
        &format!("{}\n", agent_line("journal-agent", "sC", "hJ", cwd)),
    );

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let rows = helper_events(&conn);
    assert_eq!(rows.len(), 1, "only the workflow task's own file counts");
    assert_eq!(rows[0].session_id.as_deref(), Some("sC"));
    assert!(rows[0].source_id.starts_with("w1:m"));
}

#[test]
fn claude_helpers_background_job_session_produces_no_claude_work() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects");
    let jobs_dir = tmp.path().join("jobs");
    write_file(&jobs_dir, "j1/state.json", r#"{"sessionId":"sJ"}"#);
    write_file(
        &projects_dir,
        "proj/sJ.jsonl",
        &format!(
            "{}\n",
            serde_json::json!({
                "type": "assistant",
                "timestamp": "2026-04-18T09:00:05Z",
                "sessionId": "sJ",
                "uuid": "u1",
                "cwd": "/home/x/Desktop/Work/widget",
                "message": {"role": "assistant", "content": [{"type": "text", "text": "x"}]}
            })
        ),
    );

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dirs(&conn, &projects_dir, &jobs_dir, since, until).unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let helper: Vec<_> = events
        .iter()
        .filter(|e| e.source == SOURCE_CLAUDE_HELPER)
        .collect();
    assert_eq!(helper.len(), 1);
    assert!(helper[0].source_id.starts_with("sJ:m"));
    assert_eq!(helper[0].session_id.as_deref(), Some("sJ"));
    let record: RawRecord = serde_json::from_str(helper[0].raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::Helper {
            parent_session_id,
            helper_kind,
            ..
        } => {
            assert_eq!(parent_session_id, "sJ");
            assert_eq!(helper_kind, HelperKind::BackgroundJob);
        }
        other => panic!("expected Helper, got {other:?}"),
    }
    assert_eq!(
        events.iter().filter(|e| e.source == "claude_work").count(),
        0
    );
}

#[test]
fn claude_helpers_teammate_message_and_peer_attachment_are_never_claude_turn() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = "/home/x/Desktop/Work/widget";
    let teammate_line = serde_json::json!({
        "type": "user",
        "timestamp": "2026-04-18T09:00:00Z",
        "sessionId": "sM",
        "uuid": "u1",
        "cwd": cwd,
        "message": {"role": "user", "content": "<teammate-message teammate_id=\"peerAgent\">status update here</teammate-message>"}
    }).to_string();
    let attachment_line = serde_json::json!({
        "type": "attachment",
        "timestamp": "2026-04-18T09:00:10Z",
        "sessionId": "sM",
        "uuid": "u2",
        "cwd": cwd,
        "attachment": {
            "type": "queued_command",
            "origin": {"kind": "peer", "from": "peerAgent2"},
            "prompt": "<agent-message from=\"peerAgent2\">do task</agent-message>"
        }
    })
    .to_string();
    write_file(
        tmp.path(),
        "proj/sM.jsonl",
        &format!("{teammate_line}\n{attachment_line}\n"),
    );

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events.iter().filter(|e| e.source == "claude_turn").count(),
        0
    );
    let mut messages: Vec<_> = events
        .iter()
        .filter(|e| e.source == SOURCE_CLAUDE_MESSAGE)
        .collect();
    assert_eq!(messages.len(), 2);
    messages.sort_by(|a, b| a.source_id.cmp(&b.source_id));

    assert_eq!(messages[0].title, "message from peerAgent");
    let record: RawRecord = serde_json::from_str(messages[0].raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::SessionMessage { from, text } => {
            assert_eq!(from, "peerAgent");
            assert_eq!(text, "status update here");
        }
        other => panic!("expected SessionMessage, got {other:?}"),
    }

    assert_eq!(messages[1].title, "message from peerAgent2");
    let record: RawRecord = serde_json::from_str(messages[1].raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::SessionMessage { from, text } => {
            assert_eq!(from, "peerAgent2");
            assert_eq!(
                text,
                "<agent-message from=\"peerAgent2\">do task</agent-message>"
            );
        }
        other => panic!("expected SessionMessage, got {other:?}"),
    }
}

#[test]
fn claude_helpers_secret_in_a_message_is_scrubbed() {
    let tmp = tempfile::tempdir().unwrap();
    let token = format!("ghp_{}", "a".repeat(36));
    let line = serde_json::json!({
        "type": "user",
        "timestamp": "2026-04-18T09:00:00Z",
        "sessionId": "sS",
        "uuid": "u1",
        "cwd": "/home/x/Desktop/Work/widget",
        "message": {"role": "user", "content": format!("<teammate-message teammate_id=\"peer\">token {token}</teammate-message>")}
    }).to_string();
    write_file(tmp.path(), "proj/sS.jsonl", &format!("{line}\n"));

    let conn = open_memory().unwrap();
    let (since, until) = wide_window();
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ev = events
        .iter()
        .find(|e| e.source == SOURCE_CLAUDE_MESSAGE)
        .unwrap();
    let record: RawRecord = serde_json::from_str(ev.raw_json.as_deref().unwrap()).unwrap();
    match record {
        RawRecord::SessionMessage { text, .. } => {
            assert_eq!(text, format!("token {SECRET_PLACEHOLDER}"));
            assert!(!text.contains(&token));
        }
        other => panic!("expected SessionMessage, got {other:?}"),
    }
}

#[test]
fn claude_helpers_teammate_message_line_is_never_owner_typed() {
    let value = serde_json::json!({
        "type": "user",
        "message": {
            "content": "<teammate-message teammate_id=\"x\">hi</teammate-message>"
        }
    });
    assert!(!is_owner_typed(&value));
}
