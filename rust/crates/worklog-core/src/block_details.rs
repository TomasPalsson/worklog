//! The Details view's backing query: every event of a block, including
//! helper/message activity in its span, in time order (spec 006, FR-20).
//! Populated by T015: `details_for_block`.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use std::collections::HashSet;

use crate::clues_contract::{
    RawRecord, SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_MESSAGE, SOURCE_CLAUDE_TOOL,
};
use crate::models::Event;
use crate::repo;

/// One row of the Details view timeline (design §2, §4).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetailRow {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub title: String,
    pub details: Option<String>,
    pub repo: Option<String>,
    pub project_path: Option<String>,
    pub session_id: Option<String>,
    pub jira_issue: Option<String>,
    pub raw: Option<RawRecord>,
}

/// Every event linked to `block_id` via `block_events`, plus every
/// claude_tool/claude_helper/claude_message event whose session_id
/// matches a linked event's session and whose timestamp falls inside
/// the block's `[started_at, ended_at]` window — time-ordered, no
/// duplicates (FR-20).
pub fn details_for_block(conn: &Connection, block_id: i64) -> Result<Vec<DetailRow>> {
    let block =
        repo::get_block(conn, block_id)?.ok_or_else(|| anyhow!("block {block_id} not found"))?;

    let linked = repo::list_events_for_block(conn, block_id)?;
    let linked_ids: HashSet<i64> = linked.iter().filter_map(|e| e.id).collect();
    let session_ids: Vec<String> = linked
        .iter()
        .filter_map(|e| e.session_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut events = linked;
    if !session_ids.is_empty() {
        let span_start = parse_ts(&block.started_at);
        let span_end = parse_ts(&block.ended_at);
        let candidates = helper_activity_for_sessions(conn, &session_ids)?;
        events.extend(candidates.into_iter().filter(|e| {
            if e.id.is_some_and(|id| linked_ids.contains(&id)) {
                return false;
            }
            match (parse_ts(&e.started_at), span_start, span_end) {
                (Some(ts), Some(s), Some(en)) => ts >= s && ts <= en,
                _ => false,
            }
        }));
    }

    let mut rows: Vec<(DateTime<Utc>, i64, DetailRow)> = events
        .into_iter()
        .map(|e| {
            let ts = parse_ts(&e.started_at).unwrap_or(DateTime::<Utc>::MIN_UTC);
            let id = e.id.unwrap_or_default();
            (ts, id, to_detail_row(e))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(rows.into_iter().map(|(_, _, row)| row).collect())
}

fn to_detail_row(e: Event) -> DetailRow {
    let raw = e
        .raw_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<RawRecord>(s).ok());
    DetailRow {
        id: e.id.unwrap_or_default(),
        source: e.source,
        started_at: e.started_at,
        title: e.title,
        details: e.details,
        repo: e.repo,
        project_path: e.project_path,
        session_id: e.session_id,
        jira_issue: e.jira_issue,
        raw,
    }
}

/// `claude_tool`/`claude_helper`/`claude_message` events for any of
/// `session_ids` — one query regardless of how many sessions the block
/// touches. Span filtering happens in Rust (`parse_ts`) because blocks
/// and events store ISO strings in slightly different formats.
fn helper_activity_for_sessions(conn: &Connection, session_ids: &[String]) -> Result<Vec<Event>> {
    let placeholders = vec!["?"; session_ids.len()].join(",");
    let sql = format!(
        "SELECT id, source, source_id, started_at, ended_at,
                duration_seconds, title, details, repo,
                project_path, jira_issue, session_id,
                tempo_worklog_id, raw_json
           FROM events
          WHERE source IN (?, ?, ?)
            AND session_id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params = [
        SOURCE_CLAUDE_TOOL,
        SOURCE_CLAUDE_HELPER,
        SOURCE_CLAUDE_MESSAGE,
    ]
    .into_iter()
    .chain(session_ids.iter().map(String::as_str));
    let rows = stmt.query_map(params_from_iter(params), |r| {
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
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

/// Accepts the ISO-8601 variants this codebase writes (`+00:00` offset,
/// `Z`, or a naive string treated as UTC) — mirrors `infer::parse_maybe_utc`.
fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(naive.and_utc());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::params;

    const START: &str = "2026-04-18T09:00:00+00:00";
    const END: &str = "2026-04-18T09:30:00+00:00";

    fn seed_block(conn: &Connection, start: &str, end: &str) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', ?1, ?2, 1800)",
            params![start, end],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_event(conn: &Connection, source: &str, source_id: &str, started_at: &str) -> i64 {
        repo::upsert_event(
            conn,
            &Event::minimal(source, source_id, started_at, "title"),
        )
        .unwrap()
    }

    fn set_raw(conn: &Connection, event_id: i64, raw: &str) {
        conn.execute(
            "UPDATE events SET raw_json = ?1 WHERE id = ?2",
            params![raw, event_id],
        )
        .unwrap();
    }

    fn set_session(conn: &Connection, event_id: i64, session_id: &str) {
        conn.execute(
            "UPDATE events SET session_id = ?1 WHERE id = ?2",
            params![session_id, event_id],
        )
        .unwrap();
    }

    fn link(conn: &Connection, block_id: i64, event_id: i64) {
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, event_id],
        )
        .unwrap();
    }

    fn tool_raw(session_id: &str) -> String {
        serde_json::to_string(&RawRecord::ClaudeTool {
            session_id: session_id.to_string(),
            tool: "Bash".to_string(),
            input: serde_json::json!({}),
            output: None,
            output_cut_bytes: 0,
            files: vec![],
        })
        .unwrap()
    }

    #[test]
    fn block_details_unknown_block_errors() {
        let conn = db::open_memory().unwrap();
        assert!(details_for_block(&conn, 999).is_err());
    }

    #[test]
    fn block_details_time_orders_linked_events_with_parsed_raw() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(&conn, START, END);
        // Inserted out of time order to prove the result is sorted, not
        // insertion-ordered.
        let e_later = seed_event(&conn, "shell", "a", "2026-04-18T09:10:00+00:00");
        set_raw(
            &conn,
            e_later,
            &serde_json::to_string(&RawRecord::Shell {
                command: "ls".to_string(),
                cwd: None,
            })
            .unwrap(),
        );
        let e_earlier = seed_event(&conn, "claude_prompt", "b", "2026-04-18T09:05:00+00:00");
        set_raw(
            &conn,
            e_earlier,
            &serde_json::to_string(&RawRecord::ClaudePrompt {
                session_id: "s1".to_string(),
                text: "fix bug".to_string(),
            })
            .unwrap(),
        );
        link(&conn, bid, e_later);
        link(&conn, bid, e_earlier);

        let rows = details_for_block(&conn, bid).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, e_earlier);
        assert_eq!(rows[1].id, e_later);
        assert_eq!(
            rows[0].raw,
            Some(RawRecord::ClaudePrompt {
                session_id: "s1".to_string(),
                text: "fix bug".to_string(),
            })
        );
        assert_eq!(
            rows[1].raw,
            Some(RawRecord::Shell {
                command: "ls".to_string(),
                cwd: None,
            })
        );
    }

    #[test]
    fn block_details_includes_claude_tool_row_of_linked_session_inside_span() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(&conn, START, END);
        let prompt = seed_event(&conn, "claude_prompt", "p", "2026-04-18T09:05:00+00:00");
        set_session(&conn, prompt, "s1");
        link(&conn, bid, prompt);

        let tool = seed_event(
            &conn,
            SOURCE_CLAUDE_TOOL,
            "s1:1",
            "2026-04-18T09:12:00+00:00",
        );
        set_session(&conn, tool, "s1");
        set_raw(&conn, tool, &tool_raw("s1"));

        let rows = details_for_block(&conn, bid).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.id == tool));
    }

    #[test]
    fn block_details_excludes_claude_tool_row_outside_span() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(&conn, START, END);
        let prompt = seed_event(&conn, "claude_prompt", "p", "2026-04-18T09:05:00+00:00");
        set_session(&conn, prompt, "s1");
        link(&conn, bid, prompt);

        // Outside [09:00, 09:30].
        let tool = seed_event(
            &conn,
            SOURCE_CLAUDE_TOOL,
            "s1:1",
            "2026-04-18T10:00:00+00:00",
        );
        set_session(&conn, tool, "s1");
        set_raw(&conn, tool, &tool_raw("s1"));

        let rows = details_for_block(&conn, bid).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows.iter().all(|r| r.id != tool));
    }

    #[test]
    fn block_details_excludes_claude_tool_row_from_unrelated_session() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(&conn, START, END);
        let prompt = seed_event(&conn, "claude_prompt", "p", "2026-04-18T09:05:00+00:00");
        set_session(&conn, prompt, "s1");
        link(&conn, bid, prompt);

        // Inside the span, but a different session than any linked event.
        let tool = seed_event(
            &conn,
            SOURCE_CLAUDE_TOOL,
            "s2:1",
            "2026-04-18T09:12:00+00:00",
        );
        set_session(&conn, tool, "s2");
        set_raw(&conn, tool, &tool_raw("s2"));

        let rows = details_for_block(&conn, bid).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows.iter().all(|r| r.id != tool));
    }

    #[test]
    fn block_details_malformed_raw_json_yields_none() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(&conn, START, END);
        let e = seed_event(&conn, "shell", "a", "2026-04-18T09:10:00+00:00");
        set_raw(&conn, e, "{not json");
        link(&conn, bid, e);

        let rows = details_for_block(&conn, bid).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].raw, None);
    }

    #[test]
    fn block_details_thousand_events_under_one_second() {
        let conn = db::open_memory().unwrap();
        let bid = seed_block(
            &conn,
            "2026-04-18T00:00:00+00:00",
            "2026-04-18T23:59:59+00:00",
        );
        for i in 0..1000 {
            let started_at = format!("2026-04-18T{:02}:{:02}:00+00:00", i / 60 % 24, i % 60);
            let e = seed_event(&conn, SOURCE_CLAUDE_TOOL, &format!("s1:{i}"), &started_at);
            set_session(&conn, e, "s1");
            set_raw(&conn, e, &tool_raw("s1"));
            link(&conn, bid, e);
        }

        let start = std::time::Instant::now();
        let rows = details_for_block(&conn, bid).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(rows.len(), 1000);
        assert!(
            elapsed.as_secs_f64() < 1.0,
            "details_for_block took {elapsed:?}, want < 1s"
        );
        assert!(rows.windows(2).all(|w| w[0].started_at <= w[1].started_at));
        assert!(matches!(rows[0].raw, Some(RawRecord::ClaudeTool { .. })));
    }
}
