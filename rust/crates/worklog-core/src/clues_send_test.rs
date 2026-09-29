//! Tests for T018 — `clues_send::build_block_input` / `build_line_input`
//! (spec 006, D-02). Every test proves the send/never-send split, not
//! just that a `DescriptionInput` gets built.

use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::clues_contract::{BillingLineKey, HelperKind, RawRecord, SOURCE_CLAUDE_HELPER};
use crate::db;
use crate::models::Event;
use crate::routing_contract::{SOURCE_FIREFOX, SOURCE_SLACK};
use rusqlite::params;

fn home_work(sub: &str) -> String {
    format!(
        "{}/Desktop/Work/{sub}",
        dirs::home_dir().unwrap().to_string_lossy()
    )
}

#[allow(clippy::too_many_arguments)]
fn seed_block(
    conn: &Connection,
    day: &str,
    started_at: &str,
    ended_at: &str,
    duration_seconds: i64,
    jira_issue: Option<&str>,
    description: Option<&str>,
    is_personal: bool,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks
            (day, jira_issue, started_at, ended_at, duration_seconds, description, is_personal)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            day,
            jira_issue,
            started_at,
            ended_at,
            duration_seconds,
            description,
            is_personal as i64
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// Inserts `ev` and links it to `block_id` via `block_events` — every
/// fixture event is linked directly, so `details_for_block` and
/// `billing::work_folder_for_block` see it regardless of session/span
/// matching.
fn seed_event(conn: &Connection, block_id: i64, ev: Event) -> i64 {
    let id = crate::repo::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, id],
    )
    .unwrap();
    id
}

fn pin_folder(conn: &Connection, folder: &str, customer: &str) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.to_string(),
            customer: Some(customer.to_string()),
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
}

