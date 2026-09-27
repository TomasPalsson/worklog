use super::*;
use crate::collectors::claude_transcripts::collect_from_dir;
use crate::db::open_memory;
use crate::purge;
use chrono::NaiveDate;

fn write_line(dir: &Path, project: &str, session: &str, line: &str) {
    let project_dir = dir.join(project);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join(format!("{session}.jsonl")),
        format!("{line}\n"),
    )
    .unwrap();
}

const PROMPT_LINE: &str = r#"{"type":"user","timestamp":"2026-04-18T09:00:00Z","sessionId":"s1","uuid":"u1","cwd":"/home/x/Desktop/Work/widget","message":{"role":"user","content":"fix the bug"}}"#;

#[test]
fn a_conflicting_cached_claim_forces_a_full_reread_instead_of_reuse() {
    // A previous tick fully read this file alone and it won uuid "u1".
    let conn = open_memory().unwrap();
    store(&conn, "/f", 0, 100, 10, 1, &["u1".to_string()], 1, "").unwrap();

    // This run, a sibling file processed earlier already claimed "u1" (e.g.
    // a brand-new resumed copy beating the unchanged original to it).
    let mut seen: HashSet<String> = ["u1".to_string()].into_iter().collect();
    let mut report = CollectReport::default();
    let mut read_ran = false;
    skip_or_read(
        &conn,
        "/f",
        0,
        100,
        10,
        1,
        "",
        &mut seen,
        &mut report,
        |_seen, claimed, report| {
            read_ran = true;
            // The file's own, correct contribution under the CURRENT seen
            // state: "u1" is already taken, so this file itself claims and
            // writes nothing.
            claimed.clear();
            report.events_written += 0;
            Ok(())
        },
    )
    .unwrap();

    assert!(
        read_ran,
        "a cached claim that conflicts with this run's `seen` must not be trusted blindly"
    );
    assert_eq!(
        report.events_written, 0,
        "must reflect this run's actual outcome, not the stale cached one"
    );
}

#[test]
fn a_matching_fingerprint_with_no_conflict_is_skipped() {
    let conn = open_memory().unwrap();
    store(&conn, "/f", 0, 100, 10, 1, &["u1".to_string()], 1, "").unwrap();

    let mut seen = HashSet::new();
    let mut report = CollectReport::default();
    skip_or_read(
        &conn,
        "/f",
        0,
        100,
        10,
        1,
        "",
        &mut seen,
        &mut report,
        |_seen, _claimed, _report| panic!("must not re-read an unconflicted cache hit"),
    )
    .unwrap();

    assert!(seen.contains("u1"), "must reseed the cached claim");
    assert_eq!(report.events_written, 1, "must replay the cached count");
}

#[test]
fn purge_clears_the_cache_so_a_deleted_row_is_restored_by_the_next_tick() {
    // FATAL finding: a fingerprint cache doesn't know purge deleted the
    // rows its file produced, so an untouched file stays skipped forever
    // and a deleted row never comes back, even though the baseline
    // (no cache) always restores it on the next tick.
    let tmp = tempfile::tempdir().unwrap();
    write_line(tmp.path(), "proj", "s1", PROMPT_LINE);
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let until = NaiveDate::from_ymd_opt(2100, 1, 1).unwrap();

    let report = collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(report.events_written, 1, "first tick collects the prompt");

    let cutoff = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let purged = purge::purge_rows(&conn, cutoff, false).unwrap();
    assert_eq!(purged.events_deleted, 1, "purge deletes the orphan event");

    let row_count = |conn: &Connection| -> i64 {
        conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(row_count(&conn), 0, "the row is gone right after purge");

    // Same window, same byte-identical file: a cache that trusts its stale
    // fingerprint would skip the read and merely replay the old
    // `events_written` count, never re-inserting the row into `events` —
    // exactly the bug the review caught (stdout claims it wrote it, the row
    // isn't there). Checking the real table, not just the report, is the
    // point of this test.
    collect_from_dir(&conn, tmp.path(), since, until).unwrap();
    assert_eq!(
        row_count(&conn),
        1,
        "the purged row must be restored on the next tick, like baseline"
    );
}

#[test]
fn a_cached_path_missing_from_disk_forces_a_full_reread_of_its_window() {
    // Finding 2: if a file cached for this window no longer exists, cross-
    // file uuid ownership for a fresh run could differ (e.g. a resumed
    // session's original file was the one removed), so the window's whole
    // cache must be dropped rather than trusted.
    let conn = open_memory().unwrap();
    store(&conn, "/gone", 0, 100, 10, 1, &["u1".to_string()], 1, "").unwrap();

    prepare_window(&conn, 0, 100).unwrap();

    assert!(
        lookup(&conn, "/gone", 0, 100, 10, 1, "").is_none(),
        "a missing cached path must invalidate its whole window"
    );
}

#[test]
fn prepare_window_drops_rows_from_a_different_window() {
    // Retention: the table holds at most one window's worth of state.
    let conn = open_memory().unwrap();
    store(&conn, "/f", 0, 100, 10, 1, &["u1".to_string()], 1, "").unwrap();

    prepare_window(&conn, 200, 300).unwrap();

    assert!(
        lookup(&conn, "/f", 0, 100, 10, 1, "").is_none(),
        "a row from a stale window must not survive into the new one"
    );
}
