// Billing export over multi-tenant slices (spec 005, T005). Split file for
// billing.rs's line budget — see tenant_split_test.rs for the split rules
// themselves.

use super::*;
use crate::billing_deildir::upsert_deild;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::deild_contract::{BlockShares, Deild, ShareRow};
use crate::models::Event;
use crate::repo as repository;
use crate::tenant_contract::HOUSE_CUSTOMER;
use crate::tenant_shares::save_rows;
use rusqlite::params;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

fn work(sub: &str) -> String {
    format!("{}/Desktop/Work/{sub}", home())
}

fn multi_tenant_folder(folder: &str, customer: Option<&str>) -> FolderMap {
    FolderMap {
        id: None,
        folder: folder.into(),
        customer: customer.map(str::to_owned),
        verkefni: None,
        billable: true,
        multi_tenant: true,
    }
}

fn seed_block(
    conn: &Connection,
    started_at: &str,
    duration_seconds: i64,
    description: Option<&str>,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks
            (day, jira_issue, started_at, ended_at, duration_seconds, description, is_personal)
         VALUES ('2026-09-24', NULL, ?1, ?1, ?2, ?3, 0)",
        params![started_at, duration_seconds, description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(
    conn: &Connection,
    block_id: i64,
    source_id: &str,
    project_path: &str,
    title: &str,
    started_at: &str,
) {
    let mut ev = Event::minimal("claude", source_id, started_at, title);
    ev.project_path = Some(project_path.to_string());
    let eid = repository::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();
}

// B8 (billing side) / FR-09: no timestamped clue -> billing falls back to
// today's resolution (here: the folder's own House pin), exactly as if the
// block were never split.
#[test]
fn no_clue_block_falls_back() {
    let c = open_memory().unwrap();
    upsert_folder(
        &c,
        &multi_tenant_folder("vitinn-infra", Some(HOUSE_CUSTOMER)),
    )
    .unwrap();

    let block = seed_block(
        &c,
        "2026-09-24T09:00:00+00:00",
        3600,
        Some("Unrelated maintenance work"),
    );
    seed_event(
        &c,
        block,
        "e1",
        &work("vitinn-infra"),
        "commit",
        "2026-09-24T09:00:00+00:00",
    );

    let rows = rows_for_day(&c, "2026-09-24").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].customer, Some(HOUSE_CUSTOMER.to_string()));
    assert_eq!(rows[0].hours, 1.0);
}

// B12 / J1: a 2026-09-24-shaped day — a Sjúkra-branch block and a
// clue-less block in the same multi-tenant folder — exports as two lines,
// one per customer, each billed only its own slice.
#[test]
fn mixed_day_exports_customer_lines() {
    let c = open_memory().unwrap();
    upsert_customer(
        &c,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec!["Sjukra".into()],
        },
    )
    .unwrap();
    upsert_folder(
        &c,
        &multi_tenant_folder("vitinn-infra", Some(HOUSE_CUSTOMER)),
    )
    .unwrap();

    // 2.5h of Sjúkra-branch work.
    let sjukra_block = seed_block(
        &c,
        "2026-09-24T09:00:00+00:00",
        9000,
        Some("Sjukra tenant work"),
    );
    seed_event(
        &c,
        sjukra_block,
        "e1",
        &work("vitinn-infra"),
        "checkout sjukra",
        "2026-09-24T09:00:00+00:00",
    );

    // 1h with no clue at all -> the folder's House pin.
    let house_block = seed_block(
        &c,
        "2026-09-24T13:00:00+00:00",
        3600,
        Some("General maintenance"),
    );
    seed_event(
        &c,
        house_block,
        "e2",
        &work("vitinn-infra"),
        "commit",
        "2026-09-24T13:00:00+00:00",
    );

    let rows = rows_for_day(&c, "2026-09-24").unwrap();
    assert_eq!(rows.len(), 2);

    let sjukra_row = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some("Sjúkra"))
        .expect("a Sjúkra line");
    assert_eq!(sjukra_row.hours, 2.5);

    let house_row = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some(HOUSE_CUSTOMER))
        .expect("an APRÓ line");
    assert_eq!(house_row.hours, 1.0);
}