/// The block from spec 006's send/never-send list (D-02): one event per
/// forbidden field, plus one clue clean-room controls should let through.
#[test]
fn clues_send_block_input_never_leaks_forbidden_fields() {
    let conn = db::open_memory().unwrap();
    let folder = "clues-secret-folder";
    let bid = seed_block(
        &conn,
        "2026-05-01",
        "2026-05-01T09:00:00+00:00",
        "2026-05-01T09:30:00+00:00",
        1800,
        None,
        None,
        false,
    );

    // A claude_turn prompt: the full text must never reach the input.
    seed_event(
        &conn,
        bid,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::ClaudePrompt {
                    session_id: "s1".into(),
                    text: "SECRET-PROMPT-TEXT".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal("claude_turn", "e1", "2026-05-01T09:01:00+00:00", "prompt")
        },
    );

    // A claude_tool call: only the file basename may leak, never the
    // tool input.
    seed_event(
        &conn,
        bid,
        Event {
            project_path: Some(home_work(folder)),
            raw_json: Some(
                serde_json::to_string(&RawRecord::ClaudeTool {
                    session_id: "s1".into(),
                    tool: "Bash".into(),
                    input: serde_json::json!({"cmd": "SECRET-TOOL-INPUT"}),
                    output: None,
                    output_cut_bytes: 0,
                    files: vec!["/Users/x/Desktop/Work/vitinn-infra/src/main.rs".into()],
                })
                .unwrap(),
            ),
            ..Event::minimal("claude_tool", "s1:1", "2026-05-01T09:02:00+00:00", "Bash")
        },
    );

    // A shell command: only the program name (title) may leak, never the
    // raw command or the internal host it hit.
    seed_event(
        &conn,
        bid,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::Shell {
                    command: "curl https://internal.example.com/secret?q=1 -H token".into(),
                    cwd: None,
                })
                .unwrap(),
            ),
            ..Event::minimal("shell", "e3", "2026-05-01T09:03:00+00:00", "curl")
        },
    );

    // A Firefox tab: only the URL's host may leak, never the page title.
    seed_event(
        &conn,
        bid,
        Event {
            details: Some("https://jira.apro.is/browse/ABC-1?x=y".into()),
            ..Event::minimal(
                SOURCE_FIREFOX,
                "e4",
                "2026-05-01T09:04:00+00:00",
                "Private page title",
            )
        },
    );

    // A Slack channel message (source_id "C…": a real channel/group id per
    // collectors::slack): only the channel name may leak.
    seed_event(
        &conn,
        bid,
        Event {
            details: Some("SLACK-MESSAGE-TEXT".into()),
            ..Event::minimal(
                SOURCE_SLACK,
                "C0123:1727.1",
                "2026-05-01T09:05:00+00:00",
                "team-dev",
            )
        },
    );

    // A Slack DM (source_id "D…"): nothing about it may leak — not the
    // counterpart's name, not the message.
    seed_event(
        &conn,
        bid,
        Event {
            details: Some("DM-TEXT".into()),
            ..Event::minimal(
                SOURCE_SLACK,
                "D0456:1727.2",
                "2026-05-01T09:06:00+00:00",
                "Jón Jónsson",
            )
        },
    );

    // A group DM shares the channel "G" prefix but is named after people.
    seed_event(
        &conn,
        bid,
        Event::minimal(
            SOURCE_SLACK,
            "G0789:1727.3",
            "2026-05-01T09:06:30+00:00",
            "mpdm-anna--bjorn--tomas-1",
        ),
    );

    // A GitHub commit: only the stripped title may leak, never the repo
    // or the commit body.
    seed_event(
        &conn,
        bid,
        Event {
            repo: Some("aproorg/secret-repo".into()),
            raw_json: Some(
                serde_json::to_string(&RawRecord::Commit {
                    sha: "deadbeef".into(),
                    body: "COMMIT-BODY-TEXT".into(),
                    local_folder: None,
                })
                .unwrap(),
            ),
            ..Event::minimal(
                "github_commit",
                "e7",
                "2026-05-01T09:07:00+00:00",
                "ABC-12 fix login (#44)",
            )
        },
    );

    // A reflog checkout whose branch name embeds an email address.
    seed_event(
        &conn,
        bid,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::Reflog {
                    message: "checkout: moving from main to fix/tomas.ari.palsson@apro.is".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                "git_reflog",
                "e8",
                "2026-05-01T09:08:00+00:00",
                "checkout fix/tomas.ari.palsson@apro.is",
            )
        },
    );

    let input = build_block_input(&conn, bid).unwrap();
    let json = serde_json::to_string(&input).unwrap();

    // A per-block input never groups into work items — the field must
    // not even appear on the wire (`skip_serializing_if`), not just be
    // an empty array.
    assert!(input.work_items.is_empty());
    assert!(
        !json.contains("work_items"),
        "empty work_items must be omitted: {json}"
    );

    for forbidden in [
        "SECRET-PROMPT-TEXT",
        "SECRET-TOOL-INPUT",
        "internal.example.com/secret",
        "Private page title",
        "SLACK-MESSAGE-TEXT",
        "DM-TEXT",
        "Jón Jónsson",
        "mpdm-",
        "COMMIT-BODY-TEXT",
        "aproorg/secret-repo",
        "/Users/",
        "ABC-12",
        "#44",
        "tomas.ari.palsson@apro.is",
    ] {
        assert!(
            !json.contains(forbidden),
            "leaked forbidden field: {forbidden}\n{json}"
        );
    }
    for expected in [
        "main.rs",
        "curl",
        "jira.apro.is",
        "team-dev",
        "fix login",
        folder,
    ] {
        assert!(
            json.contains(expected),
            "missing expected clue: {expected}\n{json}"
        );
    }
}

#[test]
fn clues_send_block_input_errors_for_missing_block() {
    let conn = db::open_memory().unwrap();
    assert!(build_block_input(&conn, 999).is_err());
}

#[test]
fn clues_send_block_input_errors_for_personal_block() {
    let conn = db::open_memory().unwrap();
    let bid = seed_block(
        &conn,
        "2026-05-01",
        "2026-05-01T09:00:00+00:00",
        "2026-05-01T09:30:00+00:00",
        1800,
        None,
        None,
        true,
    );
    assert!(build_block_input(&conn, bid).is_err());
}

