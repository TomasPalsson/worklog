//! Tests for T007: Tempo line text checks (spec 017 FR-19..FR-21, B6).

use super::*;
use crate::db;
use crate::tempo_line_contract::{SetTempoLineText, TempoLineKey};
use crate::tempo_lines;
use crate::verdict_contract::LineCheck;
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::cell::RefCell;

const SUMMARY: &str = "Fix login redirect";
const SPECIFIC: &str = "a specific piece of work such as a feature, fix, file or meeting topic";

/// Answers by query and records every call.
struct Fake {
    about: bool,
    specific: bool,
    calls: RefCell<Vec<(String, Vec<String>)>>,
}

impl Fake {
    fn new(about: bool, specific: bool) -> Self {
        Self {
            about,
            specific,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn ask(&self, query: &str, texts: &[String]) -> Result<Vec<bool>> {
        self.calls
            .borrow_mut()
            .push((query.to_string(), texts.to_vec()));
        let answer = if query == SUMMARY {
            self.about
        } else {
            self.specific
        };
        Ok(vec![answer; texts.len()])
    }
}

fn run(about: bool, specific: bool) -> Option<LineCheck> {
    let fake = Fake::new(about, specific);
    check(
        |q, t| fake.ask(q, t),
        "Fixed the redirect after login",
        SUMMARY,
    )
    .unwrap()
}

#[test]
fn both_yes_passes() {
    // catches: returning NeedsLook unconditionally
    assert_eq!(run(true, true), Some(LineCheck::Passed));
}

#[test]
fn about_no_needs_look() {
    // catches: only asking the specific-work question
    assert_eq!(run(false, true), Some(LineCheck::NeedsLook));
}

#[test]
fn vague_needs_look() {
    // catches: only asking the about-ticket question, or OR instead of AND
    assert_eq!(run(true, false), Some(LineCheck::NeedsLook));
    assert_eq!(run(false, false), Some(LineCheck::NeedsLook));
}

#[test]
fn asks_the_two_questions_with_the_line_as_the_only_text() {
    // catches: swapped query/text, a reworded second question
    let fake = Fake::new(true, true);
    check(|q, t| fake.ask(q, t), "Fixed the redirect", SUMMARY).unwrap();
    let calls = fake.calls.borrow();
    let texts = vec!["Fixed the redirect".to_string()];
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], (SUMMARY.to_string(), texts.clone()));
    assert_eq!(calls[1], (SPECIFIC.to_string(), texts));
}

#[test]
fn unreachable_verdict_is_not_checked() {
    // catches: reading a Verdict failure as a failed check (spurious flag) or a pass
    let got = check(
        |_, _| Err(anyhow!("verdict not reachable")),
        "Fixed it",
        SUMMARY,
    );
    assert_eq!(got.unwrap(), None);
}

#[test]
fn blank_summary_is_not_checked_and_not_asked() {
    // catches: sending an empty query (Verdict answers 400) or passing without the ticket question
    let fake = Fake::new(true, true);
    let got = check(|q, t| fake.ask(q, t), "Fixed it", "   ").unwrap();
    assert_eq!(got, None);
    assert!(fake.calls.borrow().is_empty());
}

/// Answers per text so the regenerated text can differ from the first.
fn checked_with_regen(
    first: (bool, bool),
    second: (bool, bool),
) -> (String, Option<LineCheck>, usize) {
    let regens = RefCell::new(0usize);
    let (text, status) = check_with_regenerate(
        |q, t| {
            let (about, specific) = if t[0] == "first" { first } else { second };
            Ok(vec![if q == SUMMARY { about } else { specific }])
        },
        "first".to_string(),
        SUMMARY,
        || {
            *regens.borrow_mut() += 1;
            Ok("second".to_string())
        },
    )
    .unwrap();
    let count = *regens.borrow();
    (text, status, count)
}

#[test]
fn passing_line_is_never_regenerated() {
    // catches: regenerating every line
    let got = checked_with_regen((true, true), (false, false));
    assert_eq!(got, ("first".to_string(), Some(LineCheck::Passed), 0));
}

