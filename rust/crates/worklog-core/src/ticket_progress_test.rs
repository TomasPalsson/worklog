use super::*;
use crate::db::open_memory;
use chrono::{Duration, TimeZone};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 9, 12, 0, 0).unwrap()
}

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn log(id: &str, account: &str, name: &str, day: &str, seconds: i64) -> RawWorklog {
    RawWorklog {
        worklog_id: id.into(),
        account_id: account.into(),
        name: name.into(),
        day: d(day),
        seconds,
    }
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| s.to_string()).collect()
}

fn block(conn: &Connection, day: &str, issue: Option<&str>, personal: i64) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, is_personal)
         VALUES (?1, ?2, ?1 || 'T09:00:00Z', ?1 || 'T10:00:00Z', 3600, ?3)",
        params![day, issue, personal],
    )
    .unwrap();
}

#[test]
fn fresh_cache_is_not_stale_and_ten_minutes_exactly_is_fresh() {
    let conn = open_memory().unwrap();
    store_ticket(&conn, "A-1", None, &[], now() - Duration::seconds(600)).unwrap();
    // `>=` instead of `>` would list A-1 here
    assert!(stale_keys(&conn, &keys(&["A-1"]), now())
        .unwrap()
        .is_empty());
}

#[test]
fn cache_one_second_past_ten_minutes_is_stale() {
    let conn = open_memory().unwrap();
    store_ticket(&conn, "A-1", None, &[], now() - Duration::seconds(601)).unwrap();
    assert_eq!(
        stale_keys(&conn, &keys(&["A-1"]), now()).unwrap(),
        keys(&["A-1"])
    );
}

#[test]
fn never_fetched_key_is_stale_and_unknown_to_view() {
    let conn = open_memory().unwrap();
    // an inner join with the cache would drop the unfetched key
    assert_eq!(
        stale_keys(&conn, &keys(&["A-1"]), now()).unwrap(),
        keys(&["A-1"])
    );
    let v = view(&conn, &keys(&["A-1"]), None).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].pulled_at, None);
    assert_eq!(v[0].estimate_seconds, None);
    assert!(v[0].people.is_empty());
    assert_eq!(v[0].logged_seconds, 0);
}

#[test]
fn stale_keys_empty_input_is_empty_and_only_stale_ones_returned() {
    let conn = open_memory().unwrap();
    store_ticket(&conn, "A-1", None, &[], now()).unwrap();
    assert!(stale_keys(&conn, &[], now()).unwrap().is_empty());
    // returning every key regardless of age would include A-1
    assert_eq!(
        stale_keys(&conn, &keys(&["A-1", "B-2"]), now()).unwrap(),
        keys(&["B-2"])
    );
}

#[test]
fn store_replaces_the_tickets_rows_and_leaves_siblings_alone() {
    let conn = open_memory().unwrap();
    store_ticket(
        &conn,
        "A-1",
        Some(7200),
        &[log("1", "u1", "Ann", "2026-10-01", 3600)],
        now(),
    )
    .unwrap();
    store_ticket(
        &conn,
        "A-10",
        None,
        &[log("2", "u1", "Ann", "2026-10-01", 600)],
        now(),
    )
    .unwrap();
    store_ticket(
        &conn,
        "A-1",
        Some(9000),
        &[log("3", "u2", "Bo", "2026-10-02", 1800)],
        now(),
    )
    .unwrap();
    let v = view(&conn, &keys(&["A-1", "A-10"]), None).unwrap();
    // appending instead of replacing would keep Ann's 3600 on A-1
    assert_eq!(v[0].estimate_seconds, Some(9000));
    assert_eq!(v[0].logged_seconds, 1800);
    assert_eq!(v[0].people.len(), 1);
    assert_eq!(v[0].people[0].name, "Bo");
    // a LIKE 'A-1%' delete would wipe the prefix sibling A-10
    assert_eq!(v[1].logged_seconds, 600);
}

#[test]
fn restore_with_no_estimate_stores_none_not_zero() {
    let conn = open_memory().unwrap();
    store_ticket(&conn, "A-1", Some(3600), &[], now()).unwrap();
    store_ticket(&conn, "A-1", None, &[], now()).unwrap();
    assert_eq!(
        view(&conn, &keys(&["A-1"]), None).unwrap()[0].estimate_seconds,
        None
    );
}

