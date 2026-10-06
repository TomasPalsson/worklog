use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::repo;
use crate::verdict_contract::{RankedOption, Ranking};

fn row(kind: DecisionKind, source: DecisionSource, subject: &str, at: &str) -> DecisionRow {
    DecisionRow {
        kind,
        source,
        subject: subject.into(),
        state_json: "{}".into(),
        options: vec!["a".into(), "b".into()],
        ranking: None,
        chosen: None,
        previous: None,
        decided_at: at.into(),
    }
}

fn owner_fix(subject: &str, to: &str, at: &str) -> DecisionRow {
    let mut r = row(DecisionKind::Project, DecisionSource::Owner, subject, at);
    r.chosen = Some(to.into());
    r
}

fn event(conn: &Connection, sid: &str, started: &str, title: &str) -> i64 {
    repo::upsert_event(conn, &Event::minimal("slack", sid, started, title)).unwrap()
}

#[test]
fn record_round_trips_every_field() {
    let conn = open_memory().unwrap();
    let mut r = row(
        DecisionKind::Ticket,
        DecisionSource::Verdict,
        "7",
        "2026-10-05T10:00:00Z",
    );
    r.state_json = r#"{"t":"x"}"#.into();
    r.ranking = Some(Ranking {
        ranking: vec![RankedOption {
            id: "a".into(),
            probability: 0.75,
        }],
        abstain: 0.125,
        agreed: true,
    });
    r.chosen = Some("a".into());
    r.previous = Some("b".into());
    let id = record(&conn, &r).unwrap();
    assert!(id > 0);
    // catches: dropping a column on write or read
    assert_eq!(
        latest_for(&conn, DecisionKind::Ticket, "7").unwrap(),
        Some(r)
    );
}

#[test]
fn record_returns_distinct_ids() {
    let conn = open_memory().unwrap();
    let r = row(
        DecisionKind::Project,
        DecisionSource::Owner,
        "1",
        "2026-10-05T10:00:00Z",
    );
    // catches: returning a constant / row count instead of last_insert_rowid
    assert_ne!(record(&conn, &r).unwrap(), record(&conn, &r).unwrap());
}

#[test]
fn bad_ranking_json_reads_as_no_ranking() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO verdict_decisions (kind, source, subject, state_json, options, ranking, decided_at)
         VALUES ('project', 'verdict', '1', '{}', '[]', 'not json', '2026-10-05T10:00:00Z')",
        [],
    )
    .unwrap();
    // catches: unwrap/? on a malformed ranking column (panic or whole-read error)
    let got = latest_for(&conn, DecisionKind::Project, "1")
        .unwrap()
        .unwrap();
    assert_eq!(got.ranking, None);
}

#[test]
fn latest_for_missing_is_none() {
    let conn = open_memory().unwrap();
    // catches: erroring on no rows
    assert_eq!(latest_for(&conn, DecisionKind::Project, "1").unwrap(), None);
}

#[test]
fn latest_for_picks_newest_decided_at_not_newest_insert() {
    let conn = open_memory().unwrap();
    record(&conn, &owner_fix("1", "new", "2026-10-05T12:00:00Z")).unwrap();
    record(&conn, &owner_fix("1", "old", "2026-10-05T09:00:00Z")).unwrap();
    // catches: ordering by id, or taking the first row
    let got = latest_for(&conn, DecisionKind::Project, "1")
        .unwrap()
        .unwrap();
    assert_eq!(got.chosen.as_deref(), Some("new"));
}

#[test]
fn latest_for_same_instant_prefers_later_insert() {
    let conn = open_memory().unwrap();
    record(&conn, &owner_fix("1", "first", "2026-10-05T09:00:00Z")).unwrap();
    record(&conn, &owner_fix("1", "second", "2026-10-05T09:00:00Z")).unwrap();
    // catches: a tie returning the earlier row
    let got = latest_for(&conn, DecisionKind::Project, "1")
        .unwrap()
        .unwrap();
    assert_eq!(got.chosen.as_deref(), Some("second"));
}

#[test]
fn latest_for_matches_kind_and_whole_subject() {
    let conn = open_memory().unwrap();
    record(&conn, &owner_fix("10", "ten", "2026-10-05T09:00:00Z")).unwrap();
    let mut other_kind = owner_fix("1", "ticket", "2026-10-05T09:00:00Z");
    other_kind.kind = DecisionKind::Ticket;
    record(&conn, &other_kind).unwrap();
    // catches: LIKE 'subject%' prefix match, and ignoring kind
    assert_eq!(latest_for(&conn, DecisionKind::Project, "1").unwrap(), None);
    assert_eq!(
        latest_for(&conn, DecisionKind::Ticket, "1")
            .unwrap()
            .unwrap()
            .chosen
            .as_deref(),
        Some("ticket")
    );
}

