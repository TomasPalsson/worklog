use super::*;
use crate::db;
use crate::models::Event;
use crate::repo;
use rusqlite::params;

fn seed_elsewhere_event(
    conn: &Connection,
    source_id: &str,
    started_at: &str,
    repo_name: &str,
) -> i64 {
    let id = repo::upsert_event(
        conn,
        &Event::minimal("github_commit", source_id, started_at, "fix oauth"),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET elsewhere = 1, repo = ?1 WHERE id = ?2",
        params![repo_name, id],
    )
    .unwrap();
    id
}

fn seed_block(conn: &Connection, start: &str, end: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', ?1, ?2, 1800)",
        params![start, end],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn link_event_with_project(
    conn: &Connection,
    block_id: i64,
    source_id: &str,
    project_path: &str,
) {
    let id = repo::upsert_event(
        conn,
        &Event::minimal("claude", source_id, "2026-04-18T09:05:00+00:00", "x"),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET project_path = ?1 WHERE id = ?2",
        params![project_path, id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, id],
    )
    .unwrap();
}

#[test]
fn list_for_day_excludes_non_elsewhere_and_other_days() {
    let conn = db::open_memory().unwrap();
    seed_elsewhere_event(
        &conn,
        "far-sha",
        "2026-04-18T10:00:00+00:00",
        "aproorg/code-interpreter",
    );
    // Not elsewhere — must be excluded.
    repo::upsert_event(
        &conn,
        &Event::minimal(
            "github_commit",
            "local-sha",
            "2026-04-18T09:00:00+00:00",
            "x",
        ),
    )
    .unwrap();
    // Elsewhere, but a different day — must be excluded.
    seed_elsewhere_event(
        &conn,
        "other-day-sha",
        "2026-04-19T10:00:00+00:00",
        "aproorg/code-interpreter",
    );

    let items = list_for_day(&conn, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()).unwrap();
    assert_eq!(
        items.len(),
        1,
        "expected exactly the one elsewhere event on 2026-04-18"
    );
    assert_eq!(items[0].source, "github_commit");
    assert_eq!(items[0].repo.as_deref(), Some("aproorg/code-interpreter"));
    assert_eq!(items[0].title, "fix oauth");
}

#[test]
fn list_for_day_is_time_ordered() {
    let conn = db::open_memory().unwrap();
    seed_elsewhere_event(&conn, "later-sha", "2026-04-18T14:00:00+00:00", "aproorg/x");
    seed_elsewhere_event(
        &conn,
        "earlier-sha",
        "2026-04-18T08:00:00+00:00",
        "aproorg/x",
    );

    let items = list_for_day(&conn, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].started_at, "2026-04-18T08:00:00+00:00");
    assert_eq!(items[1].started_at, "2026-04-18T14:00:00+00:00");
}

#[test]
fn move_into_block_sets_dominant_project_clears_elsewhere_and_links() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_block(
        &conn,
        "2026-04-18T09:00:00+00:00",
        "2026-04-18T09:30:00+00:00",
    );
    link_event_with_project(
        &conn,
        block_id,
        "a",
        "/Users/tomas/Desktop/Work/code-interpreter",
    );
    link_event_with_project(
        &conn,
        block_id,
        "b",
        "/Users/tomas/Desktop/Work/code-interpreter",
    );
    link_event_with_project(&conn, block_id, "c", "/Users/tomas/Desktop/Work/other");
    let event_id = seed_elsewhere_event(
        &conn,
        "far-sha",
        "2026-04-18T10:00:00+00:00",
        "aproorg/code-interpreter",
    );

    move_into_block(&conn, event_id, block_id).unwrap();

    let (project_path, elsewhere): (Option<String>, i64) = conn
        .query_row(
            "SELECT project_path, elsewhere FROM events WHERE id = ?1",
            params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        project_path.as_deref(),
        Some("/Users/tomas/Desktop/Work/code-interpreter"),
        "must key to the block's dominant project"
    );
    assert_eq!(elsewhere, 2, "must be owner-moved, out of the elsewhere list");

    let linked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM block_events WHERE block_id = ?1 AND event_id = ?2",
            params![block_id, event_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(linked, 1, "must be linked into the block");
}

#[test]
fn move_into_block_errors_on_unknown_event() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_block(
        &conn,
        "2026-04-18T09:00:00+00:00",
        "2026-04-18T09:30:00+00:00",
    );
    let err = move_into_block(&conn, 999_999, block_id).unwrap_err();
    assert!(
        err.to_string().contains("999999"),
        "error should name the missing event: {err}"
    );
}

