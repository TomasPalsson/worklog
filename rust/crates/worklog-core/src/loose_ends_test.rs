use super::*;
use crate::db::open_memory;
use crate::time_leaks_contract::{ExportGap, TempoGap, TempoGapReason, UnbilledDay};
use rusqlite::params;

const TODAY: &str = "2026-10-11";
const START: u32 = 20;

struct Row<'a> {
    day: &'a str,
    start: &'a str,
    seconds: i64,
    jira: Option<&'a str>,
    tempo: Option<&'a str>,
    exported: Option<&'a str>,
    personal: bool,
    ignored: bool,
}

fn open(day: &str) -> Row<'_> {
    Row {
        day,
        start: "09:00",
        seconds: 1800,
        jira: Some("ABC-1"),
        tempo: None,
        exported: None,
        personal: false,
        ignored: false,
    }
}

fn seed(conn: &Connection, r: Row) {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description, jira_issue,
                             tempo_worklog_id, exported_at, is_personal, ignored_at)
         VALUES (?1, ?1 || 'T' || ?2 || ':00+00:00', ?1 || 'T' || ?2 || ':00+00:00', ?3, 'did it',
                 ?4, ?5, ?6, ?7, CASE WHEN ?8 THEN '2026-10-01T00:00:00Z' END)",
        params![r.day, r.start, r.seconds, r.jira, r.tempo, r.exported, r.personal, r.ignored],
    )
    .unwrap();
}

fn remote(conn: &Connection, id: &str, day: &str, jira: &str, seconds: i64, pulled_at: &str) {
    conn.execute(
        "INSERT INTO tempo_remote_worklogs
            (tempo_worklog_id, day, issue_id, jira_issue, seconds, owner, pulled_at)
         VALUES (?1, ?2, 1, ?3, ?4, 'outside', ?5)",
        params![id, day, jira, seconds, pulled_at],
    )
    .unwrap();
}

fn dismiss(conn: &Connection, day: &str, reason: &str) {
    conn.execute(
        "INSERT INTO tempo_day_dismissals (day, reason, dismissed_at) VALUES (?1, ?2, ?1)",
        params![day, reason],
    )
    .unwrap();
}

fn run(conn: &Connection) -> UnbilledDays {
    for_cycle_with(conn, TODAY, START).unwrap()
}

fn only_day(conn: &Connection) -> UnbilledDay {
    let mut days = run(conn).days;
    assert_eq!(days.len(), 1, "{days:?}");
    days.remove(0)
}

fn gap(jira: &str, seconds: i64, in_tempo: i64, reason: TempoGapReason) -> TempoGap {
    TempoGap {
        jira_issue: jira.into(),
        seconds,
        in_tempo_seconds: in_tempo,
        reason,
    }
}

fn day_list(conn: &Connection, today: &str) -> Vec<String> {
    let le = for_cycle_with(conn, today, START).unwrap();
    le.days.into_iter().map(|d| d.day).collect()
}

const PULLED: &str = "2026-10-09T14:13:33Z";

#[test]
fn empty_db_returns_the_cycle_bounds_and_no_days() {
    let conn = open_memory().unwrap();
    let le = run(&conn);
    assert_eq!(le.cycle_start, "2026-09-20");
    assert_eq!(le.cycle_end, "2026-10-19");
    assert!(le.days.is_empty());
    // catches: a made-up pull time when Tempo was never pulled
    assert_eq!(le.tempo_pulled_at, None);
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
    assert_eq!(day_list(&conn, "2026-10-19"), ["2026-09-20", "2026-10-19"]);
}

// catches a window one day too wide on either side
#[test]
fn days_just_outside_the_cycle_are_left_out() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-09-19"));
    seed(&conn, open("2026-10-20"));
    assert!(day_list(&conn, "2026-10-19").is_empty());
}

// catches: listing days after today (the cycle end is in the future)
#[test]
fn future_days_are_left_out() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-10-11"));
    seed(&conn, open("2026-10-12"));
    assert_eq!(day_list(&conn, TODAY), ["2026-10-11"]);
}

// catches: unordered output
#[test]
fn days_come_oldest_first() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-10-05"));
    seed(&conn, open("2026-10-02"));
    assert_eq!(day_list(&conn, TODAY), ["2026-10-02", "2026-10-05"]);
}

// catches: a manual 'outside' Tempo entry ignored because no block carries its id
#[test]
fn outside_entry_covering_the_line_is_no_gap() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            exported: Some("x"),
            seconds: 3600,
            ..open("2026-10-01")
        },
    );
    remote(&conn, "9", "2026-10-01", "ABC-1", 3600, PULLED);
    assert!(run(&conn).days.is_empty());
}

// catches: Tempo's round-up compared with == instead of >=
#[test]
fn tempo_rounded_above_the_line_is_no_gap() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            exported: Some("x"),
            seconds: 6120,
            ..open("2026-10-01")
        },
    );
    remote(&conn, "9", "2026-10-01", "ABC-1", 7200, PULLED);
    assert!(run(&conn).days.is_empty());
}

