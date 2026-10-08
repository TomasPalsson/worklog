use super::*;
use crate::block_service::set_description;
use crate::db::open_memory;
use crate::tempo_hub_contract::HubError;
use serde_json::{json, Value};
use std::cell::RefCell;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()
}

fn note_body(start: &str, minutes: i64, note: &str) -> NoteBlockBody {
    NoteBlockBody {
        jira_issue: "APRO-1".into(),
        day: "2026-04-18".into(),
        start: start.into(),
        minutes,
        note: note.into(),
    }
}

fn saved(conn: &Connection) -> Block {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    log_note_block(conn, &note_body("10:00", 30, "fixed login bug"), today()).unwrap()
}

fn block_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap()
}

fn mark_synced(conn: &Connection, id: i64) {
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '77' WHERE id = ?1",
        [id],
    )
    .unwrap();
}

fn plain_block(conn: &Connection, description: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
         VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800,?1)",
        [description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn fetch(conn: &Connection, id: i64) -> Block {
    crate::repo::get_block(conn, id).unwrap().unwrap()
}

struct Capture {
    reply: Value,
    seen: RefCell<Vec<(String, String, Value, String)>>,
}

impl Capture {
    fn replying(reply: Value) -> Capture {
        Capture {
            reply,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl ModelInvoker for Capture {
    fn invoke(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        model: &str,
    ) -> anyhow::Result<Value> {
        self.seen
            .borrow_mut()
            .push((system.into(), user.into(), schema.clone(), model.into()));
        Ok(self.reply.clone())
    }
}

fn prep() -> NotePrep {
    NotePrep {
        block_id: 1,
        note: "fixed login bug".into(),
        jira_issue: "APRO-1".into(),
        ticket_summary: Some("Login is broken".into()),
        minutes: 30,
    }
}

fn reply_of(text: &str) -> Result<String, String> {
    invoke_note(
        &prep(),
        &Capture::replying(json!({ "description": text })),
        "m",
    )
}

// FR-02b, FR-03, FR-04

#[test]
fn log_note_block_keeps_the_note_beside_a_manual_block() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    assert_eq!(block.rough_note.as_deref(), Some("fixed login bug"));
    assert_eq!(block.description.as_deref(), Some("fixed login bug"));
    assert_eq!(block.description_origin, Some(DescriptionOrigin::Note));
    assert_eq!(block.jira_issue.as_deref(), Some("APRO-1"));
    assert_eq!(block.estimated_by.as_deref(), Some("manual"));
    assert_eq!(block.duration_seconds, 1800);
}

#[test]
fn log_note_block_refuses_to_end_after_midnight() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    let err = log_note_block(&conn, &note_body("23:30", 60, "late"), today()).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<HubError>(),
        Some(HubError::InvalidInput(m)) if m.contains("ends after midnight")
    ));
    assert_eq!(block_count(&conn), 0);
    // one minute past the line: a check that only compares the start is caught
    assert!(log_note_block(&conn, &note_body("23:01", 60, "late"), today()).is_err());
    assert_eq!(block_count(&conn), 0);
}

#[test]
fn log_note_block_allows_ending_exactly_at_midnight() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    // `>=` instead of `>` refuses this one
    let block = log_note_block(&conn, &note_body("23:00", 60, "late"), today()).unwrap();
    assert_eq!(block.ended_at, "2026-04-19T00:00:00+00:00");
}

#[test]
fn log_note_block_midnight_check_uses_local_start_not_utc() {
    let _g = crate::tz::test_env_lock();
    std::env::set_var("WORKLOG_TZ", "+02:00");
    let conn = open_memory().unwrap();
    // local 23:30 is 21:30 UTC: a UTC-based check would let it through
    let refused = log_note_block(&conn, &note_body("23:30", 60, "late"), today());
    // local 00:30 is 22:30 UTC the day before: a UTC-based check would refuse it
    let allowed = log_note_block(&conn, &note_body("00:30", 60, "early"), today());
    std::env::remove_var("WORKLOG_TZ");
    assert!(refused.is_err());
    assert!(allowed.is_ok());
}

