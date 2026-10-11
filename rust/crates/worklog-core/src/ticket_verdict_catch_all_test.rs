use super::*;
use crate::models::{Event, JiraTicket};

fn catch_all(conn: &Connection, key: &str) {
    repo::upsert_external_ticket(
        conn,
        &JiraTicket {
            key: key.into(),
            summary: "[T] Námskeið og ráðstefnur".into(),
            status: Some("In Progress".into()),
            project_key: Some("APRO".into()),
            updated: Some("2026-04-19T00:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
}

#[test]
fn catch_all_tickets_are_offered_after_every_other_source() {
    let conn = crate::db::open_memory().unwrap();
    catch_all(&conn, "APRO-13");
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-20', '2026-04-20T10:00:00+00:00', '2026-04-20T10:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    let b = conn.last_insert_rowid();
    let mut e = Event::minimal(
        "firefox",
        "f1".to_string(),
        "2026-04-20T10:00:00+00:00".to_string(),
        "AWS Certified Solutions Architect Associate Practice Exams",
    );
    e.session_id = Some("s1".into());
    let id = repo::upsert_event(&conn, &e).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![b, id],
    )
    .unwrap();
    let block = repo::get_block(&conn, b).unwrap().unwrap();
    // A browser-only block has no session, written or recent ticket.
    assert_eq!(ticket_options(&conn, &block).unwrap(), ["APRO-13"]);
    // Real signals still come first.
    session_tickets::set(&conn, "s1", "VIT-1").unwrap();
    assert_eq!(ticket_options(&conn, &block).unwrap(), ["VIT-1", "APRO-13"]);
}