// catches: remote matched on jira_issue only, when the pull could not name the issue
#[test]
fn remote_row_without_a_key_matches_through_jira_tickets() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id) VALUES ('ABC-1', 's', '1')",
        [],
    )
    .unwrap();
    seed(
        &conn,
        Row {
            exported: Some("x"),
            ..open("2026-10-01")
        },
    );
    conn.execute(
        "INSERT INTO tempo_remote_worklogs
            (tempo_worklog_id, day, issue_id, jira_issue, seconds, owner, pulled_at)
         VALUES ('9', '2026-10-01', 1, NULL, 1800, 'outside', ?1)",
        [PULLED],
    )
    .unwrap();
    assert!(run(&conn).days.is_empty());
}

// catches: a dismissed day still reported as a Tempo gap, or the reason dropped
#[test]
fn dismissed_day_has_no_tempo_gap_and_carries_the_reason() {
    let conn = open_memory().unwrap();
    seed(&conn, open("2026-10-01"));
    dismiss(&conn, "2026-10-01", "Logged in mirres");
    let d = only_day(&conn);
    assert_eq!(d.dismissed.as_deref(), Some("Logged in mirres"));
    assert!(d.tempo.is_empty());
    // catches: dismissal hiding the export gap too
    assert_eq!(d.export, Some(ExportGap { seconds: 1800 }));
}

// catches: a dismissed day with nothing else open still listed
#[test]
fn dismissed_day_with_nothing_else_is_left_out() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            exported: Some("x"),
            ..open("2026-10-01")
        },
    );
    dismiss(&conn, "2026-10-01", "Left earlier");
    assert!(run(&conn).days.is_empty());
}

fn seed_synced_4920(conn: &Connection) {
    seed(
        conn,
        Row {
            tempo: Some("250571"),
            exported: Some("x"),
            seconds: 4920,
            ..open("2026-10-02")
        },
    );
}

// catches: a synced id trusted although Tempo no longer has it
#[test]
fn id_missing_from_a_later_pull_is_deleted() {
    let conn = open_memory().unwrap();
    seed_synced_4920(&conn);
    remote(&conn, "250572", "2026-10-02", "ABC-1", 3600, PULLED);
    let d = only_day(&conn);
    assert_eq!(d.tempo, [gap("ABC-1", 4920, 3600, TempoGapReason::Deleted)]);
}

// catches: calling an id deleted when the day was never pulled (Tempo may just be stale)
#[test]
fn synced_line_on_an_unpulled_day_is_covered() {
    let conn = open_memory().unwrap();
    seed_synced_4920(&conn);
    assert!(run(&conn).days.is_empty());
}

// catches: a pull made before the line was sent read as proof of deletion
#[test]
fn pull_older_than_the_auto_send_is_not_deleted() {
    let conn = open_memory().unwrap();
    seed_synced_4920(&conn);
    remote(
        &conn,
        "250572",
        "2026-10-02",
        "ABC-1",
        3600,
        "2026-10-03T08:00:00Z",
    );
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, updated_at, auto_sent_at)
         VALUES ('2026-10-02', 'ABC-1', 'x', '2026-10-03T09:00:00+00:00')",
        [],
    )
    .unwrap();
    assert!(run(&conn).days.is_empty());
}

// catches: a pull taken during the day itself (before the 17:00 send) read as proof of deletion
#[test]
fn pull_before_the_day_ended_is_not_deleted() {
    let conn = open_memory().unwrap();
    seed_synced_4920(&conn);
    remote(
        &conn,
        "250572",
        "2026-10-02",
        "ABC-1",
        3600,
        "2026-10-02T12:00:00Z",
    );
    assert!(run(&conn).days.is_empty());
}

// catches: a partial Tempo line reported as not sent
#[test]
fn partial_tempo_is_short() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            exported: Some("x"),
            seconds: 3600,
            ..open("2026-10-03")
        },
    );
    remote(&conn, "9", "2026-10-03", "ABC-1", 1800, PULLED);
    assert_eq!(
        only_day(&conn).tempo,
        [gap("ABC-1", 3600, 1800, TempoGapReason::Short)]
    );
}

// catches: an unsent line hidden, or `IS NULL` instead of COALESCE(.., '') = ''
#[test]
fn nothing_anywhere_is_not_sent_including_empty_string_ids() {
    let conn = open_memory().unwrap();
    seed(
        &conn,
        Row {
            tempo: Some(""),
            exported: Some(""),
            ..open("2026-10-03")
        },
    );
    let d = only_day(&conn);
    assert_eq!(d.tempo, [gap("ABC-1", 1800, 0, TempoGapReason::NotSent)]);
    assert_eq!(d.export, Some(ExportGap { seconds: 1800 }));
    assert_eq!(d.dismissed, None);
    assert_eq!(d.ticketless_seconds, 0);
}

