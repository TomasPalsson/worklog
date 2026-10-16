use super::*;
use crate::daily_helpers_contract::UndoOutcome;
use crate::db::open_memory;
use crate::estimate::FixedInvoker;
use crate::models::Event;
use crate::undo::undo_last;
use rusqlite::types::Value as SqlValue;
use std::cell::RefCell;

fn block_with(conn: &Connection, description: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue)
         VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800,?1,'APRO-1')",
        [description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn described(conn: &Connection) -> i64 {
    block_with(conn, Some("Worked on login"))
}

fn snapshot(conn: &Connection, id: i64) -> Vec<(String, SqlValue)> {
    let mut stmt = conn.prepare("SELECT * FROM blocks WHERE id = ?1").unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
    let row: Vec<SqlValue> = stmt
        .query_row([id], |r| (0..names.len()).map(|i| r.get(i)).collect())
        .unwrap();
    names.into_iter().zip(row).collect()
}

fn changed_columns(before: &[(String, SqlValue)], after: &[(String, SqlValue)]) -> Vec<String> {
    before
        .iter()
        .zip(after)
        .filter(|(b, a)| b != a)
        .map(|(b, _)| b.0.clone())
        .collect()
}

fn run(conn: &Connection, id: i64, text: &str) -> Result<RedescribeResult> {
    let reply = FixedInvoker(json!({ "description": text }));
    redescribe_block_with(conn, id, "make it shorter", &reply, "test-model")
}

struct Capture {
    reply: Value,
    seen: RefCell<Vec<(String, String, Value, String)>>,
}

impl ModelInvoker for Capture {
    fn invoke(&self, system: &str, user: &str, schema: &Value, model: &str) -> Result<Value> {
        self.seen
            .borrow_mut()
            .push((system.into(), user.into(), schema.clone(), model.into()));
        Ok(self.reply.clone())
    }
}

fn capture(text: &str) -> Capture {
    Capture {
        reply: json!({ "description": text }),
        seen: RefCell::new(Vec::new()),
    }
}

fn refusal(conn: &Connection, id: i64) -> String {
    let before = snapshot(conn, id);
    let err = run(conn, id, "new").unwrap_err().to_string();
    assert_eq!(snapshot(conn, id), before, "refused block must not change");
    err
}

// FR-01: returns the model's text and changes only the description.
#[test]
fn reindexes_the_block_for_ask() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    run(&conn, id, "Fixed the logout redirect").unwrap();
    let text: String = conn
        .query_row(
            "SELECT text FROM ask_index WHERE block_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(text.contains("Fixed the logout redirect"), "{text}");
}

#[test]
fn rewrites_only_the_description_column() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let before = snapshot(&conn, id);
    let result = run(&conn, id, "Fixed the login bug").unwrap();
    assert_eq!(
        result,
        RedescribeResult {
            block_id: id,
            description: "Fixed the login bug".into()
        }
    );
    // Catches an UPDATE that also stamps estimated_by / description_origin.
    assert_eq!(
        changed_columns(&before, &snapshot(&conn, id)),
        vec!["description"]
    );
}

// FR-02: instruction, current description, ticket and evidence reach the model.
#[test]
fn prompt_carries_instruction_description_ticket_and_evidence() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('APRO-1','Login is broken')",
        [],
    )
    .unwrap();
    let event = Event::minimal(
        "git",
        "c1",
        "2026-04-18T09:05:00+00:00",
        "fix: login redirect",
    );
    let event_id = crate::repo::upsert_event(&conn, &event).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        [id, event_id],
    )
    .unwrap();
    let model = capture("ok");
    redescribe_block_with(&conn, id, "  mention the redirect  ", &model, "test-model").unwrap();
    let seen = model.seen.borrow();
    let (_, user, schema, used_model) = &seen[0];
    assert!(
        user.contains("Instruction: mention the redirect\n"),
        "{user}"
    );
    assert!(user.contains("Worked on login"), "{user}");
    assert!(user.contains("APRO-1 (Login is broken)"), "{user}");
    assert!(user.contains("30 minutes"), "{user}");
    assert!(user.contains("fix: login redirect"), "{user}");
    assert_eq!(schema["required"], json!(["description"]));
    assert_eq!(used_model, "test-model");
}

// FR-03: a second run without the text carries no trace of the first.
#[test]
fn instruction_is_not_remembered() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    run(&conn, id, "first result").unwrap();
    let model = capture("second result");
    redescribe_block_with(&conn, id, "another angle", &model, "m").unwrap();
    let seen = model.seen.borrow();
    assert!(!seen[0].1.contains("make it shorter"), "{}", seen[0].1);
}

// FR-05: each refusal carries its contract reason and leaves the row alone.
#[test]
fn refuses_personal_block() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    conn.execute("UPDATE blocks SET is_personal = 1", [])
        .unwrap();
    assert_eq!(refusal(&conn, id), REASON_PERSONAL);
}

