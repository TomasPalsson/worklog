use super::*;
use crate::block_digest::write_digest;
use crate::db::open_memory;
use crate::digest_contract::BlockDigest;
use crate::models::Event;
use crate::repo;
use rusqlite::{params, Connection};

fn block(conn: &Connection, day: &str, hour: u32, ticket: Option<&str>, text: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue)
         VALUES (?1, ?2, ?3, 3600, ?4, ?5)",
        params![
            day,
            format!("{day}T{hour:02}:00:00+00:00"),
            format!("{day}T{:02}:00:00+00:00", hour + 1),
            text,
            ticket
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn card(conn: &Connection, id: i64, prompts: &[&str], files: &[&str]) {
    let d = BlockDigest {
        prompts: prompts.iter().map(|s| s.to_string()).collect(),
        files: files.iter().map(|s| s.to_string()).collect(),
        ..BlockDigest::default()
    };
    write_digest(conn, id, &d).unwrap();
}

fn in_repo(conn: &Connection, id: i64, repo_name: &str, sid: &str) {
    let mut e = Event::minimal("claude_turn", sid, "2026-04-18T09:00:00+00:00", "t");
    e.repo = Some(repo_name.to_string());
    let eid = repo::upsert_event(conn, &e).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![id, eid],
    )
    .unwrap();
}

fn ask(conn: &Connection, q: &str) -> Vec<Hit> {
    search(conn, q).unwrap()
}

fn index_dump(conn: &Connection) -> Vec<(i64, String)> {
    let mut st = conn
        .prepare("SELECT block_id, text FROM ask_index ORDER BY block_id")
        .unwrap();
    st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn edits_reindex_so_search_never_returns_old_text() {
    let conn = open_memory().unwrap();
    let id = block(&conn, "2026-04-18", 9, None, "alpha work");
    sync(&conn).unwrap();
    // catches: description edit leaving the old text indexed
    crate::block_service::set_description(&conn, id, "bravo work").unwrap();
    assert!(ask(&conn, "alpha").is_empty());
    assert_eq!(ask(&conn, "bravo").len(), 1);
    // catches: ticket edit not re-indexed
    crate::block_service::assign_ticket(&conn, id, Some("ZED-9")).unwrap();
    assert_eq!(ask(&conn, "ZED-9").len(), 1);
}

#[test]
fn delete_and_merge_drop_stale_index_rows() {
    let conn = open_memory().unwrap();
    let a = block(&conn, "2026-04-18", 9, None, "keeper");
    let b = block(&conn, "2026-04-18", 10, None, "absorbed words");
    let c = block(&conn, "2026-04-18", 12, None, "deleted words");
    sync(&conn).unwrap();
    // catches: merge leaving the absorbed block's row behind
    crate::block_service::merge_blocks(&conn, a, &[b]).unwrap();
    assert!(ask(&conn, "absorbed").is_empty());
    // catches: delete leaving the row behind
    crate::block_service::delete_block(&conn, c).unwrap();
    assert!(index_dump(&conn).iter().all(|(id, _)| *id != c && *id != b));
}

#[test]
fn split_indexes_the_new_tail() {
    let conn = open_memory().unwrap();
    let id = block(&conn, "2026-04-18", 9, None, "tailword");
    sync(&conn).unwrap();
    // catches: split not indexing the tail block
    crate::block_service::split_block(&conn, id, 20).unwrap();
    assert_eq!(ask(&conn, "tailword").len(), 2);
}

#[test]
fn returns_five_newest_first_with_date_time_and_ticket() {
    let conn = open_memory().unwrap();
    // Six matches on six days: a missing LIMIT returns 6; ordering by id or
    // by relevance instead of time breaks the order check.
    for d in [3, 1, 6, 2, 5, 4] {
        block(
            &conn,
            &format!("2026-04-0{d}"),
            9,
            Some("AAA-1"),
            "refactor invoice",
        );
    }
    sync(&conn).unwrap();
    let hits = ask(&conn, "invoice");
    let days: Vec<&str> = hits.iter().map(|h| h.day.as_str()).collect();
    assert_eq!(
        days,
        [
            "2026-04-06",
            "2026-04-05",
            "2026-04-04",
            "2026-04-03",
            "2026-04-02"
        ]
    );
    assert_eq!(hits[0].started_at, "2026-04-06T09:00:00+00:00");
    assert_eq!(hits[0].ended_at, "2026-04-06T10:00:00+00:00");
    assert_eq!(hits[0].jira_issue.as_deref(), Some("AAA-1"));
}

#[test]
fn exactly_five_matches_all_returned_and_unticketed_block_has_none() {
    let conn = open_memory().unwrap();
    for d in 1..=5 {
        block(&conn, &format!("2026-04-0{d}"), 9, None, "refactor invoice");
    }
    sync(&conn).unwrap();
    let hits = ask(&conn, "invoice");
    assert_eq!(hits.len(), 5); // LIMIT 4 / `< 5` off-by-one
    assert_eq!(hits[0].jira_issue, None);
}

#[test]
fn finds_digest_prompts_and_files_not_only_description() {
    let conn = open_memory().unwrap();
    let id = block(&conn, "2026-04-01", 9, None, "misc");
    card(&conn, id, &["fix the zebra parser please"], &["quokka.rs"]);
    sync(&conn).unwrap();
    assert_eq!(ask(&conn, "zebra").len(), 1); // index of description only
    assert_eq!(ask(&conn, "quokka").len(), 1);
}

#[test]
fn no_match_and_blank_query_return_nothing() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-04-01", 9, None, "refactor invoice");
    sync(&conn).unwrap();
    assert!(ask(&conn, "nonexistentword").is_empty());
    assert!(ask(&conn, "").is_empty()); // MATCH '' is a syntax error
    assert!(ask(&conn, "   ").is_empty());
}

