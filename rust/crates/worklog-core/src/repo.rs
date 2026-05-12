//! Repository layer. Thin, typed queries over `rusqlite::Connection`.
//!
//! Invariant: every collector writes through `upsert_event`, dedupe keyed on
//! `(source, source_id)` — mirrors `db.upsert_event` in the Python codebase.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Block, Event, JiraTicket};

// ───────────────────────── events ─────────────────────────

/// Insert or update an event. Returns the event id.
///
/// Dedupe is enforced by the `UNIQUE(source, source_id)` constraint in
/// `schema.sql`; on conflict we update the mutable columns in place.
pub fn upsert_event(conn: &Connection, e: &Event) -> Result<i64> {
    conn.execute(
        "INSERT INTO events
            (source, source_id, started_at, ended_at, duration_seconds,
             title, details, repo, project_path, jira_issue, session_id,
             tempo_worklog_id, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(source, source_id) DO UPDATE SET
            started_at       = excluded.started_at,
            -- COALESCE: collectors don't populate these on re-collect,
            -- so a None on the new side must not wipe existing data.
            ended_at         = COALESCE(excluded.ended_at, events.ended_at),
            duration_seconds = COALESCE(excluded.duration_seconds, events.duration_seconds),
            title            = excluded.title,
            details          = excluded.details,
            repo             = excluded.repo,
            project_path     = excluded.project_path,
            jira_issue       = excluded.jira_issue,
            -- session_id: preserve existing when the new side doesn't
            -- carry one — matches Python and prevents e.g. a GitHub
            -- re-collect from wiping the claude session linkage.
            session_id       = COALESCE(events.session_id, excluded.session_id),
            -- tempo_worklog_id: the CLAUDE.md canary. Must NEVER be
            -- cleared. Collectors always pass None here, so unconditional
            -- overwrite would destroy the double-sync guard on every
            -- re-collect. COALESCE keeps the existing value.
            tempo_worklog_id = COALESCE(events.tempo_worklog_id, excluded.tempo_worklog_id),
            raw_json         = excluded.raw_json",
        params![
            e.source,
            e.source_id,
            e.started_at,
            e.ended_at,
            e.duration_seconds,
            e.title,
            e.details,
            e.repo,
            e.project_path,
            e.jira_issue,
            e.session_id,
            e.tempo_worklog_id,
            e.raw_json,
        ],
    )
    .context("upsert event")?;

    let id: i64 = conn
        .query_row(
            "SELECT id FROM events WHERE source = ?1 AND source_id = ?2",
            params![e.source, e.source_id],
            |r| r.get(0),
        )
        .context("fetching event id after upsert")?;
    Ok(id)
}

pub fn count_events(conn: &Connection) -> Result<i64> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
    Ok(n)
}

