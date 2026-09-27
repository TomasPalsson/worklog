//! Database-level tests for saved splits: every rebuild path reads them,
//! and saved blocks keep their project and tickets.

use super::*;
use chrono::TimeZone;

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
}

fn shares(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(p, f)| (p.to_string(), *f)).collect()
}

const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const C: &str = "/Users/dev/Desktop/Work/lyfjastofnun";

/// Every path that rebuilds a day (daemon, `worklog infer`, `worklog
/// day` on the 15-min schedule) must honour a saved split — the CLI
/// used to rebuild without it and undo the owner's choice.
#[test]
fn day_rebuild_reads_the_saved_split() {
    use crate::models::Event;
    let conn = crate::db::open_memory().unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
    for i in 0..30u32 {
        let p = if i < 10 { A } else { C };
        let mut e = Event::minimal(
            "claude_turn",
            format!("t{i}"),
            at(9, i * 2).to_rfc3339(),
            "prompt",
        );
        e.project_path = Some(p.into());
        crate::repo::upsert_event(&conn, &e).unwrap();
    }
    let s = shares(&[("vitinn-infra", 1.0)]);
    crate::overlaps::save_allocation(&conn, day, at(9, 0), at(10, 0), &s).unwrap();

    let blocks = build_day_blocks(&conn, day).unwrap();
    // The project must survive saving: it is read back from the
    // block's linked events, not from anything held in memory.
    crate::infer::persist_blocks(&conn, day, &blocks).unwrap();
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM blocks WHERE day = ?1")
        .unwrap()
        .query_map([day.to_string()], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!ids.is_empty());
    for id in ids {
        let p = crate::personal::dominant_project_path_for_block(&conn, id).unwrap();
        assert_eq!(
            p.as_deref(),
            Some(A),
            "saved block {id} must read as vitinn-infra"
        );
    }
    assert!(blocks
        .iter()
        .all(|b| b.dominant_project_path().as_deref() == Some(A)));
    let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
    assert!(
        total >= 55 * 60,
        "the saved 100% split must hold, got {total}s"
    );
}

