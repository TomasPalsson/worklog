use super::*;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;

fn registry(names: &[&str]) -> Registry {
    Registry {
        customers: names
            .iter()
            .map(|n| Customer {
                id: None,
                name: (*n).to_string(),
                aliases: Vec::new(),
            })
            .collect(),
        folders: Vec::new(),
    }
}

fn shares(day: &str, started_at: &str, pairs: &[(&str, f64)]) -> CustomerShares {
    CustomerShares {
        day: day.to_string(),
        started_at: started_at.to_string(),
        shares: pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    }
}

#[test]
fn save_load_and_clear_round_trip() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra", "APRÓ"]);
    let s = shares(
        "2026-09-23",
        "2026-09-23T10:00:00Z",
        &[("Sjúkra", 0.7), ("APRÓ", 0.3)],
    );
    save_shares(&conn, &s, &reg).unwrap();

    let loaded = load_shares(&conn, "2026-09-23", "2026-09-23T10:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.shares, s.shares);

    // Upsert overwrites rather than duplicating.
    let s2 = shares("2026-09-23", "2026-09-23T10:00:00Z", &[("APRÓ", 1.0)]);
    save_shares(&conn, &s2, &reg).unwrap();
    let loaded = load_shares(&conn, "2026-09-23", "2026-09-23T10:00:00Z")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.shares, s2.shares);

    clear_shares(&conn, "2026-09-23", "2026-09-23T10:00:00Z").unwrap();
    assert!(load_shares(&conn, "2026-09-23", "2026-09-23T10:00:00Z")
        .unwrap()
        .is_none());
}

#[test]
fn load_missing_row_is_none() {
    let conn = open_memory().unwrap();
    assert!(load_shares(&conn, "2026-09-23", "2026-09-23T10:00:00Z")
        .unwrap()
        .is_none());
}

#[test]
fn save_rejects_shares_that_do_not_sum_to_one() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra", "APRÓ"]);
    let s = shares(
        "2026-09-23",
        "2026-09-23T10:00:00Z",
        &[("Sjúkra", 0.5), ("APRÓ", 0.2)],
    );
    let err = save_shares(&conn, &s, &reg).unwrap_err();
    assert_eq!(err.to_string(), "Shares must add up to 100%");
}

#[test]
fn save_rejects_unknown_customer() {
    let conn = open_memory().unwrap();
    let reg = registry(&["APRÓ"]);
    let s = shares("2026-09-23", "2026-09-23T10:00:00Z", &[("Ghost", 1.0)]);
    let err = save_shares(&conn, &s, &reg).unwrap_err();
    assert_eq!(err.to_string(), "Customer no longer exists");
}

#[test]
fn save_rejects_a_share_at_or_below_zero() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra", "APRÓ"]);
    let s = shares(
        "2026-09-23",
        "2026-09-23T10:00:00Z",
        &[("Sjúkra", 1.0), ("APRÓ", 0.0)],
    );
    let err = save_shares(&conn, &s, &reg).unwrap_err();
    assert_eq!(err.to_string(), "Share must be above 0");
}

#[test]
fn slices_from_shares_are_consecutive_in_customer_name_order_with_remainder_last() {
    // 100 seconds, split 30/70 between APRO (alphabetically first) and Sjukra.
    let s = shares(
        "2026-09-23",
        "2026-09-23T10:00:00Z",
        &[("Sjukra", 0.7), ("APRO", 0.3)],
    );
    let slices = slices_from_shares(1000, 1100, &s);
    assert_eq!(slices.len(), 2);

    assert_eq!(slices[0].customer, Some("APRO".to_string()));
    assert_eq!(slices[0].intervals, vec![(1000, 1030)]);
    assert_eq!(slices[0].origin, SplitOrigin::Manual);

    assert_eq!(slices[1].customer, Some("Sjukra".to_string()));
    assert_eq!(slices[1].intervals, vec![(1030, 1100)]);
    assert_eq!(slices[1].origin, SplitOrigin::Manual);
}

#[test]
fn slices_from_shares_last_slice_absorbs_the_rounding_remainder() {
    // 10 seconds split three ways: 1/3 each doesn't divide evenly, so the
    // last customer (name order) must absorb the leftover second.
    let s = shares(
        "2026-09-23",
        "2026-09-23T10:00:00Z",
        &[("A", 1.0 / 3.0), ("B", 1.0 / 3.0), ("C", 1.0 / 3.0)],
    );
    let slices = slices_from_shares(0, 10, &s);
    assert_eq!(slices.len(), 3);
    let total: i64 = slices
        .iter()
        .map(|sl| sl.intervals[0].1 - sl.intervals[0].0)
        .sum();
    assert_eq!(total, 10);
    assert_eq!(slices.last().unwrap().intervals[0].1, 10);
}

// ───────── FR-11 (B10): a hand-set split survives re-inference keyed on
// (day, started_at) — and does NOT follow a block whose start moved ─────────

fn work(sub: &str) -> String {
    format!(
        "{}/Desktop/Work/{sub}",
        dirs::home_dir().unwrap().to_string_lossy()
    )
}

