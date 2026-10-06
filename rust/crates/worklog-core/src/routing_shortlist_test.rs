use super::*;
use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use crate::routing::fetch_event;

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()
}

fn pin(conn: &Connection, folder: &str, customer: Option<&str>) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.into(),
            customer: customer.map(str::to_owned),
            verkefni: None,
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
            aliases: vec![],
        },
    )
    .unwrap();
}

/// `n` work events in `n` distinct minutes starting at `date` 09:00 UTC.
fn work(conn: &Connection, path_tail: &str, date: &str, n: u32, per_minute: u32) {
    let prefix = crate::billing::work_prefix().unwrap();
    for m in 0..n {
        for k in 0..per_minute {
            let sid = format!("{path_tail}-{date}-{m}-{k}");
            let ts = format!("{date}T09:{m:02}:{k:02}+00:00");
            let id = repo::upsert_event(conn, &Event::minimal("claude", sid, ts, "t")).unwrap();
            conn.execute(
                "UPDATE events SET project_path = ?1 WHERE id = ?2",
                rusqlite::params![format!("{prefix}/{path_tail}"), id],
            )
            .unwrap();
        }
    }
}

fn loose(conn: &Connection, title: &str, details: Option<&str>, container: Option<&str>) -> i64 {
    let id = repo::upsert_event(
        conn,
        &Event::minimal("slack", "loose", "2026-04-20T10:00:00+00:00", title),
    )
    .unwrap();
    conn.execute(
        "UPDATE events SET details = ?1, container = ?2 WHERE id = ?3",
        rusqlite::params![details, container, id],
    )
    .unwrap();
    id
}

fn offered(conn: &Connection, id: i64) -> Vec<String> {
    let row = fetch_event(conn, id).unwrap().unwrap();
    shortlist(conn, &row, day()).unwrap()
}

#[test]
fn thirty_folders_yield_the_expected_six_in_order() {
    let conn = open_memory().unwrap();
    for i in 0..30 {
        pin(&conn, &format!("zq-pad{i:02}"), None);
    }
    for f in [
        "zq-n1", "zq-n2", "zq-ra", "zq-rb", "zq-rc", "zq-rd", "zq-re",
    ] {
        pin(&conn, f, None);
    }
    // Minutes descending is the reverse of alphabetical: catches sorting by name.
    for (f, n) in [
        ("zq-rd", 5),
        ("zq-rc", 4),
        ("zq-rb", 3),
        ("zq-ra", 2),
        ("zq-re", 1),
    ] {
        work(&conn, f, "2026-04-19", n, 1);
    }
    let id = loose(&conn, "zq-n1 review", Some("see zq-n2"), None);
    // Near-hits before recents, recents by minutes, capped at 6 (not 7+).
    assert_eq!(
        offered(&conn, id),
        ["zq-n1", "zq-n2", "zq-rd", "zq-rc", "zq-rb", "zq-ra"]
    );
}

#[test]
fn recent_window_is_the_fourteen_days_ending_on_the_day() {
    let conn = open_memory().unwrap();
    for f in ["zq-in", "zq-out", "zq-future"] {
        pin(&conn, f, None);
    }
    work(&conn, "zq-in", "2026-04-07", 1, 1); // day - 13: first included day
    work(&conn, "zq-out", "2026-04-06", 5, 1); // day - 14: just past the limit
    work(&conn, "zq-future", "2026-04-21", 5, 1); // after the day
    let id = loose(&conn, "nothing", None, None);
    // Catches > for >= (drops zq-in) and an off-by-one window (adds zq-out).
    assert_eq!(offered(&conn, id), ["zq-in"]);
}

#[test]
fn minutes_are_distinct_minutes_not_event_counts() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-burst", None);
    pin(&conn, "zq-steady", None);
    work(&conn, "zq-burst", "2026-04-19", 1, 10); // 10 events, 1 minute
    work(&conn, "zq-steady", "2026-04-19", 3, 1); // 3 events, 3 minutes
    let id = loose(&conn, "nothing", None, None);
    // Catches COUNT(*).
    assert_eq!(offered(&conn, id), ["zq-steady", "zq-burst"]);
}

#[test]
fn worktree_work_counts_toward_the_project_root() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-wt", None);
    work(
        &conn,
        "zq-wt/.claude/worktrees/branch-x",
        "2026-04-19",
        2,
        1,
    );
    let id = loose(&conn, "nothing", None, None);
    // Catches using the path basename (a branch name) as the folder.
    assert_eq!(offered(&conn, id), ["zq-wt"]);
}