/// Saving a split and then resetting it must not lose the tickets the
/// owner put on blocks in that range.
#[test]
fn tickets_survive_a_split_and_its_reset() {
    use crate::models::Event;
    let conn = crate::db::open_memory().unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
    for i in 0..30u32 {
        let p = if i < 10 { A } else { C };
        let mut e = Event::minimal(
            "claude_turn",
            format!("t{i}"),
            at(9, i * 2).to_rfc3339(),
            "prompt",
        );
        e.project_path = Some(p.into());
        crate::repo::upsert_event(&conn, &e).unwrap();
    }
    let rebuild = || {
        let blocks = build_day_blocks(&conn, day).unwrap();
        crate::infer::persist_blocks(&conn, day, &blocks).unwrap();
    };
    let tickets = || -> Vec<String> {
        let mut t: Vec<String> = conn
            .prepare("SELECT jira_issue FROM blocks WHERE day = ?1 AND jira_issue IS NOT NULL")
            .unwrap()
            .query_map([day.to_string()], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        t.sort();
        t
    };
    rebuild();
    conn.execute(
        "UPDATE blocks SET jira_issue = 'T-A' WHERE started_at LIKE '%T09:00%'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE blocks SET jira_issue = 'T-C' WHERE started_at LIKE '%T09:20%'",
        [],
    )
    .unwrap();
    assert_eq!(tickets(), ["T-A", "T-C"]);

    let s = shares(&[("lyfjastofnun", 0.5), ("vitinn-infra", 0.5)]);
    crate::overlaps::save_allocation(&conn, day, at(9, 0), at(10, 0), &s).unwrap();
    rebuild();
    assert_eq!(
        tickets(),
        ["T-A", "T-C"],
        "a split keeps the range's tickets"
    );

    crate::overlaps::delete_allocation(&conn, day, at(9, 0), at(10, 0)).unwrap();
    rebuild();
    assert_eq!(tickets(), ["T-A", "T-C"], "a reset keeps them too");
}

const SHARED: &str = "/Users/dev/Desktop/Work/vitinn-infra";

/// Six alternating ~20-min stretches over two hours: session A owns the
/// even stretches, session B the odd ones, both in the same shared folder.
fn seed_two_session_stretches(conn: &rusqlite::Connection, title_a: &str, title_b: &str) {
    use crate::models::Event;
    for stretch in 0..6u32 {
        let (session, title) = if stretch % 2 == 0 {
            ("sessA", title_a)
        } else {
            ("sessB", title_b)
        };
        for step in 0..10u32 {
            let minute = stretch * 20 + step * 2;
            let mut e = Event::minimal(
                "claude_turn",
                format!("t{stretch}-{step}"),
                at(9 + minute / 60, minute % 60).to_rfc3339(),
                title,
            );
            e.project_path = Some(SHARED.into());
            e.session_id = Some(session.into());
            crate::repo::upsert_event(conn, &e).unwrap();
        }
    }
}

/// A folder shared by two customers' sessions must yield separate blocks
/// per customer, not one lane that mixes both (session lanes, FR-05).
#[test]
fn two_customer_sessions_split_into_separate_blocks() {
    use crate::billing_registry::{upsert_customer, Customer};
    let conn = crate::db::open_memory().unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "APRÓ".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    seed_two_session_stretches(&conn, "Sjúkra onboarding call", "APRÓ migration work");

    let blocks = build_day_blocks(&conn, day).unwrap();
    let non_calendar: Vec<&InferBlock> = blocks.iter().filter(|b| !b.is_calendar).collect();
    assert!(
        non_calendar.len() >= 2,
        "expected at least 2 blocks, got {}",
        non_calendar.len()
    );
    for b in &non_calendar {
        let sessions: BTreeMap<&str, ()> = b
            .events
            .iter()
            .filter_map(|e| e.session_id.as_deref())
            .map(|s| (s, ()))
            .collect();
        assert!(
            sessions.len() <= 1,
            "block must not mix both sessions, got {sessions:?}"
        );
        assert!(
            b.events
                .iter()
                .all(|e| e.project_path.as_deref() == Some(SHARED)),
            "every event must stay under vitinn-infra"
        );
    }

    crate::infer::persist_blocks(&conn, day, &blocks).unwrap();
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM blocks WHERE day = ?1")
        .unwrap()
        .query_map([day.to_string()], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!ids.is_empty());
    for id in ids {
        let p = crate::personal::dominant_project_path_for_block(&conn, id).unwrap();
        assert_eq!(
            p.as_deref(),
            Some(SHARED),
            "saved block {id} must read as vitinn-infra"
        );
    }
}

/// On real data every `claude_turn` is titled "prompt"; the owner's message
/// lives in `raw_json` (`RawRecord::ClaudePrompt`). Session lanes must
/// resolve the customer from there, not from the (uniform) DB title.
#[test]
fn prompt_text_names_the_session_customer() {
    use crate::billing_registry::{upsert_customer, Customer};
    use crate::clues_contract::RawRecord;
    use crate::models::Event;
    let conn = crate::db::open_memory().unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "APRÓ".into(),
            aliases: vec![],
        },
    )
    .unwrap();

    for stretch in 0..6u32 {
        let (session, text) = if stretch % 2 == 0 {
            ("sessA", "Sjúkra onboarding call")
        } else {
            ("sessB", "APRÓ migration work")
        };
        for step in 0..10u32 {
            let minute = stretch * 20 + step * 2;
            let mut e = Event::minimal(
                "claude_turn",
                format!("t{stretch}-{step}"),
                at(9 + minute / 60, minute % 60).to_rfc3339(),
                "prompt",
            );
            e.project_path = Some(SHARED.into());
            e.session_id = Some(session.into());
            e.raw_json = Some(
                serde_json::to_string(&RawRecord::ClaudePrompt {
                    session_id: session.into(),
                    text: text.into(),
                })
                .unwrap(),
            );
            crate::repo::upsert_event(&conn, &e).unwrap();
        }
    }

    let blocks = build_day_blocks(&conn, day).unwrap();
    let non_calendar: Vec<&InferBlock> = blocks.iter().filter(|b| !b.is_calendar).collect();
    assert!(
        non_calendar.len() >= 2,
        "expected at least 2 blocks, got {}",
        non_calendar.len()
    );
    for b in &non_calendar {
        let sessions: BTreeMap<&str, ()> = b
            .events
            .iter()
            .filter_map(|e| e.session_id.as_deref())
            .map(|s| (s, ()))
            .collect();
        assert!(
            sessions.len() <= 1,
            "block must not mix both sessions, got {sessions:?}"
        );
    }
}

/// Both sessions naming the same customer must not split the lane —
/// the registry then behaves exactly as if it held no customers (FR-04).
#[test]
fn two_sessions_same_customer_matches_no_customer_registry() {
    use crate::billing_registry::{upsert_customer, Customer};
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();

    let with_customer = crate::db::open_memory().unwrap();
    upsert_customer(
        &with_customer,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    seed_two_session_stretches(&with_customer, "Sjúkra onboarding call", "Sjúkra followup");
    let tagged_blocks = build_day_blocks(&with_customer, day).unwrap();

    let without_customer = crate::db::open_memory().unwrap();
    seed_two_session_stretches(
        &without_customer,
        "Sjúkra onboarding call",
        "Sjúkra followup",
    );
    let plain_blocks = build_day_blocks(&without_customer, day).unwrap();

    let shape = |blocks: &[InferBlock]| -> Vec<(DateTime<Utc>, DateTime<Utc>, u32)> {
        blocks
            .iter()
            .map(|b| (b.started_at, b.ended_at, b.event_count))
            .collect()
    };
    assert_eq!(
        shape(&tagged_blocks),
        shape(&plain_blocks),
        "one resolved customer must not split the folder's lane"
    );
}
