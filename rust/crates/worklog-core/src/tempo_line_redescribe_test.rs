use super::*;
use crate::clues_contract::LineTextOrigin;
use crate::db::open_memory;
use crate::estimate::FixedInvoker;
use crate::line_text::SYSTEM_PROMPT_IS;
use crate::tempo_line_contract::{SetTempoLineHours, SetTempoLineText, REASON_TEXT_CHANGED};
use crate::tempo_lines;
use rusqlite::params;
use std::sync::Mutex;

const DAY: &str = "2026-10-05";
const NEW_TEXT: &str = "Lagaði villu í innskráningu. Prófaði breytinguna.";
const OLD_TEXT: &str = "Gamall texti. Hann er til.";
const HAND_TEXT: &str = "Handskrifað. Alveg nýtt.";

fn key() -> TempoLineKey {
    TempoLineKey {
        day: DAY.into(),
        jira_issue: "APRO-1".into(),
    }
}

fn seed_block(conn: &Connection, synced: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description, tempo_worklog_id)
         VALUES (?1, 'APRO-1', '2026-10-05T09:00:00+00:00', '2026-10-05T09:30:00+00:00', 1800, 'Worked on login', ?2)",
        params![DAY, synced],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn store_generated(conn: &Connection, text: &str) {
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, text, text_origin, source_hash, check_status, updated_at)
         VALUES (?1, 'APRO-1', ?2, 'generated', 'h1', 'passed', '2026-10-05T10:00:00+00:00')",
        params![DAY, text],
    )
    .unwrap();
}

fn hand_edit(conn: &Connection) {
    tempo_lines::set_text(
        conn,
        &SetTempoLineText {
            day: DAY.into(),
            jira_issue: "APRO-1".into(),
            text: HAND_TEXT.into(),
        },
    )
    .unwrap();
}

fn line(conn: &Connection) -> TempoLine {
    tempo_lines::line_for(conn, &key()).unwrap().unwrap()
}

fn dirty(conn: &Connection, id: i64) -> i64 {
    conn.query_row("SELECT dirty FROM blocks WHERE id = ?1", [id], |r| r.get(0))
        .unwrap()
}

struct Spy(Mutex<Vec<(String, String)>>);

impl ModelInvoker for Spy {
    fn invoke(
        &self,
        system: &str,
        user: &str,
        _schema: &serde_json::Value,
        _model: &str,
    ) -> anyhow::Result<serde_json::Value> {
        self.0.lock().unwrap().push((system.into(), user.into()));
        Ok(serde_json::json!({ "text": NEW_TEXT }))
    }
}

// Catches: returning Some for a line with no blocks.
#[test]
fn prepare_is_none_when_the_line_has_no_blocks() {
    let conn = open_memory().unwrap();
    assert!(prepare(&conn, &key(), "styttra").unwrap().is_none());
}

// Catches: using the stored text only (no fallback) or never trimming.
#[test]
fn prepare_without_stored_text_uses_the_fallback_and_trims() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "  gerðu það styttra \n")
        .unwrap()
        .unwrap();
    assert_eq!(prep.instruction, "gerðu það styttra");
    assert_eq!(prep.stored_text, None);
    assert_eq!(prep.current_text, line(&conn).fallback_text);
    assert!(!prep.current_text.is_empty());
}

// Catches: preferring fallback_text over the stored line text.
#[test]
fn prepare_with_stored_text_uses_it_as_current() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    store_generated(&conn, OLD_TEXT);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    assert_eq!(prep.current_text, OLD_TEXT);
    assert_eq!(prep.stored_text.as_deref(), Some(OLD_TEXT));
}

// Catches: the message dropping the instruction or the current text.
#[test]
fn user_msg_carries_instruction_and_current_text() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "nefndu prófin").unwrap().unwrap();
    let msg: serde_json::Value = serde_json::from_str(&prep.user_msg).unwrap();
    assert_eq!(msg["owner_instruction"], "nefndu prófin");
    assert_eq!(msg["current_text"], prep.current_text.as_str());
}

// Catches: sending only the base prompt, or only the rule.
#[test]
fn write_sends_base_prompt_plus_instruction_rule_and_returns_the_text() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    let spy = Spy(Mutex::new(Vec::new()));
    assert_eq!(write(&prep, &spy, "m").unwrap(), NEW_TEXT);
    let seen = spy.0.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(!INSTRUCTION_RULE_IS.is_empty());
    assert_eq!(
        seen[0].0,
        format!("{SYSTEM_PROMPT_IS}\n\n{INSTRUCTION_RULE_IS}")
    );
    assert!(seen[0].1.contains("owner_instruction"));
}

// Catches: returning unvalidated text / swallowing the writer's error.
#[test]
fn write_surfaces_the_validation_failure() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    let bad = FixedInvoker(serde_json::json!({ "text": "Lagaði src/main.rs. Já." }));
    assert!(write(&prep, &bad, "m")
        .unwrap_err()
        .contains("reynt 3 sinnum"));
}

