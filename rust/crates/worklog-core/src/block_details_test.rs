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

/// 2026-09-28 VÍS block 5935: a genai-infra session's SessionStart ping
/// rode into the block (R3 lifecycle rider), and its 69 in-span RU-chatbot
/// tool calls followed it into the description writer's evidence.
#[test]
fn block_details_excludes_tool_rows_of_a_lifecycle_rider_session() {
    let conn = db::open_memory().unwrap();
    let bid = seed_block(&conn, START, END);
    let prompt = seed_event(&conn, "claude_prompt", "p", "2026-04-18T09:05:00+00:00");
    set_session(&conn, prompt, "s1");
    link(&conn, bid, prompt);

    let rider = repo::upsert_event(
        &conn,
        &Event::minimal(
            "claude",
            "start",
            "2026-04-18T09:06:00+00:00",
            "SessionStart",
        ),
    )
    .unwrap();
    set_session(&conn, rider, "s2");
    link(&conn, bid, rider);

    let own_tool = seed_event(
        &conn,
        SOURCE_CLAUDE_TOOL,
        "s1:1",
        "2026-04-18T09:10:00+00:00",
    );
    set_session(&conn, own_tool, "s1");
    set_raw(&conn, own_tool, &tool_raw("s1"));
    let rider_tool = seed_event(
        &conn,
        SOURCE_CLAUDE_TOOL,
        "s2:1",
        "2026-04-18T09:12:00+00:00",
    );
    set_session(&conn, rider_tool, "s2");
    set_raw(&conn, rider_tool, &tool_raw("s2"));

    let ids: Vec<i64> = details_for_block(&conn, bid)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    assert!(ids.contains(&own_tool), "{ids:?}");
    assert!(
        !ids.contains(&rider_tool),
        "rider session's tools leaked: {ids:?}"
    );
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

#[test]
fn block_details_includes_offset_shifted_row_needing_widened_day_bounds() {
    let conn = db::open_memory().unwrap();
    let bid = seed_block(&conn, START, END);
    let prompt = seed_event(&conn, "claude_prompt", "p", "2026-04-18T09:05:00+00:00");
    set_session(&conn, prompt, "s1");
    link(&conn, bid, prompt);

    // Written with a +15:00 offset, so its leading date is 2026-04-19 — one day after the
    // block's day — but its UTC instant, 09:15:00Z, is inside the span. Only the widened SQL
    // day pre-filter (widen_day_bounds) keeps this row; an exact-day filter would drop it.
    let tool = seed_event(
        &conn,
        SOURCE_CLAUDE_TOOL,
        "s1:1",
        "2026-04-19T00:15:00+15:00",
    );
    set_session(&conn, tool, "s1");
    set_raw(&conn, tool, &tool_raw("s1"));

    let rows = details_for_block(&conn, bid).unwrap();
    assert!(rows.iter().any(|r| r.id == tool));
}
