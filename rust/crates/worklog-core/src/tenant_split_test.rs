use chrono::{DateTime, TimeZone, Utc};
use rand::Rng;

use super::*;
use crate::billing_registry::{Customer, FolderMap};
use crate::db::open_memory;
use crate::models::Block;
use crate::tenant_contract::ClueStrength;

fn registry(customer_names: &[&str], folders: &[FolderMap]) -> Registry {
    Registry {
        customers: customer_names
            .iter()
            .map(|n| Customer {
                id: None,
                name: (*n).to_string(),
                aliases: Vec::new(),
            })
            .collect(),
        folders: folders.to_vec(),
    }
}

fn multi_tenant_folder(folder: &str) -> FolderMap {
    FolderMap {
        id: None,
        folder: folder.to_string(),
        customer: None,
        verkefni: None,
        billable: true,
        multi_tenant: true,
    }
}

fn at(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
}

fn clue(when: DateTime<Utc>, customer: &str, strength: ClueStrength) -> Clue {
    Clue {
        at: when,
        customer: customer.to_string(),
        strength,
    }
}

// B5 / FR-06: "Block with clues Sjúkra@13:40, MMS@15:30 splits at the
// midpoint; the midpoint second goes to Sjúkra" — the spec's own example.
#[test]
fn split_gives_seconds_to_nearest_clue() {
    let start = at(2026, 1, 1, 12, 0, 0).timestamp();
    let end = at(2026, 1, 1, 17, 0, 0).timestamp();
    let clues = vec![
        clue(at(2026, 1, 1, 13, 40, 0), "Sjúkra", ClueStrength::Branch),
        clue(at(2026, 1, 1, 15, 30, 0), "MMS", ClueStrength::Branch),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(slices.len(), 2);

    // 13:40 to 15:30 is 110 minutes; the midpoint clock time is 14:35:00,
    // and that second belongs to Sjúkra (the earlier clue) — so Sjúkra's
    // slice runs up to and including 14:35:00, i.e. ends at 14:35:01.
    let boundary = at(2026, 1, 1, 14, 35, 1).timestamp();

    let sjukra = slices
        .iter()
        .find(|s| s.customer.as_deref() == Some("Sjúkra"))
        .unwrap();
    assert_eq!(sjukra.intervals, vec![(start, boundary)]);
    assert_eq!(sjukra.origin, SplitOrigin::Clues);

    let mms = slices
        .iter()
        .find(|s| s.customer.as_deref() == Some("MMS"))
        .unwrap();
    assert_eq!(mms.intervals, vec![(boundary, end)]);
    assert_eq!(mms.origin, SplitOrigin::Clues);
}

// B6 / FR-07: "Block with APRÓ + Sjúkra clues → 100% Sjúkra".
#[test]
fn house_clues_dropped_when_other_customer_present() {
    let start = at(2026, 1, 1, 9, 0, 0).timestamp();
    let end = at(2026, 1, 1, 11, 0, 0).timestamp();
    let clues = vec![
        clue(
            at(2026, 1, 1, 9, 15, 0),
            HOUSE_CUSTOMER,
            ClueStrength::Branch,
        ),
        clue(at(2026, 1, 1, 10, 0, 0), "Sjúkra", ClueStrength::Branch),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start, end)],
            origin: SplitOrigin::Clues,
        }]
    );
}

// B7 / FR-08: "`mms` branch + `sjukra` path in one minute → Sjúkra"
// (path beats branch).
#[test]
fn stronger_clue_wins_its_minute() {
    let start = at(2026, 1, 1, 10, 0, 0).timestamp();
    let end = at(2026, 1, 1, 10, 1, 0).timestamp();
    let clues = vec![
        clue(at(2026, 1, 1, 10, 0, 5), "MMS", ClueStrength::Branch),
        clue(
            at(2026, 1, 1, 10, 0, 40),
            "Sjúkra",
            ClueStrength::TenantPath,
        ),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start, end)],
            origin: SplitOrigin::Clues,
        }]
    );
}

// B8 / FR-09: no timestamped clue → `split_block` is `None`, and
// `tenant_slices_for_block` falls back to a `Fallback` slice for billing to
// resolve (here: no events, no matching summary text).
#[test]
fn no_clue_block_falls_back() {
    assert_eq!(split_block(0, 100, &[]), None);

    let conn = open_memory().unwrap();
    let reg = registry(
        &["Sjúkra", HOUSE_CUSTOMER],
        &[multi_tenant_folder("vitinn-infra")],
    );
    let start = at(2026, 1, 1, 9, 0, 0);
    let block = Block {
        id: 1,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: at(2026, 1, 1, 10, 0, 0).to_rfc3339(),
        duration_seconds: 3600,
        description: Some("Unrelated maintenance work".to_string()),
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: None,
            intervals: vec![(start.timestamp(), start.timestamp() + 3600)],
            origin: SplitOrigin::Fallback,
        }]
    );
}

// B9 / FR-12: over random clues, slices are disjoint and sum exactly to the
// block's duration.
#[test]
fn slices_sum_exactly_property() {
    let mut rng = rand::thread_rng();
    let customers = ["Sjúkra", "MMS", HOUSE_CUSTOMER];
    let strengths = [
        ClueStrength::Summary,
        ClueStrength::Branch,
        ClueStrength::TenantPath,
    ];
    let start = 1_700_000_000_i64;

    for _ in 0..1000 {
        let duration = rng.gen_range(1..=3600);
        let end = start + duration;
        let clue_count = rng.gen_range(1..=6);
        let clues: Vec<Clue> = (0..clue_count)
            .map(|_| Clue {
                at: DateTime::<Utc>::from_timestamp(rng.gen_range(start..end), 0).unwrap(),
                customer: customers[rng.gen_range(0..customers.len())].to_string(),
                strength: strengths[rng.gen_range(0..strengths.len())],
            })
            .collect();

        let slices = split_block(start, end, &clues)
            .expect("clues is non-empty, split_block must produce a split");

        let mut intervals: Vec<(i64, i64)> =
            slices.iter().flat_map(|s| s.intervals.clone()).collect();
        intervals.sort_by_key(|&(from, _)| from);

        let mut cursor = start;
        for (from, to) in &intervals {
            assert_eq!(*from, cursor, "slices must never gap or overlap");
            cursor = *to;
        }
        assert_eq!(
            cursor, end,
            "slices must sum exactly to the block's duration"
        );
    }
}
