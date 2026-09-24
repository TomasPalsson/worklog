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
