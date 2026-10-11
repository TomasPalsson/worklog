use super::*;
use crate::db::open_memory;
use rusqlite::params;

const TODAY: &str = "2026-10-11";
const START: u32 = 20;

#[derive(Default)]
struct Row<'a> {
    day: &'a str,
    jira: Option<&'a str>,
    tempo: Option<&'a str>,
    exported: Option<&'a str>,
    personal: bool,
    ignored: bool,
}

fn seed(conn: &Connection, r: Row) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue,
                             tempo_worklog_id, exported_at, is_personal, ignored_at)
         VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00', 1800, 'did it', ?2, ?3, ?4, ?5,
                 CASE WHEN ?6 THEN '2026-10-01T00:00:00Z' END)",
        params![r.day, r.jira, r.tempo, r.exported, r.personal, r.ignored],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn open(day: &str) -> Row<'_> {
    Row {
        day,
        jira: Some("ABC-1"),
        ..Default::default()
    }
}

fn days(conn: &Connection, today: &str) -> Vec<String> {
    for_cycle_with(conn, today, START)
        .unwrap()
        .items
        .into_iter()
        .map(|i| i.day)
        .collect()
}

#[test]
fn empty_db_returns_the_cycle_bounds_and_no_items() {
    let conn = open_memory().unwrap();
    let le = for_cycle_with(&conn, TODAY, START).unwrap();
    assert_eq!(le.cycle_start, "2026-09-20");
    assert_eq!(le.cycle_end, "2026-10-19");
    assert!(le.items.is_empty());
}

// catches a window that ignores the start day (calendar month) or uses <= for the next cycle
#[test]
fn today_on_the_start_day_opens_the_next_cycle() {
    let conn = open_memory().unwrap();
    let le = for_cycle_with(&conn, "2026-10-20", START).unwrap();
    assert_eq!(
        (le.cycle_start.as_str(), le.cycle_end.as_str()),
        ("2026-10-20", "2026-11-19")
    );
    let le = for_cycle_with(&conn, "2026-10-19", START).unwrap();
    assert_eq!(
        (le.cycle_start.as_str(), le.cycle_end.as_str()),
        ("2026-09-20", "2026-10-19")
    );
}

// catches > for >= at the start and < for <= at the end
#[test]
fn first_and_last_day_of_the_cycle_are_inside() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-09-20"));
    seed(&conn, open("2026-10-19"));
    assert_eq!(days(&conn, TODAY), ["2026-09-20", "2026-10-19"]);
}

// catches a window one day too wide on either side
#[test]
fn days_just_outside_the_cycle_are_left_out() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-09-19"));
    seed(&conn, open("2026-10-20"));
    assert!(days(&conn, TODAY).is_empty());
}

// catches a filter that only checks one of the two ids, or a fixed `missing` list
#[test]
fn missing_lists_each_combination() {
    let conn = open_memory().unwrap();
    let both = seed(&conn, open("2026-10-01"));
    let tempo_only = seed(
        &conn,
        Row {
            exported: Some("2026-10-02"),
            ..open("2026-10-02")
        },
    );
    let export_only = seed(
        &conn,
        Row {
            tempo: Some("w1"),
            ..open("2026-10-03")
        },
    );
    seed(
        &conn,
        Row {
            tempo: Some("w2"),
            exported: Some("2026-10-04"),
            ..open("2026-10-04")
        },
    );
    let got: Vec<(i64, Vec<LooseEndMissing>)> = for_cycle_with(&conn, TODAY, START)
        .unwrap()
        .items
        .into_iter()
        .map(|i| (i.block_id, i.missing))
        .collect();
    use LooseEndMissing::{Export, Tempo};
    assert_eq!(
        got,
        [
            (both, vec![Tempo, Export]),
            (tempo_only, vec![Tempo]),
            (export_only, vec![Export]),
        ]
    );
}

// catches `IS NULL` instead of COALESCE(.., '') = ''
#[test]
fn empty_string_ids_count_as_missing() {
    let conn = open_memory().unwrap();
    let id = seed(
        &conn,
        Row {
            tempo: Some(""),
            exported: Some(""),
            ..open("2026-10-01")
        },
    );
    let items = for_cycle_with(&conn, TODAY, START).unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].block_id, id);
    assert_eq!(
        items[0].missing,
        [LooseEndMissing::Tempo, LooseEndMissing::Export]
    );
}

// catches dropping any one of the personal, ignored, or ticketed filters
#[test]
fn personal_ignored_and_unticketed_blocks_are_left_out() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            personal: true,
            ..open("2026-10-01")
        },
    );
    seed(
        &conn,
        Row {
            ignored: true,
            ..open("2026-10-01")
        },
    );
    seed(
        &conn,
        Row {
            jira: None,
            ..open("2026-10-01")
        },
    );
    seed(
        &conn,
        Row {
            jira: Some(""),
            ..open("2026-10-01")
        },
    );
    assert!(days(&conn, TODAY).is_empty());
}

// catches unordered output and mis-mapped fields
#[test]
fn row_carries_the_block_fields_in_start_order() {
    let conn = open_memory().unwrap();
    let later = seed(&conn, open("2026-10-05"));
    let earlier = seed(&conn, open("2026-10-02"));
    let items = for_cycle_with(&conn, TODAY, START).unwrap().items;
    assert_eq!(
        items.iter().map(|i| i.block_id).collect::<Vec<_>>(),
        [earlier, later]
    );
    let first = &items[0];
    assert_eq!(first.day, "2026-10-02");
    assert_eq!(first.jira_issue, "ABC-1");
    assert_eq!(first.started_at, "2026-10-02T09:00:00+00:00");
    assert_eq!(first.ended_at, "2026-10-02T09:30:00+00:00");
    assert_eq!(first.seconds, 1800);
    assert_eq!(first.description.as_deref(), Some("did it"));
}

// spec s5: < 200 ms on a 2,000-block fixture; catches a full-scan or per-row query regression
#[test]
fn query_over_2000_blocks_takes_under_200ms() {
    let conn = open_memory().unwrap();
    conn.execute_batch("BEGIN").unwrap();
    for i in 0..2000 {
        let day = format!("2026-{:02}-{:02}", 1 + (i / 28) % 12, 1 + i % 28);
        seed(&conn, open(&day));
    }
    conn.execute_batch("COMMIT").unwrap();
    let t = std::time::Instant::now();
    let le = for_cycle_with(&conn, TODAY, START).unwrap();
    let took = t.elapsed();
    assert!(!le.items.is_empty());
    assert!(took.as_millis() < 200, "took {took:?}");
}