#[test]
fn nothing_known_offers_nothing() {
    let conn = open_memory().unwrap();
    pin(&conn, "zq-idle", None);
    let id = loose(&conn, "nothing", None, None);
    // Catches padding the list with arbitrary project keys.
    assert!(offered(&conn, id).is_empty());
}

#[test]
fn near_hit_is_a_whole_word_and_ignores_case() {
    let conn = open_memory().unwrap();
    for f in ["zq-app", "zq-ap", "zq-apple"] {
        pin(&conn, f, None);
    }
    let id = loose(&conn, "see ZQ-APP, then zq-apples", None, None);
    // zq-ap is a prefix of a longer token, zq-apple is a prefix of zq-apples.
    // Catches substring contains().
    assert_eq!(offered(&conn, id), ["zq-app"]);
}

#[test]
fn near_hit_reads_title_details_and_container() {
    let conn = open_memory().unwrap();
    for f in ["zq-t", "zq-d", "zq-c"] {
        pin(&conn, f, None);
    }
    let id = loose(&conn, "about zq-t", Some("link zq-d"), Some("zq-c"));
    // Catches reading only one of the three fields.
    assert_eq!(offered(&conn, id), ["zq-c", "zq-d", "zq-t"]);
}

#[test]
fn pinned_folder_alone_is_never_empty() {
    let conn = open_memory().unwrap();
    customer(&conn, "Sjúkra");
    pin(&conn, "zq-sjukra", Some("Sjúkra"));
    let id = loose(&conn, "nothing", None, Some("Sjúkra"));
    // Catches building the list only from near-hits and recents.
    assert_eq!(offered(&conn, id), ["zq-sjukra"]);
}

#[test]
fn pinned_folders_come_first_and_cap_at_six() {
    let conn = open_memory().unwrap();
    customer(&conn, "Seven");
    customer(&conn, "Six");
    for i in 0..7 {
        pin(&conn, &format!("zq-seven{i}"), Some("Seven"));
    }
    for i in 0..6 {
        pin(&conn, &format!("zq-six{i}"), Some("Six"));
    }
    let seven = loose(&conn, "nothing", None, Some("Seven"));
    let got = offered(&conn, seven);
    // Catches no cap on the pinned step.
    assert_eq!(got.len(), 6);
    assert!(got.iter().all(|f| f.starts_with("zq-seven")));
    // Exactly at the cap keeps all six: catches > 5 / >= 6 cuts.
    let six = loose(&conn, "nothing", None, Some("Six"));
    assert_eq!(offered(&conn, six).len(), 6);
}

#[test]
fn customer_container_keeps_other_customers_out() {
    let conn = open_memory().unwrap();
    customer(&conn, "Sjúkra");
    customer(&conn, "MMS");
    pin(&conn, "zq-sjukra", Some("Sjúkra"));
    pin(&conn, "zq-mms", Some("MMS"));
    work(&conn, "zq-mms", "2026-04-19", 3, 1);
    let id = loose(&conn, "zq-mms", Some("zq-mms"), Some("Sjúkra"));
    // Catches ignoring the customer narrowing (B10) in near-hits/recents.
    assert_eq!(offered(&conn, id), ["zq-sjukra"]);
}

#[test]
fn examples_are_cut_to_sixty_chars_and_capped_at_six_hundred_total() {
    let conn = open_memory().unwrap();
    let folders: Vec<String> = (0..6).map(|i| format!("zq-e{i}")).collect();
    for f in &folders {
        pin(&conn, f, None);
    }
    for f in &folders {
        for n in 0..6 {
            let title = format!("{f}-{n}-{}", "x".repeat(70));
            let id = repo::upsert_event(
                &conn,
                &Event::minimal(
                    "slack",
                    format!("{f}{n}"),
                    "2026-04-19T10:00:00+00:00",
                    title,
                ),
            )
            .unwrap();
            crate::routing::label_event(
                &conn,
                id,
                &crate::routing_contract::LabelRequest {
                    folder: f.clone(),
                    always: None,
                },
            )
            .unwrap();
        }
    }
    let got = examples_for_options(&conn, &folders).unwrap();
    let all: Vec<&String> = got.values().flatten().collect();
    // Each cut to 60: catches no cut (70) and a 61 cut.
    assert!(all.iter().all(|t| t.chars().count() == 60));
    // Together exactly 600: catches < for <= (9 examples) and no cap (30).
    assert_eq!(all.len(), 10);
    // At most 5 per project: catches a missing per-project limit.
    assert_eq!(got["zq-e0"].len(), 5);
    assert_eq!(got["zq-e1"].len(), 5);
    assert!(!got.contains_key("zq-e2"));
}
