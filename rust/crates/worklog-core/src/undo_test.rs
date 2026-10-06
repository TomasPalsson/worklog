use super::*;
use crate::block_service as bs;
use crate::daily_helpers_contract::{BlockChange, UndoOutcome, UNDO_DEPTH};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use rusqlite::{params, Connection};

fn seed(conn: &Connection, start: &str, end: &str, secs: i64) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue)
         VALUES ('2026-04-18', ?1, ?2, ?3, 'orig', 'AAA-1')",
        params![start, end, secs],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed30(conn: &Connection) -> i64 {
    seed(
        conn,
        "2026-04-18T09:00:00+00:00",
        "2026-04-18T09:30:00+00:00",
        1800,
    )
}

fn link(conn: &Connection, block: i64, source_id: &str, at: &str) {
    let eid = repo::upsert_event(conn, &Event::minimal("claude", source_id, at, "x")).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block, eid],
    )
    .unwrap();
}

fn links(conn: &Connection, block: i64) -> Vec<String> {
    let mut st = conn
        .prepare(
            "SELECT e.source_id FROM block_events be JOIN events e ON e.id = be.event_id
              WHERE be.block_id = ?1 ORDER BY e.source_id",
        )
        .unwrap();
    st.query_map(params![block], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn dump(conn: &Connection) -> String {
    let mut st = conn.prepare("SELECT * FROM blocks ORDER BY id").unwrap();
    let n = st.column_count();
    let rows: Vec<String> = st
        .query_map([], |r| {
            Ok((0..n)
                .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.join("\n")
}

fn restored(change: BlockChange, ids: &[i64]) -> UndoOutcome {
    UndoOutcome::Restored {
        change,
        block_ids: ids.to_vec(),
    }
}

fn desc(conn: &Connection, id: i64) -> Option<String> {
    repo::get_block(conn, id).unwrap().unwrap().description
}

#[test]
fn empty_journal_has_nothing_to_undo() {
    let mut conn = open_memory().unwrap();
    assert_eq!(undo_last(&mut conn).unwrap(), UndoOutcome::NothingToUndo);
}

#[test]
fn failed_change_leaves_no_journal_entry() {
    // catches: recording before validating, so a no-op delete becomes "undoable"
    let mut conn = open_memory().unwrap();
    assert!(bs::delete_block(&conn, 999).is_err());
    assert_eq!(undo_last(&mut conn).unwrap(), UndoOutcome::NothingToUndo);
}

#[test]
fn undo_ticket_restores_key_and_origin() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    let before = dump(&conn);
    bs::assign_ticket(&conn, id, Some("BBB-2")).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Ticket, &[id])
    );
    assert_eq!(dump(&conn), before); // catches: restoring jira_issue but not ticket_origin
}

#[test]
fn undo_hours_restores_duration_and_end() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    let before = dump(&conn);
    bs::set_duration(&conn, id, 90).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Hours, &[id])
    );
    assert_eq!(dump(&conn), before); // catches: forgetting ended_at / estimated_by
}

#[test]
fn undo_text_restores_description() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    let before = dump(&conn);
    bs::set_description(&conn, id, "changed").unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Text, &[id])
    );
    assert_eq!(dump(&conn), before);
}

#[test]
fn undo_personal_restores_flag() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    let before = dump(&conn);
    bs::set_personal(&conn, id, true).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Personal, &[id])
    );
    assert_eq!(dump(&conn), before);
}

#[test]
fn undo_ignored_restores_both_flags() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    let before = dump(&conn);
    bs::set_ignored(&conn, id, true).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Ignored, &[id])
    );
    assert_eq!(dump(&conn), before); // catches: clearing ignored_at but leaving is_personal = 1
}

#[test]
fn undo_delete_brings_block_back_with_id_and_events() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    link(&conn, id, "e1", "2026-04-18T09:00:00+00:00");
    let before = dump(&conn);
    bs::delete_block(&conn, id).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Delete, &[id])
    );
    assert_eq!(dump(&conn), before); // catches: re-insert under a new id
    assert_eq!(links(&conn, id), vec!["e1"]); // catches: dropping cascade-deleted event links
}

#[test]
fn undo_merge_restores_every_block_and_event_link() {
    let mut conn = open_memory().unwrap();
    let a = seed30(&conn);
    let b = seed(
        &conn,
        "2026-04-18T10:00:00+00:00",
        "2026-04-18T10:20:00+00:00",
        1200,
    );
    link(&conn, a, "ea", "2026-04-18T09:00:00+00:00");
    link(&conn, b, "eb", "2026-04-18T10:00:00+00:00");
    let before = dump(&conn);
    bs::merge_blocks(&conn, a, &[b]).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Merge, &[a, b])
    );
    assert_eq!(dump(&conn), before); // catches: restoring only the absorbed row, not the primary total
    assert_eq!(links(&conn, a), vec!["ea"]); // catches: absorbed event left on the primary
    assert_eq!(links(&conn, b), vec!["eb"]);
}

