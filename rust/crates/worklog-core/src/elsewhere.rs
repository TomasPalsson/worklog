//! Per-day "done elsewhere" list — org commits/PRs whose sha is absent
//! from every local clone (`events.elsewhere = 1`, D-07) — and moving one
//! into a chosen block by hand (FR-05, FR-06). Populated by T006.

use anyhow::{bail, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::personal;

/// One row of the per-day "done elsewhere" list (FR-05).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ElsewhereItem {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub title: String,
    pub repo: Option<String>,
}

/// Events flagged `elsewhere = 1` whose local-TZ day (`tz::local_date`,
/// the same bucketing blocks use) is `day`, time-ordered.
pub fn list_for_day(conn: &Connection, day: NaiveDate) -> Result<Vec<ElsewhereItem>> {
    // started_at is a fixed-width ISO-8601 string; lexicographic comparison
    // works once the `+00:00` suffix is trimmed the same way on both sides
    // (see infer::load_day_events's `iso_prefix`).
    let (start_utc, end_utc) = crate::tz::utc_window_for_local_day(day);
    let start = start_utc.to_rfc3339().trim_end_matches("+00:00").to_owned();
    let end = end_utc.to_rfc3339().trim_end_matches("+00:00").to_owned();
    let mut stmt = conn.prepare(
        "SELECT id, source, started_at, title, repo
           FROM events
          WHERE elsewhere = 1 AND started_at >= ?1 AND started_at < ?2
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![start, end], |r| {
        Ok(ElsewhereItem {
            id: r.get(0)?,
            source: r.get(1)?,
            started_at: r.get(2)?,
            title: r.get(3)?,
            repo: r.get(4)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

/// Move an elsewhere-flagged event into a chosen block by hand (FR-06):
/// keys it to the block's dominant project so a re-infer keeps it put
/// instead of sending it back to "done elsewhere".
pub fn move_into_block(conn: &Connection, event_id: i64, block_id: i64) -> Result<()> {
    let elsewhere: Option<i64> = conn
        .query_row(
            "SELECT elsewhere FROM events WHERE id = ?1",
            params![event_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(elsewhere) = elsewhere else {
        bail!("event {event_id} not found");
    };
    if elsewhere == 0 {
        bail!("event {event_id} is not elsewhere");
    }
    let block_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM blocks WHERE id = ?1)",
        params![block_id],
        |r| r.get(0),
    )?;
    if !block_exists {
        bail!("block {block_id} not found");
    }

    let dominant = personal::dominant_project_path_for_block(conn, block_id)?;
    conn.execute(
        "UPDATE events SET project_path = ?1, elsewhere = 0 WHERE id = ?2",
        params![dominant, event_id],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
        assert_eq!(elsewhere, 0, "must leave the elsewhere list");

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
}