// catches: one synced block hiding its unsynced sibling on the same line
#[test]
fn line_with_one_unsynced_block_is_a_gap() {
    let conn = open_memory().unwrap();
    for (start, tempo) in [("09:00", Some("1")), ("11:00", None)] {
        seed(
            &conn,
            Row {
                start,
                tempo,
                exported: Some("x"),
                ..open("2026-10-03")
            },
        );
    }
    assert_eq!(
        only_day(&conn).tempo,
        [gap("ABC-1", 3600, 0, TempoGapReason::NotSent)]
    );
}

// catches: a naive sum of overlapping blocks (would be 7200)
#[test]
fn overlapping_blocks_count_once() {
    let conn = open_memory().unwrap();
    for start in ["09:00", "09:30"] {
        seed(
            &conn,
            Row {
                start,
                seconds: 3600,
                ..open("2026-10-03")
            },
        );
    }
    let d = only_day(&conn);
    assert_eq!(d.tempo, [gap("ABC-1", 5400, 0, TempoGapReason::NotSent)]);
    assert_eq!(d.export, Some(ExportGap { seconds: 5400 }));
}

// catches: ticketless work invisible, or a day with only ticketless work dropped
#[test]
fn ticketless_only_day_is_listed_with_union_seconds() {
    let conn = open_memory().unwrap();
    for (start, jira) in [("09:00", None), ("09:15", Some(""))] {
        seed(
            &conn,
            Row {
                start,
                jira,
                ..open("2026-10-10")
            },
        );
    }
    let d = only_day(&conn);
    assert_eq!(d.ticketless_seconds, 2700);
    assert!(d.tempo.is_empty());
    // catches: ticketless blocks counted as an export gap (the invoice needs a ticket line first)
    assert_eq!(d.export, None);
}

// catches: export gap summing every block, or counting exported ones
#[test]
fn export_gap_is_the_union_of_unexported_ticketed_blocks() {
    let conn = open_memory().unwrap();
    for (start, jira, exported) in [
        ("09:00", "ABC-1", None),
        ("09:30", "ABC-2", None),
        ("13:00", "ABC-1", Some("x")),
    ] {
        seed(
            &conn,
            Row {
                start,
                jira: Some(jira),
                tempo: Some("1"),
                exported,
                seconds: 3600,
                ..open("2026-10-04")
            },
        );
    }
    let d = only_day(&conn);
    assert_eq!(d.export, Some(ExportGap { seconds: 5400 }));
    assert!(d.tempo.is_empty());
}

// catches: the pull time taken from one row instead of the latest
#[test]
fn tempo_pulled_at_is_the_latest_pull() {
    let conn = open_memory().unwrap();
    remote(
        &conn,
        "1",
        "2026-09-01",
        "ABC-1",
        60,
        "2026-09-02T10:00:00Z",
    );
    remote(&conn, "2", "2026-10-01", "ABC-1", 60, PULLED);
    remote(
        &conn,
        "3",
        "2026-10-02",
        "ABC-1",
        60,
        "2026-10-05T10:00:00Z",
    );
    assert_eq!(run(&conn).tempo_pulled_at.as_deref(), Some(PULLED));
}

// catches dropping either the personal or the ignored filter
#[test]
fn personal_and_ignored_blocks_are_left_out() {
    let conn = open_memory().unwrap();
    for jira in [Some("ABC-1"), None] {
        seed(
            &conn,
            Row {
                personal: true,
                jira,
                ..open("2026-10-01")
            },
        );
        seed(
            &conn,
            Row {
                ignored: true,
                jira,
                ..open("2026-10-01")
            },
        );
    }
    assert!(run(&conn).days.is_empty());
}

// spec s5: < 200 ms on a 2,000-block fixture; catches a full-scan or per-row query regression
#[test]
fn query_over_2000_blocks_takes_under_200ms() {
    let conn = open_memory().unwrap();
    conn.execute_batch("BEGIN").unwrap();
    for i in 0..2000 {
        // Every block lands inside the cycle, on or before TODAY (2026-09-20..=2026-10-11).
        let d = i % 22;
        let day = if d <= 10 {
            format!("2026-09-{}", 20 + d)
        } else {
            format!("2026-10-{:02}", d - 10)
        };
        let jira = format!("ABC-{}", i % 7);
        let tempo = (i % 3 == 0).then(|| i.to_string());
        seed(
            &conn,
            Row {
                jira: Some(&jira),
                tempo: tempo.as_deref(),
                ..open(&day)
            },
        );
        if i % 5 == 0 {
            remote(&conn, &format!("r{i}"), &day, &jira, 600, PULLED);
        }
    }
    conn.execute_batch("COMMIT").unwrap();
    let t = std::time::Instant::now();
    let le = run(&conn);
    let took = t.elapsed();
    assert_eq!(le.days.len(), 22, "every seeded day is open");
    assert!(took.as_millis() < 200, "took {took:?}");
}