// Catches: leaving origin 'generated', keeping source_hash/check_status,
// touching hours, block text or tempo id, or not dirtying synced blocks.
#[test]
fn commit_writes_manual_text_and_touches_nothing_else() {
    let conn = open_memory().unwrap();
    let synced = seed_block(&conn, Some("tempo-9"));
    store_generated(&conn, OLD_TEXT);
    tempo_lines::set_hours(
        &conn,
        &SetTempoLineHours {
            day: DAY.into(),
            jira_issue: "APRO-1".into(),
            seconds: Some(7200),
        },
    )
    .unwrap();
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    let got = commit(&conn, &prep, NEW_TEXT).unwrap().unwrap();
    assert_eq!(got, line(&conn));
    assert_eq!(got.text.as_deref(), Some(NEW_TEXT));
    assert_eq!(got.text_origin, Some(LineTextOrigin::Manual));
    assert_eq!(got.check_status, None);
    assert_eq!(got.hours_override_seconds, Some(7200));
    let (hash, desc, tempo): (Option<String>, String, String) = conn
        .query_row(
            "SELECT (SELECT source_hash FROM tempo_line_texts WHERE day = ?2), description, tempo_worklog_id
               FROM blocks WHERE id = ?1",
            params![synced, DAY],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(hash, None);
    assert_eq!(desc, "Worked on login");
    assert_eq!(tempo, "tempo-9");
    assert_eq!(dirty(&conn, synced), 1);
}

// Catches: a plain `text = NULL` guard that never matches, and dirtying
// blocks that were never synced.
#[test]
fn commit_into_a_line_with_no_stored_text_succeeds() {
    let conn = open_memory().unwrap();
    let unsynced = seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    let got = commit(&conn, &prep, NEW_TEXT).unwrap().unwrap();
    assert_eq!(got.text.as_deref(), Some(NEW_TEXT));
    assert_eq!(dirty(&conn, unsynced), 0);
}

// Catches: no lost-update guard; a hand edit made during the model call
// must not be overwritten.
#[test]
fn commit_refuses_when_the_text_was_hand_edited_meanwhile() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    store_generated(&conn, OLD_TEXT);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    hand_edit(&conn);
    let before = line(&conn);
    let err = commit(&conn, &prep, NEW_TEXT).unwrap_err().to_string();
    assert_eq!(err, REASON_TEXT_CHANGED);
    assert_eq!(line(&conn), before);
}

// Catches: a guard that only compares when prepare saw Some text.
#[test]
fn commit_refuses_when_text_appeared_after_a_none_prepare() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    store_generated(&conn, "Nýr texti. Kom á meðan.");
    let err = commit(&conn, &prep, NEW_TEXT).unwrap_err().to_string();
    assert_eq!(err, REASON_TEXT_CHANGED);
    assert_eq!(line(&conn).text.as_deref(), Some("Nýr texti. Kom á meðan."));
}

// Catches: writing text for a line whose blocks are gone (orphan row).
#[test]
fn commit_for_a_vanished_line_is_none_and_leaves_no_row() {
    let conn = open_memory().unwrap();
    let id = seed_block(&conn, None);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    conn.execute("DELETE FROM blocks WHERE id = ?1", [id])
        .unwrap();
    assert!(commit(&conn, &prep, NEW_TEXT).unwrap().is_none());
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM tempo_line_texts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

// Catches: refusing to overwrite a manual text that is unchanged since
// prepare — the Owner asked for the rewrite.
#[test]
fn commit_overwrites_an_unchanged_manual_text() {
    let conn = open_memory().unwrap();
    seed_block(&conn, Some("tempo-9"));
    hand_edit(&conn);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    assert_eq!(prep.current_text, HAND_TEXT);
    let got = commit(&conn, &prep, NEW_TEXT).unwrap().unwrap();
    assert_eq!(got.text.as_deref(), Some(NEW_TEXT));
    assert_eq!(got.text_origin, Some(LineTextOrigin::Manual));
}

// Catches: a second redescribe that reuses the first instruction (cached
// prep, appended history) instead of only the new one.
#[test]
fn second_redescribe_carries_nothing_of_the_first_instruction() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    let first = prepare(&conn, &key(), "gerðu það styttra").unwrap().unwrap();
    commit(&conn, &first, NEW_TEXT).unwrap();

    let second = prepare(&conn, &key(), "nefndu prófin").unwrap().unwrap();
    assert_eq!(second.instruction, "nefndu prófin");
    assert_eq!(second.current_text, NEW_TEXT);
    assert!(!second.user_msg.contains("gerðu það styttra"));
    let spy = Spy(Mutex::new(Vec::new()));
    write(&second, &spy, "m").unwrap();
    let seen = spy.0.lock().unwrap();
    assert!(!seen[0].0.contains("gerðu það styttra"));
    assert!(!seen[0].1.contains("gerðu það styttra"));
    let msg: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(msg["owner_instruction"], "nefndu prófin");
}

// Catches: a redescribed line stored as 'generated', so the automatic
// pass lists it as pending or overwrites it.
#[test]
fn redescribed_line_survives_the_automatic_generation_pass() {
    let conn = open_memory().unwrap();
    seed_block(&conn, None);
    store_generated(&conn, OLD_TEXT);
    let prep = prepare(&conn, &key(), "styttra").unwrap().unwrap();
    commit(&conn, &prep, NEW_TEXT).unwrap();

    assert!(tempo_lines::pending_generation(&conn, DAY, None)
        .unwrap()
        .is_empty());
    tempo_lines::commit_generated(&conn, &key(), "Sjálfvirkur texti.", "h2", false).unwrap();
    let got = line(&conn);
    assert_eq!(got.text.as_deref(), Some(NEW_TEXT));
    assert_eq!(got.text_origin, Some(LineTextOrigin::Manual));
}