#[test]
fn log_note_block_leaves_validation_to_log_time() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");
    let conn = open_memory().unwrap();
    assert!(log_note_block(&conn, &note_body("10:00", 30, "   "), today()).is_err());
    assert!(log_note_block(&conn, &note_body("10:00", 0, "x"), today()).is_err());
    assert!(log_note_block(&conn, &note_body("25:99", 30, "x"), today()).is_err());
    assert_eq!(block_count(&conn), 0);
}

// prepare_note

#[test]
fn prepare_note_reads_note_ticket_summary_and_minutes() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('APRO-1', 'Login is broken')",
        [],
    )
    .unwrap();
    let block = saved(&conn);
    let got = prepare_note(&conn, block.id).unwrap();
    assert_eq!(got.block_id, block.id);
    assert_eq!(got.note, "fixed login bug");
    assert_eq!(got.jira_issue, "APRO-1");
    assert_eq!(got.ticket_summary.as_deref(), Some("Login is broken"));
    assert_eq!(got.minutes, 30);
}

#[test]
fn prepare_note_without_a_cached_ticket_has_no_summary() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    assert_eq!(prepare_note(&conn, block.id).unwrap().ticket_summary, None);
}

#[test]
fn prepare_note_refuses_a_block_without_a_rough_note() {
    let conn = open_memory().unwrap();
    let id = plain_block(&conn, None);
    assert_eq!(
        prepare_note(&conn, id).unwrap_err(),
        REASON_NOT_A_NOTE_BLOCK
    );
}

#[test]
fn prepare_note_refuses_a_missing_block() {
    let conn = open_memory().unwrap();
    assert_eq!(
        prepare_note(&conn, 999).unwrap_err(),
        REASON_NOT_A_NOTE_BLOCK
    );
}

// FR-05, FR-14

#[test]
fn invoke_note_sends_note_ticket_and_minutes_with_the_given_model() {
    let invoker = Capture::replying(json!({ "description": "Fixed the login bug." }));
    let text = invoke_note(&prep(), &invoker, NOTE_MODEL).unwrap();
    assert_eq!(text, "Fixed the login bug.");
    let seen = invoker.seen.borrow();
    let (system, user, schema, model) = &seen[0];
    assert_eq!(model, NOTE_MODEL);
    assert!(user.contains("fixed login bug"));
    assert!(user.contains("APRO-1"));
    assert!(user.contains("Login is broken"));
    assert!(user.contains("30 minutes"));
    assert!(system.contains("past tense"));
    assert_eq!(schema["required"], json!(["description"]));
}

#[test]
fn invoke_note_trims_the_reply() {
    assert_eq!(reply_of("  Fixed it.\n").unwrap(), "Fixed it.");
}

#[test]
fn invoke_note_rejects_an_empty_reply() {
    assert!(reply_of("").is_err());
    // whitespace-only slips through a raw is_empty check
    assert!(reply_of("  \n ").is_err());
}

#[test]
fn invoke_note_accepts_exactly_500_chars_and_rejects_501() {
    assert_eq!(reply_of(&"a".repeat(500)).unwrap().chars().count(), 500);
    assert!(reply_of(&"a".repeat(501)).is_err());
    // chars, not bytes: 500 two-byte chars are fine
    assert!(reply_of(&"é".repeat(500)).is_ok());
}

#[test]
fn invoke_note_rejects_a_reply_without_a_string_description() {
    for bad in [
        json!({}),
        json!({ "description": 5 }),
        json!({ "description": null }),
    ] {
        assert!(invoke_note(&prep(), &Capture::replying(bad), "m").is_err());
    }
}

#[test]
fn invoke_note_reports_the_invokers_own_error() {
    struct Failing;
    impl ModelInvoker for Failing {
        fn invoke(&self, _: &str, _: &str, _: &Value, _: &str) -> anyhow::Result<Value> {
            Err(anyhow::anyhow!("model timed out"))
        }
    }
    assert!(invoke_note(&prep(), &Failing, "m")
        .unwrap_err()
        .contains("model timed out"));
}

// FR-05, FR-07, FR-10

#[test]
fn commit_note_writes_ai_text_and_keeps_the_note() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    commit_note(&conn, block.id, "Fixed the login bug.", false).unwrap();
    let got = fetch(&conn, block.id);
    assert_eq!(got.description.as_deref(), Some("Fixed the login bug."));
    assert_eq!(got.description_origin, Some(DescriptionOrigin::Ai));
    assert_eq!(got.rough_note.as_deref(), Some("fixed login bug"));
    assert_eq!(got.estimated_by.as_deref(), Some("manual"));
}