#[test]
fn undo_split_removes_tail_and_restores_events() {
    let mut conn = open_memory().unwrap();
    let a = seed30(&conn);
    link(&conn, a, "e1", "2026-04-18T09:00:00+00:00");
    link(&conn, a, "e2", "2026-04-18T09:20:00+00:00");
    let before = dump(&conn);
    bs::split_block(&conn, a, 10).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Split, &[a])
    );
    assert_eq!(dump(&conn), before); // catches: tail block left behind
    assert_eq!(links(&conn, a), vec!["e1", "e2"]); // catches: events stay on the deleted tail
}

#[test]
fn undo_is_newest_first() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    bs::set_description(&conn, id, "one").unwrap();
    bs::set_description(&conn, id, "two").unwrap();
    undo_last(&mut conn).unwrap();
    assert_eq!(desc(&conn, id).as_deref(), Some("one")); // catches: oldest-first
    undo_last(&mut conn).unwrap();
    assert_eq!(desc(&conn, id).as_deref(), Some("orig"));
    assert_eq!(undo_last(&mut conn).unwrap(), UndoOutcome::NothingToUndo);
}

#[test]
fn keeps_exactly_the_last_twenty_changes() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    for i in 0..=UNDO_DEPTH {
        bs::set_description(&conn, id, &format!("v{i}")).unwrap();
    }
    for _ in 0..UNDO_DEPTH {
        // catches: depth 19
        assert!(matches!(
            undo_last(&mut conn).unwrap(),
            UndoOutcome::Restored { .. }
        ));
    }
    // catches: depth 21
    assert_eq!(undo_last(&mut conn).unwrap(), UndoOutcome::NothingToUndo);
    // the oldest change (orig -> v0) is forgotten, so v0 stays
    assert_eq!(desc(&conn, id).as_deref(), Some("v0"));
}

#[test]
fn refuses_synced_block_and_changes_nothing() {
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = 'TW-1' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    bs::set_description(&conn, id, "after sync").unwrap();
    let before = dump(&conn);
    let refused = UndoOutcome::RefusedSynced { block_id: id };
    assert_eq!(undo_last(&mut conn).unwrap(), refused);
    assert_eq!(dump(&conn), before);
    // catches: dropping the entry on refusal
    assert_eq!(undo_last(&mut conn).unwrap(), refused);
}

#[test]
fn refuses_when_a_deleted_block_was_synced() {
    // catches: re-inserting a deleted synced block (would restore the sent-marker)
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = 'TW-1' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    bs::delete_block(&conn, id).unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        UndoOutcome::RefusedSynced { block_id: id }
    );
    assert_eq!(dump(&conn), "");
}

#[test]
fn blank_tempo_id_is_not_synced() {
    // catches: treating "" as a sent-marker
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    bs::set_description(&conn, id, "x").unwrap();
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = '' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    assert_eq!(
        undo_last(&mut conn).unwrap(),
        restored(BlockChange::Text, &[id])
    );
}

#[test]
fn undo_never_touches_sent_or_exported_markers() {
    // catches: restoring the whole before-row, nulling a marker set since
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    bs::set_description(&conn, id, "x").unwrap();
    conn.execute(
        "UPDATE blocks SET exported_at = '2026-04-19T00:00:00Z' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    undo_last(&mut conn).unwrap();
    let b = repo::get_block(&conn, id).unwrap().unwrap();
    assert_eq!(b.exported_at.as_deref(), Some("2026-04-19T00:00:00Z"));
    assert!(b.tempo_worklog_id.is_none());
}

#[test]
fn refuses_when_rebuild_replaced_the_block() {
    // catches: re-inserting the stale row under its old id beside the rebuilt block
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    bs::set_description(&conn, id, "edited").unwrap();
    conn.execute("DELETE FROM blocks WHERE id = ?1", params![id])
        .unwrap();
    let new_id = seed30(&conn);
    assert!(undo_last(&mut conn).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(desc(&conn, new_id).as_deref(), Some("orig"));
    // catches: keeping a stale entry that blocks the journal forever
    assert_eq!(undo_last(&mut conn).unwrap(), UndoOutcome::NothingToUndo);
}

#[test]
fn refuses_when_live_block_changed_since() {
    // catches: overwriting a later non-journaled write with the before-image
    let mut conn = open_memory().unwrap();
    let id = seed30(&conn);
    bs::set_description(&conn, id, "edited").unwrap();
    conn.execute(
        "UPDATE blocks SET description = 'estimator' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    assert!(undo_last(&mut conn).is_err());
    assert_eq!(desc(&conn, id).as_deref(), Some("estimator"));
}

#[test]
fn undo_split_keeps_unrelated_block_inside_span() {
    // catches: deleting every block above max_id inside the original span
    let mut conn = open_memory().unwrap();
    let a = seed30(&conn);
    bs::split_block(&conn, a, 10).unwrap();
    let other = seed(
        &conn,
        "2026-04-18T09:12:00+00:00",
        "2026-04-18T09:15:00+00:00",
        180,
    );
    undo_last(&mut conn).unwrap();
    assert!(repo::get_block(&conn, other).unwrap().is_some());
    assert_eq!(
        repo::get_block(&conn, a).unwrap().unwrap().duration_seconds,
        1800
    );
}