#[test]
fn list_since_is_inclusive_of_the_bound_and_oldest_first() {
    let conn = open_memory().unwrap();
    record(&conn, &owner_fix("3", "c", "2026-10-05T12:00:00Z")).unwrap();
    record(&conn, &owner_fix("1", "a", "2026-10-04T23:59:59Z")).unwrap();
    record(&conn, &owner_fix("2", "b", "2026-10-05T00:00:00Z")).unwrap();
    let got = list_since(&conn, DecisionKind::Project, "2026-10-05T00:00:00Z").unwrap();
    let subjects: Vec<_> = got.iter().map(|r| r.subject.as_str()).collect();
    // catches: > for >= (drops "2"), no lower bound (adds "1"), unordered output
    assert_eq!(subjects, ["2", "3"]);
}

#[test]
fn list_since_filters_kind() {
    let conn = open_memory().unwrap();
    let mut t = owner_fix("1", "a", "2026-10-05T12:00:00Z");
    t.kind = DecisionKind::LineText;
    record(&conn, &t).unwrap();
    // catches: ignoring the kind argument
    assert!(
        list_since(&conn, DecisionKind::Project, "2026-10-05T00:00:00Z")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn examples_for_returns_titles_newest_first_up_to_limit() {
    let conn = open_memory().unwrap();
    for (i, t) in ["one", "two", "three"].iter().enumerate() {
        let id = event(&conn, t, "2026-10-05T08:00:00Z", t);
        let at = format!("2026-10-05T1{i}:00:00Z");
        record(&conn, &owner_fix(&id.to_string(), "proj", &at)).unwrap();
    }
    // catches: oldest-first order, and a limit that is off by one
    assert_eq!(examples_for(&conn, "proj", 2).unwrap(), ["three", "two"]);
    assert_eq!(
        examples_for(&conn, "proj", 3).unwrap(),
        ["three", "two", "one"]
    );
    assert_eq!(examples_for(&conn, "proj", 4).unwrap().len(), 3);
    assert!(examples_for(&conn, "proj", 0).unwrap().is_empty());
}

#[test]
fn examples_for_counts_only_owner_corrections_to_exactly_that_folder() {
    let conn = open_memory().unwrap();
    let a = event(&conn, "a", "2026-10-05T08:00:00Z", "owner to proj");
    let b = event(&conn, "b", "2026-10-05T08:00:00Z", "verdict to proj");
    let c = event(&conn, "c", "2026-10-05T08:00:00Z", "owner to proj-two");
    let d = event(&conn, "d", "2026-10-05T08:00:00Z", "owner ticket");
    let at = "2026-10-05T10:00:00Z";
    record(&conn, &owner_fix(&a.to_string(), "proj", at)).unwrap();
    let mut v = owner_fix(&b.to_string(), "proj", at);
    v.source = DecisionSource::Verdict;
    record(&conn, &v).unwrap();
    record(&conn, &owner_fix(&c.to_string(), "proj-two", at)).unwrap();
    let mut t = owner_fix(&d.to_string(), "proj", at);
    t.kind = DecisionKind::Ticket;
    record(&conn, &t).unwrap();
    // catches: including Verdict rows, prefix folder match, ignoring kind
    assert_eq!(examples_for(&conn, "proj", 5).unwrap(), ["owner to proj"]);
}

#[test]
fn examples_for_skips_a_correction_whose_event_is_gone() {
    let conn = open_memory().unwrap();
    record(&conn, &owner_fix("9999", "proj", "2026-10-05T10:00:00Z")).unwrap();
    // catches: erroring or yielding an empty title for a dangling subject
    assert!(examples_for(&conn, "proj", 5).unwrap().is_empty());
}

fn verdict_row(conn: &Connection, kind: DecisionKind, id: i64, chosen: Option<&str>) {
    let mut r = row(
        kind,
        DecisionSource::Verdict,
        &id.to_string(),
        "2026-10-05T10:00:00Z",
    );
    r.chosen = chosen.map(str::to_owned);
    record(conn, &r).unwrap();
}

#[test]
fn unchecked_count_counts_loose_events_without_a_verdict_row() {
    let conn = open_memory().unwrap();
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 0);
    let a = event(&conn, "a", "2026-10-05T08:00:00+00:00", "a");
    event(&conn, "b", "2026-10-05T09:00:00+00:00", "b");
    event(&conn, "c", "2026-10-05T10:00:00+00:00", "c");
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 3);
    // an abstained Verdict row still means "checked"
    verdict_row(&conn, DecisionKind::Project, a, None);
    // catches: not joining the log, or counting only answered rows
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 2);
}

