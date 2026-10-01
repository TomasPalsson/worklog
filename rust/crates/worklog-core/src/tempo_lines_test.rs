//! Tests for T005 — stored Tempo ticket lines (spec 011, B6, B7).

use super::*;
use crate::db;
use crate::estimate::FixedInvoker;
use rusqlite::params;

const DAY: &str = "2026-09-30";

fn seed_block(
    conn: &Connection,
    issue: Option<&str>,
    started_at: &str,
    duration_seconds: i64,
    description: Option<&str>,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
        params![DAY, issue, started_at, duration_seconds, description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn key(issue: &str) -> TempoLineKey {
    TempoLineKey {
        day: DAY.to_string(),
        jira_issue: issue.to_string(),
    }
}

fn mark_synced(conn: &Connection, block_id: i64, tempo_id: &str) {
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = ?2 WHERE id = ?1",
        params![block_id, tempo_id],
    )
    .unwrap();
}

fn dirty_of(conn: &Connection, block_id: i64) -> i64 {
    conn.query_row("SELECT dirty FROM blocks WHERE id = ?1", [block_id], |r| {
        r.get(0)
    })
    .unwrap()
}

fn row_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM tempo_line_texts", [], |r| r.get(0))
        .unwrap()
}

fn text_body(issue: &str, text: &str) -> SetTempoLineText {
    SetTempoLineText {
        day: DAY.to_string(),
        jira_issue: issue.to_string(),
        text: text.to_string(),
    }
}

fn hours_body(issue: &str, seconds: Option<i64>) -> SetTempoLineHours {
    SetTempoLineHours {
        day: DAY.to_string(),
        jira_issue: issue.to_string(),
        seconds,
    }
}

#[test]
fn lines_for_day_unions_overlaps_and_skips_personal_and_unticketed() {
    let conn = db::open_memory().unwrap();
    // 10:00-10:30 and 10:15-11:00 overlap: union is 10:00-11:00 = 3600s.
    seed_block(
        &conn,
        Some("APRO-2"),
        "2026-09-30T10:00:00Z",
        1800,
        Some("Alpha"),
    );
    seed_block(
        &conn,
        Some("APRO-2"),
        "2026-09-30T10:15:00Z",
        2700,
        Some("Beta"),
    );
    // 10 minutes rounds to zero.
    seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T12:00:00Z",
        600,
        Some("Solo"),
    );
    let personal = seed_block(&conn, Some("APRO-3"), "2026-09-30T13:00:00Z", 3600, None);
    conn.execute(
        "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
        [personal],
    )
    .unwrap();
    seed_block(&conn, None, "2026-09-30T14:00:00Z", 3600, None);
    seed_block(&conn, Some(""), "2026-09-30T15:00:00Z", 3600, None);

    let lines = lines_for_day(&conn, DAY).unwrap();

    let keys: Vec<&str> = lines.iter().map(|l| l.jira_issue.as_str()).collect();
    assert_eq!(keys, ["APRO-1", "APRO-2"]);
    assert_eq!(lines[0].union_seconds, 0);
    assert_eq!(lines[0].fallback_text, "Solo");
    assert_eq!(lines[1].union_seconds, 3600);
    assert_eq!(lines[1].effective_seconds, 3600);
    assert_eq!(lines[1].fallback_text, "Alpha; Beta");
    assert_eq!(lines[1].text, None);
    assert_eq!(lines[1].text_origin, None);
}

#[test]
fn line_for_is_none_without_blocks_for_that_ticket() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, Some("APRO-1"), "2026-09-30T10:00:00Z", 1800, None);
    assert!(line_for(&conn, &key("APRO-9")).unwrap().is_none());
    assert!(line_for(&conn, &key("APRO-1")).unwrap().is_some());
}

#[test]
fn set_hours_validates_and_override_drives_effective_seconds() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, Some("APRO-1"), "2026-09-30T10:00:00Z", 3600, None);
    for bad in [0, -1800, 2700, 1] {
        set_hours(&conn, &hours_body("APRO-1", Some(bad)))
            .expect_err("only positive multiples of 1800 are valid");
    }
    assert_eq!(row_count(&conn), 0, "nothing written on rejection");

    let line = set_hours(&conn, &hours_body("APRO-1", Some(5400)))
        .unwrap()
        .unwrap();
    assert_eq!(line.union_seconds, 3600);
    assert_eq!(line.hours_override_seconds, Some(5400));
    assert_eq!(line.effective_seconds, 5400);

    let cleared = set_hours(&conn, &hours_body("APRO-1", None))
        .unwrap()
        .unwrap();
    assert_eq!(cleared.effective_seconds, 3600);
    assert_eq!(row_count(&conn), 0, "empty row is deleted");

    assert!(set_hours(&conn, &hours_body("APRO-9", Some(1800)))
        .unwrap()
        .is_none());
    assert_eq!(row_count(&conn), 0);
}

