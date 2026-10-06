use super::*;
use crate::clues_contract::LineTextOrigin;
use crate::db;
use anyhow::anyhow;
use rusqlite::params;

const DAY: &str = "2026-10-05";
const ISSUE: &str = "APRO-1";
const ISSUE_ID: i64 = 10001;

fn conn() -> Connection {
    let conn = db::open_memory().unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id) VALUES (?1, 's', ?2)",
        params![ISSUE, ISSUE_ID.to_string()],
    )
    .unwrap();
    conn
}

fn line(text: &str, seconds: i64) -> TempoLine {
    TempoLine {
        day: DAY.into(),
        jira_issue: ISSUE.into(),
        text: Some(text.into()),
        text_origin: Some(LineTextOrigin::Manual),
        fallback_text: "fallback".into(),
        union_seconds: seconds,
        hours_override_seconds: None,
        effective_seconds: seconds,
        check_status: None,
        billing: None,
    }
}

fn entry(id: &str, day: &str, issue_id: i64, seconds: i64) -> PulledWorklog {
    PulledWorklog {
        tempo_worklog_id: id.into(),
        day: day.into(),
        issue_id,
        seconds,
        description: format!("desc {id}"),
    }
}

fn yes(_: &str, t: &[String]) -> Result<Vec<bool>> {
    Ok(vec![true; t.len()])
}

fn already(id: &str) -> MatchVerdict {
    MatchVerdict::AlreadyInTempo {
        tempo_worklog_id: id.into(),
    }
}

#[test]
fn same_work_on_same_issue_and_day_is_already_in_tempo() {
    let c = conn();
    let got = check_line(
        &c,
        &line("fix", 7200),
        &[entry("77", DAY, ISSUE_ID, 7200)],
        yes,
    );
    assert_eq!(got, already("77"));
}

#[test]
fn tolerance_is_inclusive_at_30_minutes_and_exclusive_at_31() {
    let c = conn();
    let l = line("fix", 7200);
    // `<` instead of `<=` would fail the first two; a looser limit the last two.
    assert_eq!(
        check_line(&c, &l, &[entry("1", DAY, ISSUE_ID, 7200 + 1800)], yes),
        already("1")
    );
    assert_eq!(
        check_line(&c, &l, &[entry("1", DAY, ISSUE_ID, 7200 - 1800)], yes),
        already("1")
    );
    assert_eq!(
        check_line(&c, &l, &[entry("1", DAY, ISSUE_ID, 7200 + 1860)], yes),
        MatchVerdict::Different
    );
    // signed difference without abs() would call a much smaller entry the same
    assert_eq!(
        check_line(&c, &l, &[entry("1", DAY, ISSUE_ID, 7200 - 1860)], yes),
        MatchVerdict::Different
    );
}

#[test]
fn matcher_false_is_different() {
    let c = conn();
    let no = |_: &str, t: &[String]| Ok(vec![false; t.len()]);
    // ignoring the matcher and matching on hours alone would say already
    let got = check_line(
        &c,
        &line("fix", 3600),
        &[entry("1", DAY, ISSUE_ID, 3600)],
        no,
    );
    assert_eq!(got, MatchVerdict::Different);
}

#[test]
fn other_issue_other_day_or_no_entries_are_different() {
    let c = conn();
    let l = line("fix", 3600);
    // an entry filter missing the issue / day comparison would return already
    assert_eq!(
        check_line(&c, &l, &[entry("1", DAY, ISSUE_ID + 1, 3600)], yes),
        MatchVerdict::Different
    );
    assert_eq!(
        check_line(&c, &l, &[entry("1", "2026-10-04", ISSUE_ID, 3600)], yes),
        MatchVerdict::Different
    );
    assert_eq!(check_line(&c, &l, &[], yes), MatchVerdict::Different);
}