#[test]
fn refuses_note_block() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    conn.execute("UPDATE blocks SET rough_note = 'fixed login'", [])
        .unwrap();
    assert_eq!(refusal(&conn, id), REASON_NOTE_BLOCK);
}

#[test]
fn refuses_missing_empty_and_blank_descriptions() {
    let conn = open_memory().unwrap();
    for description in [None, Some(""), Some("   ")] {
        let id = block_with(&conn, description);
        assert_eq!(refusal(&conn, id), REASON_NOT_DESCRIBED, "{description:?}");
    }
}

#[test]
fn refuses_unknown_block() {
    let conn = open_memory().unwrap();
    let err = run(&conn, 999, "x").unwrap_err().to_string();
    assert_eq!(err, "block 999 not found");
}

// FR-06: synced blocks are flagged dirty; Tempo id and export stamp survive.
#[test]
fn synced_block_is_marked_dirty_and_keeps_its_ids() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '77', exported_at = '2026-04-19T08:00:00+00:00'",
        [],
    )
    .unwrap();
    let before = snapshot(&conn, id);
    run(&conn, id, "Fixed the login bug").unwrap();
    let after = snapshot(&conn, id);
    assert_eq!(
        changed_columns(&before, &after),
        vec!["description", "dirty"]
    );
    let (tempo, exported, dirty): (String, String, i64) = conn
        .query_row(
            "SELECT tempo_worklog_id, exported_at, dirty FROM blocks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (tempo.as_str(), exported.as_str(), dirty),
        ("77", "2026-04-19T08:00:00+00:00", 1)
    );
}

// An unsynced block is not falsely flagged dirty (catches an unconditional dirty = 1).
#[test]
fn unsynced_block_stays_clean() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    run(&conn, id, "Fixed the login bug").unwrap();
    let dirty: i64 = conn
        .query_row("SELECT dirty FROM blocks WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(dirty, 0);
}

// FR-07a: a reply with no usable description fails and leaves the block alone.
#[test]
fn unusable_replies_fail_without_touching_the_block() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let before = snapshot(&conn, id);
    for reply in [
        json!({ "description": "" }),
        json!({ "description": "  \n " }),
        json!({}),
        json!({ "description": 7 }),
    ] {
        let invoker = FixedInvoker(reply.clone());
        assert!(
            redescribe_block_with(&conn, id, "x", &invoker, "m").is_err(),
            "{reply}"
        );
    }
    assert_eq!(snapshot(&conn, id), before);
}

// FR-07a: over 500 chars is cut at the last word boundary, no ellipsis.
#[test]
fn long_reply_is_cut_at_a_word_boundary() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let reply = "aaaa ".repeat(120);
    let result = run(&conn, id, &reply).unwrap();
    assert_eq!(result.description, "aaaa ".repeat(99) + "aaaa");
}

#[test]
fn reply_of_exactly_500_chars_is_kept_whole() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let reply = "aaaa ".repeat(99) + "aaaab";
    assert_eq!(reply.chars().count(), 500);
    // Catches a >= limit check that would cut a 500-char reply.
    assert_eq!(run(&conn, id, &reply).unwrap().description, reply);
}

#[test]
fn reply_one_char_over_is_cut_before_the_straddling_word() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let reply = "aaaa ".repeat(99) + "aaaabc";
    assert_eq!(reply.chars().count(), 501);
    let result = run(&conn, id, &reply).unwrap();
    assert_eq!(result.description, "aaaa ".repeat(98) + "aaaa");
}

#[test]
fn reply_whose_501st_char_is_a_space_keeps_all_500() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let reply = "x".repeat(500) + " tail";
    // Catches cutting a whole word when the boundary falls exactly at the limit.
    assert_eq!(run(&conn, id, &reply).unwrap().description, "x".repeat(500));
}

#[test]
fn long_reply_without_spaces_is_hard_cut_at_500() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let result = run(&conn, id, &"y".repeat(600)).unwrap();
    assert_eq!(result.description, "y".repeat(500));
}

// FR-07: Undo restores the whole row, including the old description and dirty flag.
#[test]
fn undo_restores_the_old_row() {
    let mut conn = open_memory().unwrap();
    let id = described(&conn);
    let before = snapshot(&conn, id);
    run(&conn, id, "Fixed the login bug").unwrap();
    assert_ne!(snapshot(&conn, id), before);
    assert!(matches!(
        undo_last(&mut conn).unwrap(),
        UndoOutcome::Restored { .. }
    ));
    assert_eq!(snapshot(&conn, id), before);
}

#[test]
fn commit_refuses_when_description_changed_since_prepare() {
    let conn = open_memory().unwrap();
    let id = described(&conn);
    let prep = prepare(&conn, id, "make it shorter").unwrap();
    conn.execute("UPDATE blocks SET description = 'hand edit'", [])
        .unwrap();
    let err = commit(&conn, &prep, "AI text").unwrap_err().to_string();
    assert!(err.contains("changed while writing"), "{err}");
    let now: String = conn
        .query_row("SELECT description FROM blocks WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(now, "hand edit");
}
