// Lifecycle riders (SessionStart/Stop/SessionEnd) must never outvote a
// block's real events in the billing folder vote. Split file for
// billing.rs's line budget — see billing_tenant_test.rs for the same
// convention.

use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::repo as repository;
use rusqlite::params;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

fn work(sub: &str) -> String {
    format!("{}/Desktop/Work/{sub}", home())
}

fn seed_block(conn: &Connection, started_at: &str, duration_seconds: i64) -> i64 {
    conn.execute(
        "INSERT INTO blocks
            (day, started_at, ended_at, duration_seconds, description, is_personal)
         VALUES ('2026-09-25', ?1, ?1, ?2, 'apro-skills work', 0)",
        params![started_at, duration_seconds],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(conn: &Connection, block_id: i64, source_id: &str, project_path: &str, title: &str) {
    let mut ev = Event::minimal("claude", source_id, "2026-09-25T10:36:00Z", title);
    ev.project_path = Some(project_path.to_string());
    let eid = repository::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();
}

#[test]
fn work_folder_for_block_skips_lifecycle_riders() {
    // Regression: a real apro-skills block outvoted 5-to-2 by
    // SessionStart/SessionEnd lifecycle riders from other sessions that
    // happened to cwd into a different folder must still bill under the
    // folder its real work happened in.
    let c = open_memory().unwrap();
    let b = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    seed_event(&c, b, "real1", &work("apro-skills"), "commit");
    seed_event(&c, b, "real2", &work("apro-skills"), "commit");
    for i in 0..5 {
        let title = if i % 2 == 0 {
            "SessionStart"
        } else {
            "SessionEnd"
        };
        seed_event(&c, b, &format!("rider{i}"), &work("worklog"), title);
    }
    assert_eq!(
        work_folder_for_block(&c, b).unwrap(),
        Some("apro-skills".into()),
        "lifecycle riders must never outvote a block's real events"
    );
}