#[test]
fn entry_owned_by_worklog_is_not_a_candidate() {
    let c = conn();
    c.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, tempo_worklog_id)
         VALUES (?1, ?2, ?3, ?3, 3600, '55')",
        params![DAY, ISSUE, "2026-10-05T09:00:00Z"],
    )
    .unwrap();
    let l = line("fix", 3600);
    // not excluding owned entries would skip a line worklog itself sent
    assert_eq!(
        check_line(&c, &l, &[entry("55", DAY, ISSUE_ID, 3600)], yes),
        MatchVerdict::Different
    );
    // an owned entry beside a hand entry: the hand entry still counts
    assert_eq!(
        check_line(
            &c,
            &l,
            &[
                entry("55", DAY, ISSUE_ID, 3600),
                entry("56", DAY, ISSUE_ID, 3600)
            ],
            yes
        ),
        already("56")
    );
}

#[test]
fn matcher_error_or_empty_answer_is_unchecked() {
    let c = conn();
    let l = line("fix", 3600);
    let e = [entry("1", DAY, ISSUE_ID, 3600)];
    // swallowing the error as Different would hide that Verdict was off
    let err = check_line(&c, &l, &e, |_: &str, _: &[String]| Err(anyhow!("off")));
    assert!(matches!(err, MatchVerdict::Unchecked { .. }));
    let empty = check_line(&c, &l, &e, |_: &str, _: &[String]| Ok(vec![]));
    assert!(matches!(empty, MatchVerdict::Unchecked { .. }));
}

#[test]
fn matcher_is_not_called_for_entries_outside_the_hours_tolerance() {
    let c = conn();
    let l = line("fix", 3600);
    let far = [entry("1", DAY, ISSUE_ID, 3600 + 1860)];
    // an erroring matcher would turn this Unchecked if it were consulted
    let got = check_line(&c, &l, &far, |_: &str, _: &[String]| Err(anyhow!("off")));
    assert_eq!(got, MatchVerdict::Different);
}

#[test]
fn unknown_issue_id_is_unchecked() {
    let c = db::open_memory().unwrap();
    let got = check_line(
        &c,
        &line("fix", 3600),
        &[entry("1", DAY, ISSUE_ID, 3600)],
        yes,
    );
    assert!(matches!(got, MatchVerdict::Unchecked { .. }));
}

#[test]
fn already_holds_until_text_or_hours_change() {
    let c = conn();
    let l = line("fix", 3600);
    assert!(!is_already(&c, &l)); // no row
    mark_already(&c, &l, "77").unwrap();
    assert!(is_already(&c, &l));
    // a basis-less flag would stay true after an Owner edit
    assert!(!is_already(&c, &line("fix more", 3600)));
    assert!(!is_already(&c, &line("fix", 5400)));
    let stored: (String, String) = c
        .query_row(
            "SELECT match_status, match_tempo_id FROM tempo_line_texts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, (ALREADY_IN_TEMPO.to_string(), "77".to_string()));
}

#[test]
fn already_is_per_line() {
    let c = conn();
    mark_already(&c, &line("fix", 3600), "77").unwrap();
    let mut other = line("fix", 3600);
    other.day = "2026-10-04".into();
    // keying on issue alone would flag the other day too
    assert!(!is_already(&c, &other));
}

#[test]
fn fingerprint_uses_fallback_text_when_no_stored_text() {
    let c = conn();
    let mut l = line("x", 3600);
    l.text = None;
    mark_already(&c, &l, "1").unwrap();
    assert!(is_already(&c, &l));
    l.fallback_text = "changed".into();
    assert!(!is_already(&c, &l));
}

#[test]
fn migration_adds_match_columns_to_an_old_table() {
    let c = conn();
    c.execute_batch(
        "ALTER TABLE tempo_line_texts DROP COLUMN match_status;
         ALTER TABLE tempo_line_texts DROP COLUMN match_tempo_id;
         ALTER TABLE tempo_line_texts DROP COLUMN match_basis;",
    )
    .unwrap();
    db::migrate(&c).unwrap();
    mark_already(&c, &line("fix", 3600), "1").unwrap();
}