#[test]
fn set_text_stores_manual_blank_clears_and_dirties_only_synced_blocks() {
    let conn = db::open_memory().unwrap();
    let synced = seed_block(&conn, Some("APRO-1"), "2026-09-30T10:00:00Z", 1800, None);
    mark_synced(&conn, synced, "77");
    let empty_id = seed_block(&conn, Some("APRO-1"), "2026-09-30T11:00:00Z", 1800, None);
    mark_synced(&conn, empty_id, "");
    let unsynced = seed_block(&conn, Some("APRO-1"), "2026-09-30T12:00:00Z", 1800, None);
    let other_ticket = seed_block(&conn, Some("APRO-2"), "2026-09-30T13:00:00Z", 1800, None);
    mark_synced(&conn, other_ticket, "78");

    let line = set_text(&conn, &text_body("APRO-1", "  Wrote the report  "))
        .unwrap()
        .unwrap();
    assert_eq!(line.text.as_deref(), Some("Wrote the report"));
    assert_eq!(line.text_origin, Some(LineTextOrigin::Manual));
    assert_eq!(dirty_of(&conn, synced), 1);
    assert_eq!(dirty_of(&conn, empty_id), 0);
    assert_eq!(dirty_of(&conn, unsynced), 0);
    assert_eq!(dirty_of(&conn, other_ticket), 0);

    let cleared = set_text(&conn, &text_body("APRO-1", "   "))
        .unwrap()
        .unwrap();
    assert_eq!(cleared.text, None);
    assert_eq!(cleared.text_origin, None);
    assert_eq!(row_count(&conn), 0);

    assert!(set_text(&conn, &text_body("APRO-9", "x"))
        .unwrap()
        .is_none());
}

#[test]
fn clearing_text_keeps_the_row_while_an_override_remains() {
    let conn = db::open_memory().unwrap();
    seed_block(&conn, Some("APRO-1"), "2026-09-30T10:00:00Z", 1800, None);
    set_hours(&conn, &hours_body("APRO-1", Some(3600))).unwrap();
    set_text(&conn, &text_body("APRO-1", "Mine")).unwrap();
    let line = set_text(&conn, &text_body("APRO-1", "")).unwrap().unwrap();
    assert_eq!(line.text, None);
    assert_eq!(line.hours_override_seconds, Some(3600));
    assert_eq!(row_count(&conn), 1);
}

#[test]
fn pending_generation_covers_missing_and_stale_generated_text_only() {
    let conn = db::open_memory().unwrap();
    let block = seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T10:00:00Z",
        1800,
        Some(" Alpha "),
    );
    seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T11:00:00Z",
        1800,
        Some("Beta"),
    );
    seed_block(
        &conn,
        Some("APRO-2"),
        "2026-09-30T12:00:00Z",
        1800,
        Some("Gamma"),
    );
    set_text(&conn, &text_body("APRO-2", "Hand written")).unwrap();

    let pending = pending_generation(&conn, DAY, None).unwrap();
    assert_eq!(pending.len(), 1);
    let (pending_key, descriptions, hash) = &pending[0];
    assert_eq!(pending_key, &key("APRO-1"));
    assert_eq!(descriptions, &["Alpha".to_string(), "Beta".to_string()]);

    commit_generated(&conn, &key("APRO-1"), "Summary", hash, false).unwrap();
    let line = line_for(&conn, &key("APRO-1")).unwrap().unwrap();
    assert_eq!(line.text.as_deref(), Some("Summary"));
    assert_eq!(line.text_origin, Some(LineTextOrigin::Generated));
    assert!(pending_generation(&conn, DAY, None).unwrap().is_empty());

    conn.execute(
        "UPDATE blocks SET description = 'Alpha changed' WHERE id = ?1",
        [block],
    )
    .unwrap();
    let stale = pending_generation(&conn, DAY, None).unwrap();
    assert_eq!(stale.len(), 1);
    assert_ne!(&stale[0].2, hash, "changed descriptions change the hash");
}