fn seed_claude_event(conn: &Connection, source_id: &str, started_at: &str, folder: &str) {
    let mut ev = Event::minimal("claude", source_id, started_at, "worked");
    ev.project_path = Some(work(folder));
    // `infer::load_day_events` reads straight from `events`; no block_events
    // row is needed until `persist_blocks` creates one.
    repo::upsert_event(conn, &ev).unwrap();
}

/// Multi-tenant `vitinn-infra` folder with customer `Acme`, wired the way
/// `worklog infer` / the daemon rebuild actually resolves a block's
/// customer split.
fn seed_multi_tenant_folder(conn: &Connection) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: "vitinn-infra".into(),
            customer: None,
            verkefni: None,
            billable: true,
            multi_tenant: true,
        },
    )
    .unwrap();
    upsert_customer(
        conn,
        &Customer {
            id: None,
            name: "Acme".into(),
            aliases: Vec::new(),
        },
    )
    .unwrap();
}

/// Runs the same rebuild `worklog infer` / the daemon use: clustering, then
/// persisting (which deletes and reinserts the day's blocks).
fn reinfer(conn: &Connection, day: chrono::NaiveDate) {
    let blocks = crate::infer_allocations::build_day_blocks(conn, day).unwrap();
    crate::infer::persist_blocks(conn, day, &blocks).unwrap();
}

#[test]
fn shares_survive_reinfer() {
    let conn = open_memory().unwrap();
    seed_multi_tenant_folder(&conn);
    // Two events 3 minutes apart cluster into one >=5-minute block.
    seed_claude_event(&conn, "e1", "2026-07-23T09:00:00Z", "vitinn-infra");
    seed_claude_event(&conn, "e2", "2026-07-23T09:03:00Z", "vitinn-infra");

    let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
    reinfer(&conn, day);

    let day_blocks = repo::list_blocks_for_day(&conn, "2026-07-23").unwrap();
    assert_eq!(day_blocks.len(), 1, "got {day_blocks:#?}");
    let started_at = day_blocks[0].started_at.clone();

    let reg = Registry::load(&conn).unwrap();
    save_shares(
        &conn,
        &shares("2026-07-23", &started_at, &[("Acme", 1.0)]),
        &reg,
    )
    .unwrap();

    // Re-run the day's inference the way the app does — same events, same
    // clustering, so the block's start is unchanged even though its id
    // (and every other in-memory struct) is rebuilt from scratch.
    reinfer(&conn, day);

    let day_blocks = repo::list_blocks_for_day(&conn, "2026-07-23").unwrap();
    assert_eq!(day_blocks.len(), 1, "got {day_blocks:#?}");
    let block = &day_blocks[0];
    assert_eq!(
        block.started_at, started_at,
        "re-infer must not have moved this block's start"
    );

    let folder = crate::billing::work_folder_for_block(&conn, block.id)
        .unwrap()
        .unwrap();
    let slices = crate::tenant_split::tenant_slices_for_block(&conn, block, &folder, &reg)
        .unwrap()
        .expect("vitinn-infra is multi-tenant");
    assert!(!slices.is_empty());
    for slice in &slices {
        assert_eq!(
            slice.origin,
            SplitOrigin::Manual,
            "hand-set shares must survive a same-start re-infer: {slices:#?}"
        );
    }
    assert_eq!(slices[0].customer, Some("Acme".to_string()));
}

#[test]
fn shares_dropped_when_block_start_moves() {
    let conn = open_memory().unwrap();
    seed_multi_tenant_folder(&conn);
    seed_claude_event(&conn, "e1", "2026-07-23T09:00:00Z", "vitinn-infra");
    seed_claude_event(&conn, "e2", "2026-07-23T09:03:00Z", "vitinn-infra");

    let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
    reinfer(&conn, day);

    let day_blocks = repo::list_blocks_for_day(&conn, "2026-07-23").unwrap();
    assert_eq!(day_blocks.len(), 1, "got {day_blocks:#?}");
    let started_at = day_blocks[0].started_at.clone();

    let reg = Registry::load(&conn).unwrap();
    save_shares(
        &conn,
        &shares("2026-07-23", &started_at, &[("Acme", 1.0)]),
        &reg,
    )
    .unwrap();

    // A backfilled earlier event (same lane, well inside the timeout)
    // extends the cluster backwards, moving the block's started_at.
    seed_claude_event(&conn, "e0", "2026-07-23T08:50:00Z", "vitinn-infra");
    reinfer(&conn, day);

    let day_blocks = repo::list_blocks_for_day(&conn, "2026-07-23").unwrap();
    assert_eq!(day_blocks.len(), 1, "got {day_blocks:#?}");
    let block = &day_blocks[0];
    assert_ne!(
        block.started_at, started_at,
        "test is only meaningful if the start actually moved"
    );

    let folder = crate::billing::work_folder_for_block(&conn, block.id)
        .unwrap()
        .unwrap();
    let slices = crate::tenant_split::tenant_slices_for_block(&conn, block, &folder, &reg)
        .unwrap()
        .expect("vitinn-infra is multi-tenant");
    assert!(
        !slices.iter().any(|s| s.origin == SplitOrigin::Manual),
        "shares keyed to the old start must not apply to the moved block: {slices:#?}"
    );
}
