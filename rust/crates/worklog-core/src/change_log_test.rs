use super::*;
use crate::billing_deildir::upsert_deild;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::deild_contract::{BlockShares, Deild};
use crate::models::Event;
use crate::repo;
use crate::tenant_shares::save_rows;

const DAY: &str = "2026-09-24";

fn seed_block(
    conn: &Connection,
    started_at: &str,
    duration_seconds: i64,
    description: Option<&str>,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
         VALUES (?1, ?2, ?2, ?3, ?4)",
        params![DAY, started_at, duration_seconds, description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(conn: &Connection, block_id: i64, source_id: &str, project_path: &str) {
    let mut ev = Event::minimal("claude", source_id, "2026-09-24T09:00:00Z", "work");
    ev.project_path = Some(project_path.to_string());
    let eid = repo::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();
}

fn pin(conn: &Connection, folder: &str, customer: Option<&str>, verkefni: Option<&str>) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: customer.map(str::to_owned),
            verkefni: verkefni.map(str::to_owned),
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
}

fn customer(conn: &Connection, name: &str) {
    upsert_customer(
        conn,
        &Customer {
            id: None,
            name: name.into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();
}

fn snapshot_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM block_resolution_snapshots", [], |r| {
        r.get(0)
    })
    .unwrap()
}

#[test]
fn new_block_logs_nothing() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), None);
    let b1 = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("desc one"));
    seed_event(&conn, b1, "e1", "/tmp/acme");

    // A day's first refresh stores a snapshot and logs nothing.
    assert_eq!(
        refresh_day(&conn, DAY, ChangeSource::Rebuild, "b1").unwrap(),
        0
    );
    assert_eq!(snapshot_count(&conn), 1);

    // A rebuild that only adds a block logs nothing.
    let b2 = seed_block(&conn, "2026-09-24T11:00:00Z", 1800, Some("desc two"));
    seed_event(&conn, b2, "e2", "/tmp/acme");
    assert_eq!(
        refresh_day(&conn, DAY, ChangeSource::Rebuild, "b2").unwrap(),
        0
    );
    assert_eq!(snapshot_count(&conn), 2);

    // A deleted block's snapshot is removed, nothing logged.
    conn.execute("DELETE FROM block_events WHERE block_id = ?1", params![b2])
        .unwrap();
    conn.execute("DELETE FROM blocks WHERE id = ?1", params![b2])
        .unwrap();
    assert_eq!(
        refresh_day(&conn, DAY, ChangeSource::Rebuild, "b3").unwrap(),
        0
    );
    assert_eq!(snapshot_count(&conn), 1);

    // A duration change alone logs nothing.
    conn.execute(
        "UPDATE blocks SET duration_seconds = 7200 WHERE id = ?1",
        params![b1],
    )
    .unwrap();
    assert_eq!(
        refresh_day(&conn, DAY, ChangeSource::Rebuild, "b4").unwrap(),
        0
    );
    assert!(feed(&conn, 0).unwrap().changes.is_empty());
}

#[test]
fn customer_change_is_logged() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), None);
    let b = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("desc"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Rebuild, "b1").unwrap();

    // A Verdict re-route moves the folder's pinned customer.
    pin(&conn, "acme", Some("APRÓ"), None);
    let logged = refresh_day(&conn, DAY, ChangeSource::Verdict, "b2").unwrap();
    assert_eq!(logged, 1);

    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Customer);
    assert_eq!(changes[0].old.as_deref(), Some("Sjúkra 100%"));
    assert_eq!(changes[0].new.as_deref(), Some("APRÓ 100%"));
    assert_eq!(changes[0].source, ChangeSource::Verdict);
}

#[test]
fn a_block_with_no_customer_reads_unresolved() {
    let conn = open_memory().unwrap();
    let b = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("desc"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Rebuild, "b1").unwrap();

    pin(&conn, "acme", Some("Sjúkra"), None);
    refresh_day(&conn, DAY, ChangeSource::Keyword, "b2").unwrap();

    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes[0].old.as_deref(), Some("Unresolved 100%"));
    assert_eq!(changes[0].new.as_deref(), Some("Sjúkra 100%"));
}

#[test]
fn deild_change_is_logged() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), Some("Rekstur"));
    let b = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("ops work"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Rebuild, "b1").unwrap();

    // A keyword match beats the folder default deild.
    upsert_deild(
        &conn,
        &Deild {
            id: None,
            customer: "Sjúkra".into(),
            name: "Vöktun".into(),
            keywords: vec!["ops".into()],
        },
    )
    .unwrap();
    let logged = refresh_day(&conn, DAY, ChangeSource::Keyword, "b2").unwrap();
    assert_eq!(logged, 1);

    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Deild);
    assert_eq!(changes[0].old.as_deref(), Some("Sjúkra·Rekstur 100%"));
    assert_eq!(changes[0].new.as_deref(), Some("Sjúkra·Vöktun 100%"));
}