#[test]
fn unchecked_count_ignores_owner_rows_and_other_kinds() {
    let conn = open_memory().unwrap();
    let a = event(&conn, "a", "2026-10-05T08:00:00+00:00", "a");
    record(
        &conn,
        &owner_fix(&a.to_string(), "proj", "2026-10-05T10:00:00Z"),
    )
    .unwrap();
    verdict_row(&conn, DecisionKind::Ticket, a, Some("X-1"));
    // catches: any row for the subject counting as checked
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 1);
}

#[test]
fn unchecked_count_excludes_labelled_events() {
    let conn = open_memory().unwrap();
    let a = event(&conn, "a", "2026-10-05T08:00:00+00:00", "a");
    conn.execute(
        "UPDATE events SET label_origin = 'fix', project_path = 'p' WHERE id = ?1",
        [a],
    )
    .unwrap();
    // catches: counting events that already carry a project
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 0);
}

#[test]
fn unchecked_count_day_window_is_start_inclusive_end_exclusive() {
    let conn = open_memory().unwrap();
    event(&conn, "before", "2026-10-04T23:59:59+00:00", "x");
    event(&conn, "start", "2026-10-05T00:00:00+00:00", "x");
    event(&conn, "last", "2026-10-05T23:59:59+00:00", "x");
    event(&conn, "next", "2026-10-06T00:00:00+00:00", "x");
    // catches: >/<= swaps at either edge
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 2);
}

#[test]
fn unchecked_count_ignores_non_routable_sources() {
    let conn = open_memory().unwrap();
    for src in ["git", "claude", "jira"] {
        repo::upsert_event(
            &conn,
            &Event::minimal(src, "x", "2026-10-05T08:00:00+00:00", "x"),
        )
        .unwrap();
    }
    // catches: counting events routing can never label
    assert_eq!(unchecked_count(&conn, "2026-10-05").unwrap(), 0);
}

#[test]
fn unchecked_count_rejects_a_malformed_day() {
    let conn = open_memory().unwrap();
    // catches: swallowing the parse error into 0
    assert!(unchecked_count(&conn, "not-a-day").is_err());
}

#[test]
fn log_kind_and_source_are_constrained() {
    let conn = open_memory().unwrap();
    let insert = |kind: &str, source: &str| {
        conn.execute(
            "INSERT INTO verdict_decisions (kind, source, subject, state_json, options, decided_at)
             VALUES (?1, ?2, 's', '{}', '[]', 'x')",
            [kind, source],
        )
    };
    // catches: missing CHECK constraints
    assert!(insert("bogus", "owner").is_err());
    assert!(insert("project", "bogus").is_err());
    assert!(insert("line_text", "verdict").is_ok());
}

#[test]
fn new_columns_exist_and_check_status_is_constrained() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO tempo_line_texts (day, jira_issue, updated_at, check_status, auto_sent_at, confirmed_at, send_error)
         VALUES ('2026-10-05', 'A-1', 'x', 'needs_look', 'x', 'x', 'x')",
        [],
    )
    .unwrap();
    conn.execute("UPDATE events SET verdict_ranking = NULL", [])
        .unwrap();
    // catches: no CHECK on check_status
    assert!(conn
        .execute("UPDATE tempo_line_texts SET check_status = 'bogus'", [])
        .is_err());
}

#[test]
fn migrate_adds_the_new_columns_to_an_older_database() {
    let conn = open_memory().unwrap();
    conn.execute_batch(
        "ALTER TABLE events DROP COLUMN verdict_ranking;
         ALTER TABLE tempo_line_texts DROP COLUMN check_status;
         ALTER TABLE tempo_line_texts DROP COLUMN auto_sent_at;
         ALTER TABLE tempo_line_texts DROP COLUMN confirmed_at;
         ALTER TABLE tempo_line_texts DROP COLUMN send_error;",
    )
    .unwrap();
    crate::db::migrate(&conn).unwrap();
    crate::db::migrate(&conn).unwrap();
    // catches: a missing ensure_* for an upgraded db, or a non-idempotent ALTER
    conn.execute("UPDATE events SET verdict_ranking = NULL", [])
        .unwrap();
    conn.execute(
        "UPDATE tempo_line_texts SET check_status = NULL, auto_sent_at = NULL, confirmed_at = NULL, send_error = NULL",
        [],
    )
    .unwrap();
}