#[test]
fn clues_send_claude_helper_summary_never_leaks_but_yields_a_branch() {
    let conn = db::open_memory().unwrap();
    let bid = seed_block(
        &conn,
        "2026-05-02",
        "2026-05-02T09:00:00+00:00",
        "2026-05-02T09:30:00+00:00",
        1800,
        None,
        None,
        false,
    );
    seed_event(
        &conn,
        bid,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::Helper {
                    parent_session_id: "s1".into(),
                    helper_kind: HelperKind::Subagent,
                    summary: "branch fix-login · Edit · edited src/login.rs".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                SOURCE_CLAUDE_HELPER,
                "h1",
                "2026-05-02T09:05:00+00:00",
                "subagent",
            )
        },
    );

    let input = build_block_input(&conn, bid).unwrap();
    assert_eq!(input.branches, vec!["fix-login".to_string()]);
    // Helper file edits are never surfaced (only claude_work's are, per spec).
    assert!(input.file_basenames.is_empty());
}

/// A session whose real work ran in workflow subagents (2026-09-28 VÍS
/// block: every SOW edit was a helper's) must still hand the writer the
/// edited file basenames — D-02 sends basenames from any source.
#[test]
fn clues_send_claude_helper_edited_files_reach_the_writer() {
    let conn = db::open_memory().unwrap();
    let bid = seed_block(
        &conn,
        "2026-09-28",
        "2026-09-28T13:28:00+00:00",
        "2026-09-28T14:20:00+00:00",
        3120,
        None,
        None,
        false,
    );
    seed_event(
        &conn,
        bid,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::Helper {
                    parent_session_id: "s1".into(),
                    helper_kind: HelperKind::Subagent,
                    summary: "Edit ×7, Bash ×2, Read · edited SOW_MCP_Knowledge_Services_VIS.md"
                        .into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                SOURCE_CLAUDE_HELPER,
                "h-sow",
                "2026-09-28T13:40:00+00:00",
                "workflow-subagent: draft:new-sow",
            )
        },
    );

    let input = build_block_input(&conn, bid).unwrap();
    assert_eq!(
        input.file_basenames,
        vec!["SOW_MCP_Knowledge_Services_VIS.md".to_string()]
    );
}

/// Two blocks folded into one billing line: descriptions merge and
/// minutes sum across the group.
#[test]
fn clues_send_line_input_merges_two_blocks() {
    let conn = db::open_memory().unwrap();
    let folder = "clues-line-folder";
    pin_folder(&conn, folder, "Acme Corp");

    let b1 = seed_block(
        &conn,
        "2026-05-03",
        "2026-05-03T09:00:00+00:00",
        "2026-05-03T09:30:00+00:00",
        1800,
        None,
        Some("Did feature A"),
        false,
    );
    seed_event(
        &conn,
        b1,
        Event {
            project_path: Some(home_work(folder)),
            ..Event::minimal("shell", "l1", "2026-05-03T09:01:00+00:00", "ls")
        },
    );

    let b2 = seed_block(
        &conn,
        "2026-05-03",
        "2026-05-03T10:00:00+00:00",
        "2026-05-03T10:30:00+00:00",
        1800,
        None,
        Some("Did feature B"),
        false,
    );
    seed_event(
        &conn,
        b2,
        Event {
            project_path: Some(home_work(folder)),
            ..Event::minimal("shell", "l2", "2026-05-03T10:01:00+00:00", "ls")
        },
    );

    let key = BillingLineKey {
        day: "2026-05-03".to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };
    let input = build_line_input(&conn, &key).unwrap();

    assert_eq!(input.minutes, 60);
    assert_eq!(
        input.block_descriptions,
        vec!["Did feature A".to_string(), "Did feature B".to_string()]
    );
}

