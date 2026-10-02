use super::*;
use crate::db;
use rusqlite::params;

fn day() -> NaiveDate {
    "2026-09-28".parse().unwrap()
}

fn ticket(conn: &Connection, key: &str, updated: &str) {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, updated, fetched_at) VALUES (?1, 's', ?2, 'x')",
        params![key, updated],
    )
    .unwrap();
}

fn block(conn: &Connection, key: &str, d: &str, origin: Option<&str>, personal: i64) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, ticket_origin, is_personal)
         VALUES (?1, ?2, ?3, ?3, 60, ?4, ?5)",
        params![d, key, format!("{d}T09:00:00+00:00"), origin, personal],
    )
    .unwrap();
}

const RECENT: &str = "2026-09-18T12:00:00.000+0000";
const OLD: &str = "2026-08-19T12:00:00.000+0000";

#[test]
fn dead_status_table() {
    for s in [
        "Backlog",
        " backlog ",
        "Cancel",
        "Cancelled",
        "CANCELED",
        "Won't Do",
        "Wont fix",
        "Rejected",
        "Declined",
        "Duplicate",
        "Abandoned",
    ] {
        assert!(is_dead_status(Some(s)), "{s}");
    }
    for s in ["To Do", "In Progress", "Blocked", "Verification", "Done"] {
        assert!(!is_dead_status(Some(s)), "{s}");
    }
    assert!(!is_dead_status(None));
}

#[test]
fn active_keys_follow_jira_updates_and_hand_set_blocks() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "A-1", RECENT);
    ticket(&conn, "A-2", OLD);
    ticket(&conn, "A-3", OLD);
    block(&conn, "A-3", "2026-09-23", Some("manual"), 0);
    ticket(&conn, "A-4", OLD);
    block(&conn, "A-4", "2026-09-23", Some("auto"), 0);
    ticket(&conn, "A-5", OLD);
    block(&conn, "A-5", "2026-09-23", None, 0);
    ticket(&conn, "A-6", OLD);
    block(&conn, "A-6", "2026-08-19", Some("manual"), 0);
    ticket(&conn, "A-7", OLD);
    block(&conn, "A-7", "2026-09-23", Some("manual"), 1);
    ticket(&conn, "A-8", OLD);
    block(&conn, "A-8", "2026-09-23", Some("event"), 0);
    let active = active_keys(&conn, day()).unwrap();
    let mut keys: Vec<_> = active.iter().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["A-1", "A-3", "A-8"]);
}