#[test]
fn split_change_is_logged() {
    let conn = open_memory().unwrap();
    customer(&conn, "Sjúkra");
    customer(&conn, "APRÓ");
    let started_at = "2026-09-24T09:00:00Z";
    let b = seed_block(&conn, started_at, 3600, Some("desc"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    let registry = Registry::load(&conn).unwrap();

    save_rows(
        &conn,
        &BlockShares {
            day: DAY.into(),
            started_at: started_at.into(),
            rows: vec![
                ShareRow {
                    customer: "Sjúkra".into(),
                    deild: None,
                    fraction: 0.6,
                },
                ShareRow {
                    customer: "APRÓ".into(),
                    deild: None,
                    fraction: 0.4,
                },
            ],
        },
        &registry,
    )
    .unwrap();
    refresh_day(&conn, DAY, ChangeSource::User, "b1").unwrap();

    // The Owner re-saves the split with different shares.
    save_rows(
        &conn,
        &BlockShares {
            day: DAY.into(),
            started_at: started_at.into(),
            rows: vec![
                ShareRow {
                    customer: "Sjúkra".into(),
                    deild: None,
                    fraction: 0.5,
                },
                ShareRow {
                    customer: "APRÓ".into(),
                    deild: None,
                    fraction: 0.5,
                },
            ],
        },
        &registry,
    )
    .unwrap();
    let logged = refresh_day(&conn, DAY, ChangeSource::User, "b2").unwrap();
    assert_eq!(logged, 1);

    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Split);
    assert_eq!(changes[0].source, ChangeSource::User);
}

#[test]
fn description_change_is_logged() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), None);
    let b = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("first text"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Claude, "b1").unwrap();

    conn.execute(
        "UPDATE blocks SET description = ?1 WHERE id = ?2",
        params!["second text", b],
    )
    .unwrap();
    let logged = refresh_day(&conn, DAY, ChangeSource::Claude, "b2").unwrap();
    assert_eq!(logged, 1);

    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Description);
    assert_eq!(changes[0].old.as_deref(), Some("first text"));
    assert_eq!(changes[0].new.as_deref(), Some("second text"));
}

/// Journey 4, edge path: a hand-set split survives a description rewrite
/// (FR-08); only the description change is logged.
#[test]
fn manual_split_is_untouched_by_a_description_change() {
    let conn = open_memory().unwrap();
    customer(&conn, "Sjúkra");
    let started_at = "2026-09-24T09:00:00Z";
    let b = seed_block(&conn, started_at, 3600, Some("first text"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    let registry = Registry::load(&conn).unwrap();
    save_rows(
        &conn,
        &BlockShares {
            day: DAY.into(),
            started_at: started_at.into(),
            rows: vec![ShareRow {
                customer: "Sjúkra".into(),
                deild: None,
                fraction: 1.0,
            }],
        },
        &registry,
    )
    .unwrap();
    refresh_day(&conn, DAY, ChangeSource::User, "b1").unwrap();

    conn.execute(
        "UPDATE blocks SET description = ?1 WHERE id = ?2",
        params!["second text", b],
    )
    .unwrap();
    let logged = refresh_day(&conn, DAY, ChangeSource::Claude, "b2").unwrap();
    assert_eq!(logged, 1);
    let changes = feed(&conn, 0).unwrap().changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, ChangeField::Description);
}

#[test]
fn feed_groups_changes_into_one_batch_per_run() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), None);
    let b1 = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("d1"));
    seed_event(&conn, b1, "e1", "/tmp/acme");
    let b2 = seed_block(&conn, "2026-09-24T11:00:00Z", 3600, Some("d2"));
    seed_event(&conn, b2, "e2", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Rebuild, "seed").unwrap();

    pin(&conn, "acme", Some("APRÓ"), None);
    let batch = new_batch(ChangeSource::Verdict);
    let logged = refresh_day(&conn, DAY, ChangeSource::Verdict, &batch).unwrap();
    assert_eq!(logged, 2);

    let result = feed(&conn, 0).unwrap();
    assert_eq!(result.changes.len(), 2);
    assert_eq!(result.batches.len(), 1);
    assert_eq!(result.batches[0].batch, batch);
    assert_eq!(result.batches[0].source, ChangeSource::Verdict);
    assert_eq!(
        result.batches[0].count, 2,
        "the batch touched two distinct blocks"
    );
    assert_eq!(result.cursor, result.changes.last().unwrap().id);

    // Nothing newer: the cursor stays put, never drops back to 0 (a live
    // poller treats 0 as "first poll" and would swallow the next batch).
    let empty = feed(&conn, result.cursor).unwrap();
    assert!(empty.changes.is_empty());
    assert_eq!(empty.cursor, result.cursor);
}

#[test]
fn unseen_then_mark_seen_hides_them() {
    let conn = open_memory().unwrap();
    pin(&conn, "acme", Some("Sjúkra"), None);
    let b = seed_block(&conn, "2026-09-24T09:00:00Z", 3600, Some("d1"));
    seed_event(&conn, b, "e1", "/tmp/acme");
    refresh_day(&conn, DAY, ChangeSource::Rebuild, "seed").unwrap();

    pin(&conn, "acme", Some("APRÓ"), None);
    refresh_day(&conn, DAY, ChangeSource::Verdict, "b2").unwrap();

    let before = unseen(&conn).unwrap();
    assert_eq!(before.changes.len(), 1);

    let marked = mark_seen(&conn, before.changes[0].id).unwrap();
    assert_eq!(marked, 1);
    assert!(unseen(&conn).unwrap().changes.is_empty());
}

#[test]
fn purge_old_removes_changes_past_retention() {
    let conn = open_memory().unwrap();
    let stale = (Utc::now() - Duration::days(CHANGE_RETENTION_DAYS + 1))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    conn.execute(
        "INSERT INTO block_changes (day, started_at, field, old_value, new_value, source, batch, created_at)
         VALUES ('2026-01-01','2026-01-01T09:00:00Z','customer','A','B','user','old-batch', ?1)",
        params![stale],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_changes (day, started_at, field, old_value, new_value, source, batch)
         VALUES ('2026-09-24','2026-09-24T09:00:00Z','customer','A','B','user','new-batch')",
        [],
    )
    .unwrap();

    let purged = purge_old(&conn).unwrap();
    assert_eq!(purged, 1);
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM block_changes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(remaining, 1);
}