// Verkefni comes only from an explicit pin, and the pin belongs to the
// folder's own customer, not to every slice split out of it. A Sjúkra
// slice of an APRÓ-pinned multi-tenant folder must not inherit APRÓ's
// Verkefni — that would land an invented line item on Sjúkra's invoice.
#[test]
fn split_slice_for_other_customer_gets_no_verkefni() {
    let c = open_memory().unwrap();
    upsert_customer(
        &c,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec!["Sjukra".into()],
        },
    )
    .unwrap();
    upsert_folder(
        &c,
        &FolderMap {
            id: None,
            folder: "vitinn-infra".into(),
            customer: Some(HOUSE_CUSTOMER.to_string()),
            verkefni: Some("[P] Rekstur".into()),
            billable: true,
            multi_tenant: true,
        },
    )
    .unwrap();

    // 2.5h of Sjúkra-branch work.
    let sjukra_block = seed_block(
        &c,
        "2026-09-24T09:00:00+00:00",
        9000,
        Some("Sjukra tenant work"),
    );
    seed_event(
        &c,
        sjukra_block,
        "e1",
        &work("vitinn-infra"),
        "checkout sjukra",
        "2026-09-24T09:00:00+00:00",
    );

    // 1h with no clue at all -> the folder's own House pin, Verkefni intact.
    let house_block = seed_block(
        &c,
        "2026-09-24T13:00:00+00:00",
        3600,
        Some("General maintenance"),
    );
    seed_event(
        &c,
        house_block,
        "e2",
        &work("vitinn-infra"),
        "commit",
        "2026-09-24T13:00:00+00:00",
    );

    let rows = rows_for_day(&c, "2026-09-24").unwrap();
    assert_eq!(rows.len(), 2);

    let sjukra_row = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some("Sjúkra"))
        .expect("a Sjúkra line");
    assert_eq!(
        sjukra_row.verkefni, None,
        "a Sjúkra slice must not inherit APRÓ's Verkefni"
    );

    let house_row = rows
        .iter()
        .find(|r| r.customer.as_deref() == Some(HOUSE_CUSTOMER))
        .expect("an APRÓ line");
    assert_eq!(
        house_row.verkefni,
        Some("[P] Rekstur".into()),
        "the folder's own customer keeps its pinned Verkefni"
    );
}

// B14 / §5: a 30-block, ~2000-event multi-tenant day must still export
// under 500ms — the per-block slice lookup must not turn into an O(n^2) scan.
#[test]
fn export_latency_under_500ms() {
    let c = open_memory().unwrap();
    upsert_folder(
        &c,
        &multi_tenant_folder("vitinn-infra", Some(HOUSE_CUSTOMER)),
    )
    .unwrap();

    for b in 0..30 {
        let started_at = format!("2026-09-24T{:02}:00:00+00:00", b % 24);
        let block_id = seed_block(&c, &started_at, 1800, Some("Routine work"));
        for e in 0..67 {
            seed_event(
                &c,
                block_id,
                &format!("e{b}-{e}"),
                &work("vitinn-infra"),
                "commit",
                &started_at,
            );
        }
    }

    let start = std::time::Instant::now();
    let rows = rows_for_day(&c, "2026-09-24").unwrap();
    let elapsed = start.elapsed();

    assert!(!rows.is_empty());
    assert!(
        elapsed.as_millis() < 500,
        "export took {elapsed:?}, want < 500ms"
    );
}

// §5: the same 30-block, ~2000-event day, now with the deildir keyword
// ladder and a few hand-set splits in play, must still resolve under
// 200ms — deild resolution and saved-split lookups must not turn the
// per-block work into an O(n^2) scan either.
#[test]
fn rows_for_day_with_deildir_under_200ms() {
    let c = open_memory().unwrap();
    upsert_folder(
        &c,
        &multi_tenant_folder("vitinn-infra", Some(HOUSE_CUSTOMER)),
    )
    .unwrap();
    upsert_customer(
        &c,
        &Customer {
            id: None,
            name: HOUSE_CUSTOMER.to_string(),
            aliases: vec![],
        },
    )
    .unwrap();
    upsert_customer(
        &c,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec!["Sjukra".into()],
        },
    )
    .unwrap();
    upsert_deild(
        &c,
        &Deild {
            id: None,
            customer: HOUSE_CUSTOMER.to_string(),
            name: "Rekstur".into(),
            keywords: vec!["deploy".into()],
        },
    )
    .unwrap();

    let registry = crate::billing_registry::Registry::load(&c).unwrap();

    for b in 0..30 {
        let started_at = format!("2026-09-24T{:02}:00:00+00:00", b % 24);
        let description = if b % 3 == 0 {
            "Routine deploy work"
        } else {
            "Routine work"
        };
        let block_id = seed_block(&c, &started_at, 1800, Some(description));
        for e in 0..67 {
            seed_event(
                &c,
                block_id,
                &format!("e{b}-{e}"),
                &work("vitinn-infra"),
                "commit",
                &started_at,
            );
        }
        if b % 10 == 0 {
            save_rows(
                &c,
                &BlockShares {
                    day: "2026-09-24".into(),
                    started_at: started_at.clone(),
                    rows: vec![
                        ShareRow {
                            customer: HOUSE_CUSTOMER.to_string(),
                            deild: Some("Rekstur".into()),
                            fraction: 0.6,
                        },
                        ShareRow {
                            customer: "Sjúkra".into(),
                            deild: None,
                            fraction: 0.4,
                        },
                    ],
                },
                &registry,
            )
            .unwrap();
        }
    }

    let start = std::time::Instant::now();
    let rows = rows_for_day(&c, "2026-09-24").unwrap();
    let elapsed = start.elapsed();

    assert!(!rows.is_empty());
    assert!(
        rows.iter().any(|r| r.verkefni.is_some()),
        "at least one row should have a resolved verkefni"
    );
    assert!(
        elapsed.as_millis() <= 200,
        "rows_for_day took {elapsed:?}, want <= 200ms"
    );
}