#[test]
fn source_hash_ignores_block_order_and_whitespace() {
    let conn = db::open_memory().unwrap();
    seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T10:00:00Z",
        1800,
        Some("Beta"),
    );
    seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T11:00:00Z",
        1800,
        Some("Alpha"),
    );
    let first = pending_generation(&conn, DAY, None).unwrap()[0].2.clone();
    conn.execute(
        "UPDATE blocks SET description = ' ' || description || ' '",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE blocks SET started_at = CASE started_at
            WHEN '2026-09-30T10:00:00Z' THEN '2026-09-30T11:00:00Z'
            ELSE '2026-09-30T10:00:00Z' END",
        [],
    )
    .unwrap();
    assert_eq!(pending_generation(&conn, DAY, None).unwrap()[0].2, first);
}

#[test]
fn manual_text_survives_generation_unless_forced() {
    let conn = db::open_memory().unwrap();
    seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T10:00:00Z",
        1800,
        Some("Alpha"),
    );
    set_text(&conn, &text_body("APRO-1", "Hand written")).unwrap();

    commit_generated(&conn, &key("APRO-1"), "Model text", "hash", false).unwrap();
    let kept = line_for(&conn, &key("APRO-1")).unwrap().unwrap();
    assert_eq!(kept.text.as_deref(), Some("Hand written"));
    assert_eq!(kept.text_origin, Some(LineTextOrigin::Manual));

    let forced = pending_generation(&conn, DAY, Some(&key("APRO-1"))).unwrap();
    assert_eq!(forced.len(), 1);
    assert_eq!(forced[0].0, key("APRO-1"));
    commit_generated(&conn, &key("APRO-1"), "Model text", &forced[0].2, true).unwrap();
    let replaced = line_for(&conn, &key("APRO-1")).unwrap().unwrap();
    assert_eq!(replaced.text.as_deref(), Some("Model text"));
    assert_eq!(replaced.text_origin, Some(LineTextOrigin::Generated));
}

#[test]
fn commit_generated_keeps_override_and_dirties_synced_blocks() {
    let conn = db::open_memory().unwrap();
    let synced = seed_block(
        &conn,
        Some("APRO-1"),
        "2026-09-30T10:00:00Z",
        1800,
        Some("Alpha"),
    );
    mark_synced(&conn, synced, "77");
    set_hours(&conn, &hours_body("APRO-1", Some(3600))).unwrap();
    conn.execute("UPDATE blocks SET dirty = 0", []).unwrap();

    commit_generated(&conn, &key("APRO-1"), "Summary", "h", false).unwrap();

    let line = line_for(&conn, &key("APRO-1")).unwrap().unwrap();
    assert_eq!(line.hours_override_seconds, Some(3600));
    assert_eq!(line.text.as_deref(), Some("Summary"));
    assert_eq!(dirty_of(&conn, synced), 1);
}

#[test]
fn generate_text_uses_invoker_summary_or_joined_fallback() {
    let descriptions = vec!["Alpha".to_string(), "Beta".to_string()];
    let invoker = FixedInvoker(serde_json::json!({"description": "Implement alpha and beta"}));
    assert_eq!(
        generate_text(Some(&invoker), &key("APRO-1"), &descriptions, "m").as_deref(),
        Some("Implement alpha and beta")
    );
    assert_eq!(
        generate_text(None, &key("APRO-1"), &descriptions, "m").as_deref(),
        Some("Alpha; Beta")
    );
    assert_eq!(
        generate_text(None, &key("APRO-1"), &[], "m").as_deref(),
        Some("Work on APRO-1")
    );
}

struct FailingInvoker;
impl crate::estimate::ModelInvoker for FailingInvoker {
    fn invoke(
        &self,
        _system: &str,
        _user: &str,
        _schema: &serde_json::Value,
        _model: &str,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("model unreachable")
    }
}

#[test]
fn generate_text_is_none_when_the_model_call_fails() {
    let descriptions = vec!["Alpha".to_string(), "Beta".to_string()];
    assert_eq!(
        generate_text(Some(&FailingInvoker), &key("APRO-1"), &descriptions, "m"),
        None
    );
}