#[test]
fn view_groups_per_person_per_day_you_first_then_seconds_desc() {
    let conn = open_memory().unwrap();
    let logs = [
        log("1", "u-bo", "Bo", "2026-10-02", 5000),
        log("2", "u-bo", "Bo", "2026-10-01", 1000),
        log("3", "u-bo", "Bo", "2026-10-02", 400),
        log("4", "u-me", "Me", "2026-10-03", 600),
        log("5", "u-cy", "Cy", "2026-10-01", 9000),
    ];
    store_ticket(&conn, "A-1", Some(20000), &logs, now()).unwrap();
    let t = &view(&conn, &keys(&["A-1"]), Some("u-me")).unwrap()[0];
    let order: Vec<_> = t.people.iter().map(|p| p.name.as_str()).collect();
    // sorting by seconds alone would put Cy first; folding would lose Bo/Cy separation
    assert_eq!(order, ["Me", "Cy", "Bo"]);
    assert!(t.people[0].is_you && !t.people[1].is_you && !t.people[2].is_you);
    let bo = &t.people[2];
    assert_eq!(bo.account_id, "u-bo");
    assert_eq!(bo.seconds, 6400);
    // days merged and ascending
    assert_eq!(
        bo.by_day,
        vec![(d("2026-10-01"), 1000), (d("2026-10-02"), 5400)]
    );
    // §5: logged is the sum over people
    assert_eq!(t.logged_seconds, 600 + 9000 + 6400);
    assert_eq!(t.pulled_at.as_deref(), Some("2026-10-09T12:00:00+00:00"));
    assert_eq!(t.error, None);
}

#[test]
fn nobody_is_you_without_an_account() {
    let conn = open_memory().unwrap();
    store_ticket(
        &conn,
        "A-1",
        None,
        &[log("1", "u-me", "Me", "2026-10-01", 60)],
        now(),
    )
    .unwrap();
    // None must not match an empty-string account or the first person
    assert!(!view(&conn, &keys(&["A-1"]), None).unwrap()[0].people[0].is_you);
    assert!(!view(&conn, &keys(&["A-1"]), Some("u-other")).unwrap()[0].people[0].is_you);
}

#[test]
fn view_keeps_requested_key_order() {
    let conn = open_memory().unwrap();
    store_ticket(&conn, "A-1", None, &[], now()).unwrap();
    store_ticket(&conn, "B-2", None, &[], now()).unwrap();
    let v = view(&conn, &keys(&["B-2", "A-1"]), None).unwrap();
    assert_eq!(
        v.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
        ["B-2", "A-1"]
    );
}

#[test]
fn mark_stale_makes_fresh_keys_stale_without_deleting_numbers() {
    let conn = open_memory().unwrap();
    store_ticket(
        &conn,
        "A-1",
        Some(7200),
        &[log("1", "u1", "Ann", "2026-10-01", 3600)],
        now(),
    )
    .unwrap();
    store_ticket(&conn, "B-2", None, &[], now()).unwrap();
    mark_stale(&conn, &keys(&["A-1"])).unwrap();
    // marking every key (or none) would fail one of these two
    assert_eq!(
        stale_keys(&conn, &keys(&["A-1", "B-2"]), now()).unwrap(),
        keys(&["A-1"])
    );
    // a delete would blank the numbers shown while refetching
    let t = &view(&conn, &keys(&["A-1"]), None).unwrap()[0];
    assert_eq!(t.estimate_seconds, Some(7200));
    assert_eq!(t.logged_seconds, 3600);
    // an unfetched key is not invented by marking it
    mark_stale(&conn, &keys(&["C-3"])).unwrap();
    assert_eq!(
        view(&conn, &keys(&["C-3"]), None).unwrap()[0].pulled_at,
        None
    );
}

#[test]
fn day_ticket_keys_are_distinct_non_personal_and_of_that_day() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-09", Some("A-2"), 0);
    block(&conn, "2026-10-09", Some("A-2"), 0);
    block(&conn, "2026-10-09", Some("A-1"), 0);
    block(&conn, "2026-10-09", Some("P-9"), 1);
    block(&conn, "2026-10-09", None, 0);
    block(&conn, "2026-10-09", Some(""), 0);
    block(&conn, "2026-10-08", Some("Z-9"), 0);
    // no DISTINCT -> A-2 twice; no personal filter -> P-9; no day filter -> Z-9;
    // no NULL/empty filter -> "" present
    assert_eq!(
        day_ticket_keys(&conn, d("2026-10-09")).unwrap(),
        keys(&["A-1", "A-2"])
    );
}