#[test]
fn operator_characters_are_text_not_syntax() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-04-01", 9, None, "refactor invoice");
    sync(&conn).unwrap();
    // Unescaped, each of these is an FTS5 syntax error or an operator.
    for q in [
        "\"",
        "invoice\"",
        "AND",
        "(invoice",
        "invoice OR",
        "-invoice",
        "inv*",
        "a:b",
        "^",
    ] {
        assert!(search(&conn, q).is_ok(), "query {q:?} errored");
    }
    assert_eq!(ask(&conn, "invoice AND").len(), 0); // AND must be a literal word
    assert_eq!(ask(&conn, "\"refactor\" invoice").len(), 1);
}

#[test]
fn sync_is_idempotent_and_drops_deleted_blocks() {
    let conn = open_memory().unwrap();
    let a = block(&conn, "2026-04-01", 9, None, "refactor invoice");
    block(&conn, "2026-04-02", 9, None, "refactor invoice");
    sync(&conn).unwrap();
    sync(&conn).unwrap();
    assert_eq!(ask(&conn, "invoice").len(), 2); // double insert shows 4
    conn.execute("DELETE FROM blocks WHERE id = ?1", [a])
        .unwrap();
    sync(&conn).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM ask_index", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1); // orphan row left behind
}

#[test]
fn search_changes_nothing() {
    let conn = open_memory().unwrap();
    let id = block(&conn, "2026-04-01", 9, Some("AAA-1"), "refactor invoice");
    card(&conn, id, &["a prompt about invoices here"], &["x.rs"]);
    sync(&conn).unwrap();
    let index_before = index_dump(&conn);
    // query_only turns any write by ask into an error, which unwrap surfaces.
    conn.pragma_update(None, "query_only", true).unwrap();
    assert_eq!(ask(&conn, "invoice").len(), 1); // a no-op stub must not pass
    ask(&conn, "nothing");
    ask(&conn, "\"");
    where_stopped(&conn, "worklog").unwrap();
    assert_eq!(index_dump(&conn), index_before);
}

#[test]
fn where_stopped_returns_last_three_prompts_and_files_of_that_repo_only() {
    let conn = open_memory().unwrap();
    let old = block(&conn, "2026-04-01", 9, None, "a");
    card(
        &conn,
        old,
        &["oldest prompt one", "oldest prompt two"],
        &["old.rs"],
    );
    in_repo(&conn, old, "worklog", "s1");
    let new = block(&conn, "2026-04-02", 9, None, "b");
    card(
        &conn,
        new,
        &["newer prompt one", "newer prompt two"],
        &["new.rs", "lib.rs"],
    );
    in_repo(&conn, new, "worklog", "s2");
    let other = block(&conn, "2026-04-03", 9, None, "c");
    card(&conn, other, &["other repo prompt"], &["other.rs"]);
    in_repo(&conn, other, "elsewhere", "s3");

    let s = where_stopped(&conn, "worklog").unwrap();
    // Newest first; 3 not 2 or 4; the other repo's newer block is excluded.
    assert_eq!(
        s.prompts,
        ["newer prompt two", "newer prompt one", "oldest prompt two"]
    );
    assert!(s.files.contains(&"new.rs".to_string()));
    assert!(!s.files.contains(&"other.rs".to_string()));
}

#[test]
fn where_stopped_unknown_repo_is_empty() {
    let conn = open_memory().unwrap();
    let s = where_stopped(&conn, "nope").unwrap();
    assert!(s.prompts.is_empty() && s.files.is_empty());
}

fn indexed_ids(conn: &Connection) -> Vec<i64> {
    conn.prepare("SELECT block_id FROM ask_index ORDER BY block_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn sync_batch_caps_at_limit_newest_first_and_reports_remaining() {
    let conn = open_memory().unwrap();
    let oldest = block(&conn, "2026-04-01", 9, None, "a");
    let mid = block(&conn, "2026-04-02", 9, None, "b");
    let newest = block(&conn, "2026-04-03", 9, None, "c");
    // catches: ORDER BY id ASC (oldest indexed first) and an ignored limit (all 3 indexed)
    assert_eq!(sync_batch(&conn, 2).unwrap(), 1);
    assert_eq!(indexed_ids(&conn), vec![mid, newest]);
    // catches: remaining reported as the limit or left stale
    assert_eq!(sync_batch(&conn, 2).unwrap(), 0);
    assert_eq!(indexed_ids(&conn), vec![oldest, mid, newest]);
    // catches: an exactly-at-limit batch miscounting remaining
    assert_eq!(sync_batch(&conn, 3).unwrap(), 0);
}

#[test]
fn sync_batch_orders_by_start_not_id() {
    let conn = open_memory().unwrap();
    let newest = block(&conn, "2026-04-09", 9, None, "a");
    block(&conn, "2026-04-01", 9, None, "b");
    // catches: ordering by id DESC instead of started_at DESC
    assert_eq!(sync_batch(&conn, 1).unwrap(), 1);
    assert_eq!(indexed_ids(&conn), vec![newest]);
}

#[test]
fn sync_batch_drops_rows_of_deleted_blocks() {
    let conn = open_memory().unwrap();
    let gone = block(&conn, "2026-04-01", 9, None, "a");
    block(&conn, "2026-04-02", 9, None, "b");
    sync_batch(&conn, 10).unwrap();
    conn.execute("DELETE FROM blocks WHERE id = ?1", [gone])
        .unwrap();
    sync_batch(&conn, 10).unwrap();
    // catches: orphan rows kept when only a batch runs
    assert!(!indexed_ids(&conn).contains(&gone));
}