#[test]
fn move_into_block_errors_on_unknown_block() {
    let conn = db::open_memory().unwrap();
    let event_id =
        seed_elsewhere_event(&conn, "far-sha", "2026-04-18T10:00:00+00:00", "aproorg/x");
    let err = move_into_block(&conn, event_id, 999_999).unwrap_err();
    assert!(
        err.to_string().contains("999999"),
        "error should name the missing block: {err}"
    );
}

#[test]
fn move_into_block_errors_when_event_is_not_elsewhere() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_block(
        &conn,
        "2026-04-18T09:00:00+00:00",
        "2026-04-18T09:30:00+00:00",
    );
    let event_id = repo::upsert_event(
        &conn,
        &Event::minimal(
            "github_commit",
            "local-sha",
            "2026-04-18T09:00:00+00:00",
            "x",
        ),
    )
    .unwrap();

    let err = move_into_block(&conn, event_id, block_id).unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("elsewhere"),
        "error should say the event isn't elsewhere: {err}"
    );
}

// ───────────────────────── FR-06 rebuild survival ─────────────────────────

/// A single point event only earns `CREDIT_MINUTES` (2m) of span for a
/// non-calendar source regardless of `duration_seconds` (see
/// `InferEvent::end`), so two events straddling the desired span are
/// needed to clear `MIN_BLOCK_MINUTES` and land on the exact 09:00–09:30
/// bounds this test asserts stay put.
fn insert_project_event(conn: &Connection, source_id: &str, started_at: &str, folder: &str) -> i64 {
    let mut ev = Event::minimal("shell", source_id, started_at, "work");
    ev.project_path = Some(folder.to_string());
    repo::upsert_event(conn, &ev).unwrap()
}

fn build_and_persist(conn: &Connection, day: NaiveDate) -> i64 {
    let blocks = crate::infer_allocations::build_day_blocks(conn, day).unwrap();
    crate::infer::persist_blocks(conn, day, &blocks).unwrap();
    conn.query_row("SELECT id FROM blocks WHERE day = '2026-04-18'", [], |r| {
        r.get(0)
    })
    .unwrap()
}

fn block_bounds(conn: &Connection, block_id: i64) -> (String, String, i64) {
    conn.query_row(
        "SELECT started_at, ended_at, duration_seconds FROM blocks WHERE id = ?1",
        [block_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap()
}

/// Moves an org commit into `block_id` the way the owner would by hand.
fn seed_moved_commit(conn: &Connection, block_id: i64) -> i64 {
    let moved_id = repo::upsert_event(
        conn,
        &Event::minimal(
            "github_commit",
            "far-sha",
            "2026-04-18T14:00:00+00:00",
            "fix oauth",
        ),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET elsewhere = 1 WHERE id = ?1",
        params![moved_id],
    )
    .unwrap();
    move_into_block(conn, moved_id, block_id).unwrap();
    moved_id
}

#[test]
fn move_into_block_survives_rebuild_and_relinks_by_folder() {
    // FR-06: an owner-moved event must never be undone by the next
    // re-infer — neither by voting/extending the block's bounds (it's
    // excluded from `load_day_events`) nor by falling out of the block
    // entirely (`persist_blocks` re-links it afterward).
    let conn = db::open_memory().unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let folder = "/Users/tomas/Desktop/Work/code-interpreter";
    insert_project_event(&conn, "a", "2026-04-18T09:00:00+00:00", folder);
    insert_project_event(&conn, "b", "2026-04-18T09:28:00+00:00", folder);

    let block_id = build_and_persist(&conn, day);
    let bounds_before = block_bounds(&conn, block_id);

    let moved_id = seed_moved_commit(&conn, block_id);
    let moved_folder: Option<String> = conn
        .query_row(
            "SELECT project_path FROM events WHERE id = ?1",
            [moved_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        moved_folder.as_deref(),
        Some(folder),
        "move_into_block must key the moved event to the block's dominant folder"
    );

    // Re-infer the day (the next collect/estimate cycle).
    let block_id2 = build_and_persist(&conn, day);
    let bounds_after = block_bounds(&conn, block_id2);
    assert_eq!(
        bounds_before, bounds_after,
        "the moved event must never extend or shift the block"
    );

    let linked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM block_events WHERE block_id = ?1 AND event_id = ?2",
            params![block_id2, moved_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        linked, 1,
        "the moved event must still be linked after a rebuild"
    );

    let elsewhere: i64 = conn
        .query_row(
            "SELECT elsewhere FROM events WHERE id = ?1",
            [moved_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        elsewhere, 2,
        "must stay owner-moved, never flip back to the elsewhere list"
    );
}
