//! A block made only of calendar events is that meeting: its description
//! is the event title, never a model guess (the model never sees the
//! title and invented "Review and organize work items" for a meeting).

use anyhow::Result;
use rusqlite::{params, Connection};

/// The calendar title(s) of a block whose every linked event is a
/// calendar event, joined with "; ". `None` for any other block.
pub(crate) fn calendar_title(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.source, e.title
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1
          ORDER BY e.started_at",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![block_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    if rows.is_empty()
        || !rows
            .iter()
            .all(|(s, _)| crate::infer::is_calendar_source(s))
    {
        return Ok(None);
    }
    let mut titles: Vec<String> = rows.into_iter().map(|(_, t)| t).collect();
    titles.dedup();
    Ok(Some(titles.join("; ")))
}

/// Write the meeting title as the block's description and mark it
/// estimated, leaving its ticket alone. A hand-edited block is untouched.
pub(crate) fn describe(
    conn: &Connection,
    block_id: i64,
    title: &str,
    span_seconds: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE blocks
            SET description = ?1, estimated_by = 'claude_p', described_seconds = ?2
          WHERE id = ?3 AND COALESCE(estimated_by, '') <> 'manual'",
        params![title, span_seconds, block_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;

    fn block_with(conn: &Connection, events: &[(&str, &str)]) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-10-09', '2026-10-09T09:00:00+00:00', '2026-10-09T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        for (i, &(source, title)) in events.iter().enumerate() {
            let eid = repo::upsert_event(
                conn,
                &Event::minimal(
                    source,
                    format!("e{bid}-{i}"),
                    "2026-10-09T09:00:00+00:00",
                    title,
                ),
            )
            .unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }
        bid
    }

    #[test]
    fn calendar_only_block_is_named_after_the_meeting() {
        let conn = open_memory().unwrap();
        let bid = block_with(&conn, &[("gcal", "Öryggishugvekja")]);
        let title = calendar_title(&conn, bid).unwrap();
        assert_eq!(title.as_deref(), Some("Öryggishugvekja"));
        describe(&conn, bid, "Öryggishugvekja", 1800).unwrap();
        let (desc, by): (String, String) = conn
            .query_row(
                "SELECT description, estimated_by FROM blocks WHERE id = ?1",
                params![bid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (desc.as_str(), by.as_str()),
            ("Öryggishugvekja", "claude_p")
        );
    }

    #[test]
    fn mixed_or_empty_block_is_not_a_meeting() {
        let conn = open_memory().unwrap();
        let mixed = block_with(&conn, &[("gcal", "Standup"), ("claude", "PreToolUse")]);
        let empty = block_with(&conn, &[]);
        assert_eq!(calendar_title(&conn, mixed).unwrap(), None);
        assert_eq!(calendar_title(&conn, empty).unwrap(), None);
    }

    #[test]
    fn hand_edited_meeting_block_is_untouched() {
        let conn = open_memory().unwrap();
        let bid = block_with(&conn, &[("gcal", "Standup")]);
        conn.execute(
            "UPDATE blocks SET description = 'mine', estimated_by = 'manual' WHERE id = ?1",
            params![bid],
        )
        .unwrap();
        describe(&conn, bid, "Standup", 1800).unwrap();
        let desc: String = conn
            .query_row(
                "SELECT description FROM blocks WHERE id = ?1",
                params![bid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(desc, "mine");
    }
}
