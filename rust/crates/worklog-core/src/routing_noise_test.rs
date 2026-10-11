use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;

fn seed(
    conn: &Connection,
    n: &mut u32,
    source: &str,
    started: &str,
    url: &str,
    origin: Option<&str>,
) {
    *n += 1;
    let mut e = Event::minimal(source, format!("e{n}"), started, "page");
    e.details = Some(url.to_string());
    let id = repo::upsert_event(conn, &e).unwrap();
    conn.execute(
        "UPDATE events SET label_origin = ?1 WHERE id = ?2",
        rusqlite::params![origin, id],
    )
    .unwrap();
}

fn names(v: &[NoiseHost]) -> Vec<(String, i64)> {
    v.iter()
        .map(|h| (h.host.clone(), h.dismissed_before))
        .collect()
}

const DAY: &str = "2026-10-10";
const OLD: &str = "2026-10-01T10:00:00+00:00";

#[test]
fn empty_db_has_no_noise_hosts() {
    let conn = open_memory().unwrap();
    assert!(noise_hosts(&conn, DAY).unwrap().is_empty());
}

#[test]
fn exactly_min_dismissed_counts_one_below_does_not() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    // catches `>` instead of `>=` on the minimum
    for _ in 0..NOISE_MIN_DISMISSED {
        seed(
            &conn,
            &mut n,
            "firefox",
            OLD,
            "https://a.com/x",
            Some("dismissed"),
        );
    }
    // catches `>=` against min - 1
    for _ in 0..NOISE_MIN_DISMISSED - 1 {
        seed(
            &conn,
            &mut n,
            "firefox",
            OLD,
            "https://b.com/x",
            Some("dismissed"),
        );
    }
    assert_eq!(
        names(&noise_hosts(&conn, DAY).unwrap()),
        [("a.com".to_string(), NOISE_MIN_DISMISSED)]
    );
}

#[test]
fn one_kept_label_disqualifies_the_host() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    for _ in 0..3 {
        seed(
            &conn,
            &mut n,
            "firefox",
            OLD,
            "https://a.com/x",
            Some("dismissed"),
        );
    }
    // catches a majority rule instead of "every row"
    seed(
        &conn,
        &mut n,
        "firefox",
        OLD,
        "https://a.com/y",
        Some("fix"),
    );
    assert!(noise_hosts(&conn, DAY).unwrap().is_empty());
}

#[test]
fn unlabelled_rows_neither_count_nor_disqualify() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    for _ in 0..2 {
        seed(
            &conn,
            &mut n,
            "firefox",
            OLD,
            "https://a.com/x",
            Some("dismissed"),
        );
    }
    // catches treating a NULL origin as a non-dismissed label
    seed(&conn, &mut n, "firefox", OLD, "https://a.com/y", None);
    assert_eq!(
        names(&noise_hosts(&conn, DAY).unwrap()),
        [("a.com".to_string(), 2)]
    );
}

#[test]
fn day_itself_and_later_are_excluded() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    seed(
        &conn,
        &mut n,
        "firefox",
        "2026-10-09T23:59:59+00:00",
        "https://a.com/x",
        Some("dismissed"),
    );
    // catches `<=` on the day bound
    seed(
        &conn,
        &mut n,
        "firefox",
        "2026-10-10T00:00:00+00:00",
        "https://a.com/x",
        Some("dismissed"),
    );
    seed(
        &conn,
        &mut n,
        "firefox",
        "2026-10-11T09:00:00+00:00",
        "https://a.com/x",
        Some("dismissed"),
    );
    assert!(noise_hosts(&conn, DAY).unwrap().is_empty());
}

#[test]
fn only_firefox_events_count() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    // catches a missing source filter
    for _ in 0..3 {
        seed(
            &conn,
            &mut n,
            "slack",
            OLD,
            "https://a.com/x",
            Some("dismissed"),
        );
    }
    assert!(noise_hosts(&conn, DAY).unwrap().is_empty());
}

#[test]
fn hosts_group_by_normalised_domain_without_prefix_merging() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    // same host through different case/port/scheme groups together
    seed(
        &conn,
        &mut n,
        "firefox",
        OLD,
        "https://WWW.YouTube.com/a",
        Some("dismissed"),
    );
    seed(
        &conn,
        &mut n,
        "firefox",
        OLD,
        "http://www.youtube.com:8080/b?q=1",
        Some("dismissed"),
    );
    // a parent domain is a different host (catches suffix/prefix grouping)
    seed(
        &conn,
        &mut n,
        "firefox",
        OLD,
        "https://youtube.com/c",
        Some("dismissed"),
    );
    assert_eq!(
        names(&noise_hosts(&conn, DAY).unwrap()),
        [("www.youtube.com".to_string(), 2)]
    );
}

#[test]
fn sorted_by_count_desc_then_host() {
    let conn = open_memory().unwrap();
    let mut n = 0;
    for (url, times) in [
        ("https://b.com/", 2),
        ("https://a.com/", 2),
        ("https://c.com/", 3),
    ] {
        for _ in 0..times {
            seed(&conn, &mut n, "firefox", OLD, url, Some("dismissed"));
        }
    }
    assert_eq!(
        names(&noise_hosts(&conn, DAY).unwrap()),
        [
            ("c.com".to_string(), 3),
            ("a.com".to_string(), 2),
            ("b.com".to_string(), 2)
        ]
    );
}