#[test]
fn commit_note_overwrites_an_earlier_ai_text() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    commit_note(&conn, block.id, "First.", false).unwrap();
    commit_note(&conn, block.id, "Second.", false).unwrap();
    assert_eq!(
        fetch(&conn, block.id).description.as_deref(),
        Some("Second.")
    );
}

#[test]
fn commit_note_drops_the_write_after_a_hand_edit() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    set_description(&conn, block.id, "my own words").unwrap();
    let err = commit_note(&conn, block.id, "AI text", false).unwrap_err();
    assert_eq!(err, REASON_HAND_EDITED);
    let got = fetch(&conn, block.id);
    assert_eq!(got.description.as_deref(), Some("my own words"));
    assert_eq!(got.description_origin, Some(DescriptionOrigin::Hand));
}

#[test]
fn commit_note_with_force_replaces_a_hand_edit() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    set_description(&conn, block.id, "my own words").unwrap();
    commit_note(&conn, block.id, "AI text", true).unwrap();
    let got = fetch(&conn, block.id);
    assert_eq!(got.description.as_deref(), Some("AI text"));
    assert_eq!(got.description_origin, Some(DescriptionOrigin::Ai));
}

#[test]
fn commit_note_treats_an_unreadable_origin_as_hand() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    conn.execute(
        "UPDATE blocks SET description_origin = NULL WHERE id = ?1",
        [block.id],
    )
    .unwrap();
    assert_eq!(
        commit_note(&conn, block.id, "AI text", false).unwrap_err(),
        REASON_HAND_EDITED
    );
    assert_eq!(
        fetch(&conn, block.id).description.as_deref(),
        Some("fixed login bug")
    );
}

#[test]
fn commit_note_refuses_a_block_without_a_rough_note_even_when_forced() {
    let conn = open_memory().unwrap();
    let id = plain_block(&conn, Some("orig"));
    for force in [false, true] {
        assert_eq!(
            commit_note(&conn, id, "AI text", force).unwrap_err(),
            REASON_NOT_A_NOTE_BLOCK
        );
    }
    assert_eq!(fetch(&conn, id).description.as_deref(), Some("orig"));
}

#[test]
fn commit_note_refuses_a_missing_block() {
    let conn = open_memory().unwrap();
    assert_eq!(
        commit_note(&conn, 999, "x", false).unwrap_err(),
        REASON_NOT_A_NOTE_BLOCK
    );
}

#[test]
fn commit_note_marks_a_synced_block_dirty_and_leaves_an_unsynced_one_alone() {
    let conn = open_memory().unwrap();
    let synced = saved(&conn);
    mark_synced(&conn, synced.id);
    let unsynced = saved(&conn);
    commit_note(&conn, synced.id, "AI text", false).unwrap();
    commit_note(&conn, unsynced.id, "AI text", false).unwrap();
    assert!(fetch(&conn, synced.id).dirty);
    assert!(!fetch(&conn, unsynced.id).dirty);
}

#[test]
fn commit_note_logs_the_description_change_as_claude() {
    use crate::change_log;
    use crate::deild_contract::{ChangeField, ChangeSource};
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    change_log::refresh_day(&conn, "2026-04-18", ChangeSource::Rebuild, "seed").unwrap();
    commit_note(&conn, block.id, "AI text", false).unwrap();
    let changes = change_log::feed(&conn, 0).unwrap().changes;
    assert!(changes
        .iter()
        .any(|c| c.field == ChangeField::Description && c.source == ChangeSource::Claude));
}

// FR-06

#[test]
fn editing_a_note_blocks_description_marks_it_hand() {
    let conn = open_memory().unwrap();
    let block = saved(&conn);
    let got = set_description(&conn, block.id, "mine").unwrap();
    assert_eq!(got.description_origin, Some(DescriptionOrigin::Hand));
    assert_eq!(got.rough_note.as_deref(), Some("fixed login bug"));
}

#[test]
fn editing_an_ordinary_blocks_description_leaves_origin_null() {
    let conn = open_memory().unwrap();
    let id = plain_block(&conn, None);
    let got = set_description(&conn, id, "mine").unwrap();
    assert_eq!(got.description_origin, None);
}