#[test]
fn failing_line_is_regenerated_once_and_the_new_text_kept_when_it_passes() {
    // catches: keeping the old text, or not re-checking the new one
    let got = checked_with_regen((true, false), (true, true));
    assert_eq!(got, ("second".to_string(), Some(LineCheck::Passed), 1));
}

#[test]
fn line_failing_twice_is_flagged_after_exactly_one_regenerate() {
    // catches: a retry loop, or flagging without the regenerate
    let got = checked_with_regen((false, false), (true, false));
    assert_eq!(got, ("second".to_string(), Some(LineCheck::NeedsLook), 1));
}

#[test]
fn failed_regenerate_keeps_the_text_and_flags_it() {
    // catches: propagating the writer error and losing the flag
    let (text, status) = check_with_regenerate(
        |_, _| Ok(vec![false]),
        "first".to_string(),
        SUMMARY,
        || Err(anyhow!("model down")),
    )
    .unwrap();
    assert_eq!(
        (text.as_str(), status),
        ("first", Some(LineCheck::NeedsLook))
    );
}

#[test]
fn verdict_gone_before_the_first_check_does_not_regenerate() {
    // catches: regenerating when nothing failed
    let regens = RefCell::new(0usize);
    let (text, status) = check_with_regenerate(
        |_, _| Err(anyhow!("down")),
        "first".to_string(),
        SUMMARY,
        || {
            *regens.borrow_mut() += 1;
            Ok("second".to_string())
        },
    )
    .unwrap();
    assert_eq!(
        (text.as_str(), status, *regens.borrow()),
        ("first", None, 0)
    );
}

fn key() -> TempoLineKey {
    TempoLineKey {
        day: "2026-09-30".to_string(),
        jira_issue: "APRO-2".to_string(),
    }
}

fn status_in_db(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT check_status FROM tempo_line_texts", [], |r| {
        r.get(0)
    })
    .unwrap()
}

fn seed_generated(conn: &Connection) {
    tempo_lines::commit_generated(conn, &key(), "Fixed it", "h", false).unwrap();
}

fn write_manual(conn: &Connection) {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
         VALUES ('2026-09-30', 'APRO-2', '2026-09-30T10:00:00Z', '2026-09-30T10:00:00Z', 1800)",
        [],
    )
    .unwrap();
    let body = SetTempoLineText {
        day: key().day,
        jira_issue: key().jira_issue,
        text: "mine".to_string(),
    };
    tempo_lines::set_text(conn, &body).unwrap();
}

#[test]
fn tempo_lines_set_check_stores_status_on_a_generated_line() {
    // catches: not writing, or writing the wrong string
    let conn = db::open_memory().unwrap();
    seed_generated(&conn);
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::NeedsLook)).unwrap();
    assert_eq!(status_in_db(&conn).as_deref(), Some("needs_look"));
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::Passed)).unwrap();
    assert_eq!(status_in_db(&conn).as_deref(), Some("passed"));
    tempo_lines::set_check(&conn, &key(), None).unwrap();
    assert_eq!(status_in_db(&conn), None);
}

#[test]
fn tempo_lines_set_check_never_touches_a_manual_line() {
    // catches: checking or flagging a hand-written line (FR-21)
    let conn = db::open_memory().unwrap();
    write_manual(&conn);
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::NeedsLook)).unwrap();
    assert_eq!(status_in_db(&conn), None);
}

#[test]
fn tempo_lines_set_check_creates_no_row() {
    // catches: an upsert that leaves a text-less row behind
    let conn = db::open_memory().unwrap();
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::Passed)).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM tempo_line_texts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn tempo_lines_new_generated_text_drops_the_old_check() {
    // catches: a stale pass or flag surviving a regenerate
    let conn = db::open_memory().unwrap();
    seed_generated(&conn);
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::Passed)).unwrap();
    tempo_lines::commit_generated(&conn, &key(), "Other text", "h2", false).unwrap();
    assert_eq!(status_in_db(&conn), None);
}

#[test]
fn tempo_lines_hand_edit_drops_the_old_check() {
    // catches: a flag surviving a hand edit
    let conn = db::open_memory().unwrap();
    seed_generated(&conn);
    tempo_lines::set_check(&conn, &key(), Some(LineCheck::NeedsLook)).unwrap();
    write_manual(&conn);
    assert_eq!(status_in_db(&conn), None);
}
