use super::*;
use crate::billing_registry::Customer;
use crate::db::open_memory;

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