/// Two blocks sharing a branch (at 09:00 and 15:00 — grouping is BY TASK,
/// never by time) plus one other-branch block: 2 work items, the
/// branch-sharing pair folded into one with both blocks' minutes summed.
#[test]
fn clues_send_line_input_groups_work_items_by_branch_not_time() {
    let conn = db::open_memory().unwrap();
    let folder = "clues-work-items-folder";
    pin_folder(&conn, folder, "Acme Corp");

    let b1 = seed_block(
        &conn,
        "2026-05-05",
        "2026-05-05T09:00:00+00:00",
        "2026-05-05T09:30:00+00:00",
        1800,
        None,
        None,
        false,
    );
    seed_event(
        &conn,
        b1,
        Event {
            project_path: Some(home_work(folder)),
            raw_json: Some(
                serde_json::to_string(&RawRecord::Helper {
                    parent_session_id: "s1".into(),
                    helper_kind: HelperKind::Subagent,
                    summary: "branch shared-branch · Edit · edited a.rs".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                SOURCE_CLAUDE_HELPER,
                "wi1",
                "2026-05-05T09:05:00+00:00",
                "subagent",
            )
        },
    );
    // A raw secret co-located on the same block — proves grouping never
    // resurrects it through `title` or any `WorkItem` field.
    seed_event(
        &conn,
        b1,
        Event {
            raw_json: Some(
                serde_json::to_string(&RawRecord::Shell {
                    command: "curl https://internal.example.com/WORK-ITEM-SECRET".into(),
                    cwd: None,
                })
                .unwrap(),
            ),
            ..Event::minimal("shell", "wi1-shell", "2026-05-05T09:06:00+00:00", "curl")
        },
    );

    let b2 = seed_block(
        &conn,
        "2026-05-05",
        "2026-05-05T15:00:00+00:00",
        "2026-05-05T15:30:00+00:00",
        1800,
        None,
        None,
        false,
    );
    seed_event(
        &conn,
        b2,
        Event {
            project_path: Some(home_work(folder)),
            raw_json: Some(
                serde_json::to_string(&RawRecord::Helper {
                    parent_session_id: "s2".into(),
                    helper_kind: HelperKind::Subagent,
                    summary: "branch shared-branch · Edit · edited b.rs".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                SOURCE_CLAUDE_HELPER,
                "wi2",
                "2026-05-05T15:05:00+00:00",
                "subagent",
            )
        },
    );

    let b3 = seed_block(
        &conn,
        "2026-05-05",
        "2026-05-05T11:00:00+00:00",
        "2026-05-05T11:45:00+00:00",
        2700,
        None,
        None,
        false,
    );
    seed_event(
        &conn,
        b3,
        Event {
            project_path: Some(home_work(folder)),
            raw_json: Some(
                serde_json::to_string(&RawRecord::Helper {
                    parent_session_id: "s3".into(),
                    helper_kind: HelperKind::Subagent,
                    summary: "branch other-branch · Edit · edited c.rs".into(),
                })
                .unwrap(),
            ),
            ..Event::minimal(
                SOURCE_CLAUDE_HELPER,
                "wi3",
                "2026-05-05T11:05:00+00:00",
                "subagent",
            )
        },
    );

    let key = BillingLineKey {
        day: "2026-05-05".to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };
    let input = build_line_input(&conn, &key).unwrap();

    assert_eq!(input.work_items.len(), 2, "{:?}", input.work_items);
    assert_eq!(input.work_items[0].minutes, 60);
    assert_eq!(
        input.work_items[0].branches,
        vec!["shared-branch".to_string()]
    );
    assert_eq!(input.work_items[1].minutes, 45);
    assert_eq!(
        input.work_items[1].branches,
        vec!["other-branch".to_string()]
    );

    let json = serde_json::to_string(&input).unwrap();
    assert!(
        !json.contains("WORK-ITEM-SECRET") && !json.contains("internal.example.com"),
        "leaked forbidden field through work_items: {json}"
    );
}

#[test]
fn clues_send_line_input_errors_for_unknown_line() {
    let conn = db::open_memory().unwrap();
    let key = BillingLineKey {
        day: "2026-05-04".to_string(),
        folder: "nonexistent".to_string(),
        customer: String::new(),
    };
    assert!(build_line_input(&conn, &key).is_err());
}