/// Events whose `started_at` falls on a given ISO-8601 day (UTC).
pub fn load_day_events(conn: &Connection, day: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, started_at, ended_at, duration_seconds,
                title, details, repo, project_path, jira_issue, session_id,
                tempo_worklog_id, raw_json
           FROM events
          WHERE substr(started_at, 1, 10) = ?1
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(Event {
            id: Some(r.get(0)?),
            source: r.get(1)?,
            source_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            title: r.get(6)?,
            details: r.get(7)?,
            repo: r.get(8)?,
            project_path: r.get(9)?,
            jira_issue: r.get(10)?,
            session_id: r.get(11)?,
            tempo_worklog_id: r.get(12)?,
            raw_json: r.get(13)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ───────────────────────── blocks ─────────────────────────

pub fn list_blocks_for_day(conn: &Connection, day: &str) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(
        "SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
                description, estimated_by, flagged, tempo_worklog_id, is_personal, dirty
           FROM blocks
          WHERE day = ?1
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(Block {
            id: r.get(0)?,
            day: r.get(1)?,
            jira_issue: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            description: r.get(6)?,
            estimated_by: r.get(7)?,
            flagged: r.get::<_, i64>(8)? != 0,
            tempo_worklog_id: r.get(9)?,
            is_personal: r.get::<_, i64>(10)? != 0,
            dirty: r.get::<_, i64>(11)? != 0,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Events linked to a specific block via `block_events`, ordered by
/// their own `started_at`. Used by the per-block events drill-down in
/// the web UI.
pub fn list_events_for_block(conn: &Connection, block_id: i64) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.source, e.source_id, e.started_at, e.ended_at,
                e.duration_seconds, e.title, e.details, e.repo,
                e.project_path, e.jira_issue, e.session_id,
                e.tempo_worklog_id, e.raw_json
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1
          ORDER BY e.started_at",
    )?;
    let rows = stmt.query_map(params![block_id], |r| {
        Ok(Event {
            id: Some(r.get(0)?),
            source: r.get(1)?,
            source_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            title: r.get(6)?,
            details: r.get(7)?,
            repo: r.get(8)?,
            project_path: r.get(9)?,
            jira_issue: r.get(10)?,
            session_id: r.get(11)?,
            tempo_worklog_id: r.get(12)?,
            raw_json: r.get(13)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_block(conn: &Connection, id: i64) -> Result<Option<Block>> {
    let block = conn
        .query_row(
            "SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
                    description, estimated_by, flagged, tempo_worklog_id, is_personal, dirty
               FROM blocks WHERE id = ?1",
            params![id],
            |r| {
                Ok(Block {
                    id: r.get(0)?,
                    day: r.get(1)?,
                    jira_issue: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    duration_seconds: r.get(5)?,
                    description: r.get(6)?,
                    estimated_by: r.get(7)?,
                    flagged: r.get::<_, i64>(8)? != 0,
                    tempo_worklog_id: r.get(9)?,
                    is_personal: r.get::<_, i64>(10)? != 0,
                    dirty: r.get::<_, i64>(11)? != 0,
                })
            },
        )
        .optional()?;
    Ok(block)
}

// ───────────────────────── jira tickets ─────────────────────────

pub fn upsert_ticket(conn: &Connection, t: &JiraTicket) -> Result<()> {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, project_key, updated, issue_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(key) DO UPDATE SET
            summary     = excluded.summary,
            status      = excluded.status,
            project_key = excluded.project_key,
            updated     = excluded.updated,
            -- Don't clobber a known issue_id with NULL from a partial
            -- refresh — only overwrite when the incoming row has one.
            issue_id    = COALESCE(excluded.issue_id, jira_tickets.issue_id),
            fetched_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            t.key,
            t.summary,
            t.status,
            t.project_key,
            t.updated,
            t.issue_id
        ],
    )
    .context("upsert jira ticket")?;
    Ok(())
}

pub fn list_tickets(conn: &Connection) -> Result<Vec<JiraTicket>> {
    let mut stmt = conn.prepare(
        "SELECT key, summary, status, project_key, updated, issue_id
           FROM jira_tickets
          ORDER BY updated DESC NULLS LAST, key",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(JiraTicket {
            key: r.get(0)?,
            summary: r.get(1)?,
            status: r.get(2)?,
            project_key: r.get(3)?,
            updated: r.get(4)?,
            issue_id: r.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Cached numeric issue_id for a key, if known. Tempo sync calls this
/// before paying the cost of a Jira REST roundtrip.
pub fn get_ticket_issue_id(conn: &Connection, key: &str) -> Result<Option<String>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT issue_id FROM jira_tickets WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(id)
}

/// Write back an issue_id resolved on the fly by the tempo sync.
/// Inserts the row if the ticket isn't cached yet — partial data is
/// better than no data, and the next `worklog collect jira` will fill
/// in the rest.
pub fn set_ticket_issue_id(conn: &Connection, key: &str, issue_id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id)
         VALUES (?1, '', ?2)
         ON CONFLICT(key) DO UPDATE SET issue_id = excluded.issue_id",
        params![key, issue_id],
    )
    .context("set_ticket_issue_id")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn fresh() -> Connection {
        open_memory().expect("in-memory db")
    }

    #[test]
    fn upsert_event_dedupes_on_source_pair() {
        let c = fresh();
        let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
        let id1 = upsert_event(&c, &e).unwrap();
        e.title = "second".into();
        let id2 = upsert_event(&c, &e).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(count_events(&c).unwrap(), 1);
        let title: String = c
            .query_row(
                "SELECT title FROM events WHERE id = ?1",
                params![id1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "second");
    }

    #[test]
    fn upsert_event_preserves_tempo_worklog_id_on_re_collect() {
        // CLAUDE.md canary: tempo_worklog_id MUST NEVER be cleared.
        // A re-collect pass (which passes tempo_worklog_id=None on the
        // Event struct because collectors don't know about sync state)
        // must not overwrite a previously-synced value. The ON CONFLICT
        // UPDATE must COALESCE, not unconditionally overwrite.
        let c = fresh();
        let mut e = Event::minimal("github_commit", "abc", "2026-04-18T09:00:00Z", "first");
        let id = upsert_event(&c, &e).unwrap();
        // Simulate the sync step having marked this event as synced.
        c.execute(
            "UPDATE events SET tempo_worklog_id = 'tw-42' WHERE id = ?1",
            params![id],
        )
        .unwrap();
        // Re-collect: the event is seen again, same source_id, no
        // tempo_worklog_id in the Event struct.
        assert!(e.tempo_worklog_id.is_none());
        e.title = "updated".into();
        upsert_event(&c, &e).unwrap();
        let stored: Option<String> = c
            .query_row(
                "SELECT tempo_worklog_id FROM events WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored.as_deref(),
            Some("tw-42"),
            "tempo_worklog_id canary must survive re-collect"
        );
    }

    #[test]
    fn upsert_event_preserves_session_id_when_new_is_none() {
        // Parity with Python: session_id uses COALESCE so an existing
        // non-null value is not clobbered by a collector that doesn't
        // know about it.
        let c = fresh();
        let e = Event::minimal("claude", "evt-1", "2026-04-18T09:00:00Z", "first");
        let id = upsert_event(&c, &e).unwrap();
        c.execute(
            "UPDATE events SET session_id = 'sess-a' WHERE id = ?1",
            params![id],
        )
        .unwrap();
        // Re-upsert with session_id = None on the struct.
        upsert_event(&c, &e).unwrap();
        let stored: Option<String> = c
            .query_row(
                "SELECT session_id FROM events WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), Some("sess-a"));
    }

    #[test]
    fn load_day_events_filters_by_iso_day() {
        let c = fresh();
        upsert_event(
            &c,
            &Event::minimal("gcal", "a", "2026-04-18T08:00:00Z", "A"),
        )
        .unwrap();
        upsert_event(
            &c,
            &Event::minimal("gcal", "b", "2026-04-18T23:59:59Z", "B"),
        )
        .unwrap();
        upsert_event(
            &c,
            &Event::minimal("gcal", "c", "2026-04-19T00:00:00Z", "C"),
        )
        .unwrap();

        let got = load_day_events(&c, "2026-04-18").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].title, "A");
        assert_eq!(got[1].title, "B");
    }

    #[test]
    fn list_blocks_for_week_returns_range_inclusive_ordered() {
        let c = fresh();
        // Mon 2026-04-13 .. Sun 2026-04-19. Insert one block per day in
        // shuffled order plus blocks just outside the range to confirm
        // the inclusive bounds.
        c.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds) VALUES
                ('2026-04-12','2026-04-12T10:00:00Z','2026-04-12T10:30:00Z',1800),
                ('2026-04-15','2026-04-15T11:00:00Z','2026-04-15T11:15:00Z', 900),
                ('2026-04-13','2026-04-13T09:00:00Z','2026-04-13T09:15:00Z', 900),
                ('2026-04-19','2026-04-19T20:00:00Z','2026-04-19T20:30:00Z',1800),
                ('2026-04-15','2026-04-15T08:00:00Z','2026-04-15T08:15:00Z', 900),
                ('2026-04-20','2026-04-20T09:00:00Z','2026-04-20T09:30:00Z',1800)",
            [],
        )
        .unwrap();
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();
        let blocks = list_blocks_for_week(&c, monday).unwrap();
        let days: Vec<&str> = blocks.iter().map(|b| b.day.as_str()).collect();
        // Mon-only block, then both Wednesday blocks ordered by start, then Sun.
        assert_eq!(
            days,
            vec!["2026-04-13", "2026-04-15", "2026-04-15", "2026-04-19"]
        );
        assert!(blocks[1].started_at < blocks[2].started_at);
    }

    #[test]
    fn list_blocks_for_day_orders_by_start() {
        let c = fresh();
        c.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18','2026-04-18T10:00:00Z','2026-04-18T10:30:00Z',1800),
                    ('2026-04-18','2026-04-18T09:00:00Z','2026-04-18T09:15:00Z', 900)",
            [],
        )
        .unwrap();
        let blocks = list_blocks_for_day(&c, "2026-04-18").unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].started_at < blocks[1].started_at);
    }

    #[test]
    fn get_block_returns_none_for_missing_id() {
        let c = fresh();
        assert!(get_block(&c, 9999).unwrap().is_none());
    }

    #[test]
    fn upsert_ticket_updates_summary_in_place() {
        let c = fresh();
        let t = JiraTicket {
            key: "PROJ-1".into(),
            summary: "first".into(),
            status: Some("Open".into()),
            project_key: Some("PROJ".into()),
            updated: Some("2026-04-17T10:00:00Z".into()),
            issue_id: None,
        };
        upsert_ticket(&c, &t).unwrap();
        let mut t2 = t.clone();
        t2.summary = "updated".into();
        upsert_ticket(&c, &t2).unwrap();

        let all = list_tickets(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].summary, "updated");
    }

    #[test]
    fn list_tickets_orders_by_updated_desc() {
        let c = fresh();
        for (k, u) in [
            ("A", "2026-04-17"),
            ("B", "2026-04-18"),
            ("C", "2026-04-16"),
        ] {
            upsert_ticket(
                &c,
                &JiraTicket {
                    key: k.into(),
                    summary: format!("ticket {k}"),
                    status: None,
                    project_key: None,
                    updated: Some(u.into()),
                    issue_id: None,
                },
            )
            .unwrap();
        }
        let keys: Vec<String> = list_tickets(&c)
            .unwrap()
            .into_iter()
            .map(|t| t.key)
            .collect();
        assert_eq!(keys, vec!["B", "A", "C"]);
    }
}
