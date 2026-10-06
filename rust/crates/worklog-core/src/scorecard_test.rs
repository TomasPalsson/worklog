use super::*;
use crate::db::open_memory;
use crate::routing_contract::{DEFAULT_ABSTAIN_MARGIN, DEFAULT_RUNNER_UP_RATIO};
use crate::verdict_contract::RankedOption;
use chrono::TimeZone;
use std::collections::HashMap;

const RECENT: &str = "2026-10-05T10:00:00+00:00";
const LATER: &str = "2026-10-05T11:00:00+00:00";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 12, 0, 0).unwrap()
}

fn rule() -> RouteRule {
    RouteRule {
        abstain_margin: DEFAULT_ABSTAIN_MARGIN,
        runner_up_ratio: DEFAULT_RUNNER_UP_RATIO,
    }
}

fn rank(top: &str, p: f64, runner: f64, abstain: f64) -> Ranking {
    Ranking {
        ranking: vec![
            RankedOption {
                id: top.into(),
                probability: p,
            },
            RankedOption {
                id: "Z".into(),
                probability: runner,
            },
        ],
        abstain,
        agreed: true,
    }
}

/// Answers by the `id` in the logged state; a missing id is an unreachable helper.
#[derive(Clone)]
struct Stub(HashMap<String, Ranking>);

impl Stub {
    fn new(answers: &[(&str, Ranking)]) -> Self {
        Stub(
            answers
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }
}

impl Classifier for Stub {
    fn classify(
        &self,
        state: &serde_json::Value,
        _options: &[String],
        _examples: &std::collections::BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        Ok(state["id"].as_str().and_then(|id| self.0.get(id).cloned()))
    }
}

fn log(
    conn: &Connection,
    kind: DecisionKind,
    source: DecisionSource,
    subject: &str,
    chosen: Option<&str>,
    at: &str,
) {
    let from_verdict = source == DecisionSource::Verdict;
    verdict_decisions::record(
        conn,
        &DecisionRow {
            kind,
            source,
            subject: subject.into(),
            state_json: if from_verdict {
                format!(r#"{{"id":"{subject}"}}"#)
            } else {
                "{}".into()
            },
            options: if from_verdict {
                vec!["A".into(), "B".into(), "C".into()]
            } else {
                Vec::new()
            },
            ranking: from_verdict.then(|| rank("A", 0.5, 0.1, 0.1)),
            chosen: chosen.map(str::to_owned),
            previous: None,
            decided_at: at.into(),
        },
    )
    .unwrap();
}

fn guess(conn: &Connection, subject: &str, chosen: Option<&str>) {
    log(
        conn,
        DecisionKind::Project,
        DecisionSource::Verdict,
        subject,
        chosen,
        RECENT,
    );
}

fn owner(conn: &Connection, subject: &str, to: &str, at: &str) {
    log(
        conn,
        DecisionKind::Project,
        DecisionSource::Owner,
        subject,
        Some(to),
        at,
    );
}

fn card(conn: &Connection, stub: &Stub, apply: bool) -> Scorecard {
    let replayed = replay(load(conn, now()).unwrap(), stub);
    finish(conn, replayed, rule(), apply, None).unwrap()
}

fn tally(right: usize, wrong: usize, unsure: usize) -> Tally {
    Tally {
        right,
        wrong,
        unsure,
    }
}

// Clears the default rule (0.6 >= 0.1 * 1.2 and >= 0.1 * 1.1).
fn clear(top: &str) -> Ranking {
    rank(top, 0.6, 0.1, 0.1)
}

#[test]
fn would_file_the_value_the_owner_corrected_to_is_right() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    owner(&conn, "1", "B", LATER);
    let c = card(&conn, &Stub::new(&[("1", clear("B"))]), false);
    // catches: taking Verdict's own applied value as the final value
    assert_eq!(c.project, tally(1, 0, 0));
}

#[test]
fn would_file_what_the_owner_corrected_away_from_is_wrong() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    owner(&conn, "1", "B", LATER);
    let c = card(&conn, &Stub::new(&[("1", clear("A"))]), false);
    // catches: counting a match with the applied value instead of the correction
    assert_eq!(c.project, tally(0, 1, 0));
}

#[test]
fn the_latest_owner_correction_is_the_final_value() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    owner(&conn, "1", "B", LATER);
    owner(&conn, "1", "C", "2026-10-05T12:00:00+00:00");
    let c = card(&conn, &Stub::new(&[("1", clear("B"))]), false);
    // catches: the first correction winning (map insert that keeps the oldest)
    assert_eq!(c.project, tally(0, 1, 0));
}

#[test]
fn without_a_correction_the_applied_value_is_final() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    guess(&conn, "2", Some("A"));
    let stub = Stub::new(&[("1", clear("A")), ("2", clear("B"))]);
    // catches: treating an uncorrected row as unknown
    assert_eq!(card(&conn, &stub, false).project, tally(1, 1, 0));
}

#[test]
fn filing_what_was_left_unfiled_and_never_corrected_is_wrong() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", None);
    // catches: skipping rows whose final value is empty
    assert_eq!(
        card(&conn, &Stub::new(&[("1", clear("A"))]), false).project,
        tally(0, 1, 0)
    );
}

#[test]
fn an_answer_below_the_rule_is_unsure_even_if_its_top_differs() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    // 0.11 < 0.1 * 1.2 : under the abstain margin
    let stub = Stub::new(&[("1", rank("B", 0.11, 0.01, 0.1))]);
    // catches: counting a below-rule answer as wrong
    assert_eq!(card(&conn, &stub, false).project, tally(0, 0, 1));
}

#[test]
fn an_answer_that_fails_the_order_check_is_unsure() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    let mut r = clear("B");
    r.agreed = false;
    // catches: ignoring `agreed` when replaying
    assert_eq!(
        card(&conn, &Stub::new(&[("1", r)]), false).project,
        tally(0, 0, 1)
    );
}

#[test]
fn decisions_exactly_thirty_days_old_count_and_one_second_older_do_not() {
    let conn = open_memory().unwrap();
    for (subject, when) in [
        ("edge", "2026-09-06T12:00:00+00:00"),
        ("past", "2026-09-06T11:59:59+00:00"),
    ] {
        log(
            &conn,
            DecisionKind::Project,
            DecisionSource::Verdict,
            subject,
            Some("A"),
            when,
        );
    }
    let stub = Stub::new(&[("edge", clear("A")), ("past", clear("A"))]);
    // catches: > for >= at the window start, and no window at all
    assert_eq!(card(&conn, &stub, false).project, tally(1, 0, 0));
}

#[test]
fn projects_and_tickets_are_tallied_apart_and_do_not_share_corrections() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    log(
        &conn,
        DecisionKind::Ticket,
        DecisionSource::Verdict,
        "1",
        Some("A"),
        RECENT,
    );
    owner(&conn, "1", "B", LATER);
    let stub = Stub::new(&[("1", clear("A"))]);
    let c = card(&conn, &stub, false);
    // catches: keying the owner's final value on subject alone, and one shared tally
    assert_eq!((c.project, c.ticket), (tally(0, 1, 0), tally(1, 0, 0)));
}

#[test]
fn an_unreachable_helper_skips_the_decision_instead_of_judging_it() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    let c = card(&conn, &Stub::new(&[]), false);
    // catches: reading an unreachable helper as unsure
    assert_eq!((c.project, c.skipped), (tally(0, 0, 0), 1));
}

#[test]
fn nothing_replayed_keeps_the_current_settings_and_writes_nothing() {
    let _g = crate::envfile::ENV_TEST_LOCK.blocking_lock();
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join(".env");
    std::env::set_var("WORKLOG_ENV_FILE", &env_file);
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    let c = card(&conn, &Stub::new(&[]), true);
    let wrote = env_file.exists();
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: an empty replay trivially having "zero wrong" and saving the strictest pair
    assert_eq!((c.tuned, c.applied, wrote), (None, false, false));
}

/// Hand-computed: with these four rows a pair has zero wrong only when
/// margin >= 1.55 (row b files at <= 1.5) and gains row c only when
/// ratio <= 1.65 (0.5 >= 0.3 * r); row d files only at margin <= 1.3.
fn tuning_log(conn: &Connection) -> Stub {
    for (subject, final_value) in [("a", "A"), ("b", "C"), ("c", "A"), ("d", "A")] {
        guess(conn, subject, Some(final_value));
    }
    Stub::new(&[
        ("a", rank("A", 0.9, 0.1, 0.3)),
        ("b", rank("B", 0.45, 0.2, 0.3)),
        ("c", rank("A", 0.5, 0.3, 0.1)),
        ("d", rank("A", 0.4, 0.05, 0.3)),
    ])
}

#[test]
fn the_tuned_pair_has_zero_wrong_and_the_most_right() {
    let conn = open_memory().unwrap();
    let stub = tuning_log(&conn);
    let c = card(&conn, &stub, false);
    // catches: maximising right while ignoring wrong (that picks margin <= 1.3), and
    // a grid that stops short of 2.0 or steps in 0.1s (1.65 is not on a 0.1 grid)
    assert_eq!((c.tuned, c.applied), (Some((2.0, 1.65)), false));
}

#[test]
fn the_current_rule_is_what_the_tally_reports() {
    let conn = open_memory().unwrap();
    let stub = tuning_log(&conn);
    // 1.2 / 1.1: a right, b wrong (0.45 >= 0.36), c right (0.5 >= 0.33), d right (0.4 >= 0.36)
    // catches: tallying at the tuned pair instead of the live settings
    assert_eq!(card(&conn, &stub, false).project, tally(3, 1, 0));
}

#[test]
fn when_no_pair_has_zero_wrong_the_settings_stay_and_it_says_so() {
    let _g = crate::envfile::ENV_TEST_LOCK.blocking_lock();
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join(".env");
    std::env::set_var("WORKLOG_ENV_FILE", &env_file);
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("C"));
    // 0.9 beats 0.05 * 2.0 at every pair, so it is filed wrong everywhere.
    let c = card(&conn, &Stub::new(&[("1", rank("B", 0.9, 0.0, 0.05))]), true);
    let wrote = env_file.exists();
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: saving a least-bad pair, or staying silent
    assert_eq!((c.tuned, c.applied, wrote), (None, false, false));
    assert!(c.summary().contains("kept"), "{}", c.summary());
}

#[test]
fn apply_writes_both_settings_and_a_plain_run_writes_neither() {
    let _g = crate::envfile::ENV_TEST_LOCK.blocking_lock();
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("WORKLOG_ENV_FILE", tmp.path().join(".env"));
    let conn = open_memory().unwrap();
    let stub = tuning_log(&conn);
    let read = |k| crate::envfile::read(k);
    card(&conn, &stub, false);
    let before = (read(ABSTAIN_MARGIN_KEY), read(RUNNER_UP_RATIO_KEY));
    let c = card(&conn, &stub, true);
    let after = (read(ABSTAIN_MARGIN_KEY), read(RUNNER_UP_RATIO_KEY));
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: writing on every run, writing only one key
    assert_eq!(before, (None, None));
    assert_eq!(after, (Some("2.00".to_owned()), Some("1.65".to_owned())));
    assert!(c.applied);
}

#[test]
fn a_pair_that_files_nothing_right_is_never_tuned_to() {
    let conn = open_memory().unwrap();
    guess(&conn, "1", None);
    // catches: accepting the first zero-wrong pair even when it earns no right answers
    // 0.15 is unsure at ratio > 1.5 and filed (wrong) below it, so (2.0, 2.0) has zero wrong, zero right.
    let stub = Stub::new(&[("1", rank("A", 0.15, 0.1, 0.0))]);
    assert_eq!(card(&conn, &stub, false).tuned, None);
}

#[test]
fn a_run_that_could_not_ask_verdict_keeps_the_saved_summary() {
    let conn = open_memory().unwrap();
    let stub = tuning_log(&conn);
    let good = card(&conn, &stub, false).summary();
    card(&conn, &Stub::new(&[]), false);
    // catches: finish saving an all-skipped line over the last real one
    assert_eq!(last_summary(&conn).unwrap(), Some(good));
}

#[test]
fn a_plain_run_is_read_only() {
    let conn = open_memory().unwrap();
    let stub = tuning_log(&conn);
    run(&conn, &stub, false).unwrap();
    // catches: run saving the summary line when it is not applying
    assert_eq!(last_summary(&conn).unwrap(), None);
}

#[test]
fn p95_is_the_nearest_rank() {
    let ms = |n: u64| (1..=n).collect::<Vec<u64>>();
    // catches: max instead of p95, floor instead of ceil, and a panic on empty
    assert_eq!(p95(ms(20)), 19);
    assert_eq!(p95(ms(21)), 20);
    assert_eq!(p95(ms(100)), 95);
    assert_eq!(p95(ms(1)), 1);
    assert_eq!(p95(Vec::new()), 0);
}

#[test]
fn finishing_keeps_the_latest_summary_for_the_status_line() {
    let conn = open_memory().unwrap();
    assert_eq!(last_summary(&conn).unwrap(), None);
    let stub = tuning_log(&conn);
    let c = card(&conn, &stub, false);
    // catches: never persisting, or persisting something other than the printed line
    assert_eq!(last_summary(&conn).unwrap(), Some(c.summary()));
    assert!(c.summary().contains("3 right"), "{}", c.summary());
}

// ───────────────────────── nightly tick ─────────────────────────

use crate::daemon::{router, scorecard_due_once, state_from_conn, ClassifierFactory};
use chrono::FixedOffset;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Answers like `Stub` and counts how often it is asked.
struct Counting {
    inner: Stub,
    asked: Arc<AtomicUsize>,
}

impl Classifier for Counting {
    fn classify(
        &self,
        state: &serde_json::Value,
        options: &[String],
        examples: &std::collections::BTreeMap<String, Vec<String>>,
    ) -> Result<Option<Ranking>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.inner.classify(state, options, examples)
    }
}

fn local(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .unwrap()
}

fn counting(stub: Stub) -> (ClassifierFactory, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = asked.clone();
    let make: ClassifierFactory = Arc::new(move || {
        Box::new(Counting {
            inner: stub.clone(),
            asked: counter.clone(),
        })
    });
    (make, asked)
}

/// One decision Verdict answers, in a fresh state; the env file is a tempdir
/// because the nightly run saves its thresholds.
async fn nightly_state() -> (
    crate::daemon::Shared,
    tokio::sync::MutexGuard<'static, ()>,
    tempfile::TempDir,
) {
    let guard = crate::envfile::ENV_TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("WORKLOG_ENV_FILE", tmp.path().join(".env"));
    let conn = open_memory().unwrap();
    guess(&conn, "1", Some("A"));
    (state_from_conn(conn), guard, tmp)
}

async fn saved(state: &crate::daemon::Shared) -> Option<String> {
    let conn = state.conn.lock().await;
    last_summary(&conn).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn the_nightly_run_starts_at_three_local_and_not_a_minute_before() {
    let (state, _g, _tmp) = nightly_state().await;
    let (c, asked) = counting(Stub::new(&[("1", clear("A"))]));
    scorecard_due_once(&state, c.clone(), local(6, 2, 59)).await;
    // catches: running at any hour, and >= written as >
    assert_eq!(
        (asked.load(Ordering::SeqCst), saved(&state).await),
        (0, None)
    );
    scorecard_due_once(&state, c, local(6, 3, 0)).await;
    std::env::remove_var("WORKLOG_ENV_FILE");
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert!(saved(&state).await.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn the_nightly_run_happens_once_per_local_day() {
    let (state, _g, _tmp) = nightly_state().await;
    let (c, asked) = counting(Stub::new(&[("1", clear("A"))]));
    scorecard_due_once(&state, c.clone(), local(6, 3, 0)).await;
    scorecard_due_once(&state, c.clone(), local(6, 23, 0)).await;
    let same_day = asked.load(Ordering::SeqCst);
    scorecard_due_once(&state, c, local(7, 3, 0)).await;
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: no latch (replays every hourly tick) and a latch that never resets
    assert_eq!((same_day, asked.load(Ordering::SeqCst)), (1, 2));
}

#[tokio::test(flavor = "current_thread")]
async fn an_unreachable_verdict_does_not_use_up_the_day() {
    let (state, _g, _tmp) = nightly_state().await;
    let (down, _) = counting(Stub::new(&[]));
    scorecard_due_once(&state, down, local(6, 3, 0)).await;
    let after_down = saved(&state).await;
    let (up, asked) = counting(Stub::new(&[("1", clear("A"))]));
    scorecard_due_once(&state, up, local(6, 4, 0)).await;
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: latching (or overwriting the last summary) on a run that replayed nothing
    assert_eq!((after_down, asked.load(Ordering::SeqCst)), (None, 1));
}

#[tokio::test(flavor = "current_thread")]
async fn one_decision_verdict_never_answers_does_not_block_the_night() {
    let (state, _g, _tmp) = nightly_state().await;
    {
        let conn = state.conn.lock().await;
        guess(&conn, "2", Some("A"));
    }
    let (c, asked) = counting(Stub::new(&[("1", clear("A"))]));
    scorecard_due_once(&state, c.clone(), local(6, 3, 0)).await;
    scorecard_due_once(&state, c, local(6, 4, 0)).await;
    let line = saved(&state).await;
    std::env::remove_var("WORKLOG_ENV_FILE");
    // catches: aborting on any skipped case (never saves, replays every hour)
    assert_eq!((line.is_some(), asked.load(Ordering::SeqCst)), (true, 2));
}

#[tokio::test(flavor = "current_thread")]
async fn the_nightly_run_saves_the_tuned_thresholds() {
    let (state, _g, _tmp) = nightly_state().await;
    let (c, _) = counting(Stub::new(&[("1", clear("A"))]));
    scorecard_due_once(&state, c, local(6, 3, 0)).await;
    let keys = (
        crate::envfile::read(ABSTAIN_MARGIN_KEY),
        crate::envfile::read(RUNNER_UP_RATIO_KEY),
    );
    std::env::remove_var("WORKLOG_ENV_FILE");
    // one right row clears every pair, so the stricter tie-break gives 2.00 / 2.00
    // catches: the nightly run reporting without applying
    assert_eq!(keys, (Some("2.00".to_owned()), Some("2.00".to_owned())));
}

#[tokio::test(flavor = "current_thread")]
async fn the_status_route_shows_the_latest_scorecard_line() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    let conn = open_memory().unwrap();
    crate::purge::meta_set(&conn, LAST_KEY, "project 1 right, 0 wrong, 0 unsure").unwrap();
    let resp = router(state_from_conn(conn))
        .oneshot(
            Request::get("/verdict/status?day=2026-04-14")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // catches: the status route still returning a null scorecard
    assert_eq!(body["scorecard"], "project 1 right, 0 wrong, 0 unsure");
}

fn fixture_is_good(text: &str) -> bool {
    LINE_FIXTURE.iter().any(|(t, _, good)| *t == text && *good)
}

/// Answers both line checks the way the fixture expects.
fn fixture_matcher(_query: &str, texts: &[String]) -> Result<Vec<bool>> {
    Ok(texts.iter().map(|t| fixture_is_good(t)).collect())
}

#[test]
fn line_fixture_has_five_good_and_five_vague_lines_with_summaries() {
    // catches: a skewed fixture that makes N/10 meaningless
    assert_eq!(LINE_FIXTURE.iter().filter(|f| f.2).count(), 5);
    assert_eq!(LINE_FIXTURE.iter().filter(|f| !f.2).count(), 5);
    assert!(LINE_FIXTURE.iter().all(|f| !f.1.trim().is_empty()));
}

#[test]
fn line_fixture_all_correct_is_ten_of_ten() {
    // catches: counting only passes, or only vague lines
    assert_eq!(
        line_fixture(fixture_matcher),
        "line check fixture: 10/10 right"
    );
}

#[test]
fn line_fixture_counts_wrong_verdicts_out() {
    // catches: scoring every answered line as right (yes to all: vague lines pass wrongly)
    let yes = |_: &str, t: &[String]| Ok(vec![true; t.len()]);
    assert_eq!(line_fixture(yes), "line check fixture: 5/10 right");
    // catches: the reverse count (no to all: good lines flagged wrongly)
    let no = |_: &str, t: &[String]| Ok(vec![false; t.len()]);
    assert_eq!(line_fixture(no), "line check fixture: 5/10 right");
}

#[test]
fn line_fixture_unreachable_is_skipped() {
    // catches: reporting 0/10 for an unreachable helper
    let down = |_: &str, _: &[String]| -> Result<Vec<bool>> { anyhow::bail!("down") };
    assert_eq!(
        line_fixture(down),
        "line check fixture: skipped (Verdict not answering)"
    );
}

#[test]
fn line_fixture_dropping_out_midway_is_skipped() {
    // catches: scoring the lines answered so far as N/10
    let calls = std::cell::Cell::new(0);
    let flaky = |q: &str, t: &[String]| {
        calls.set(calls.get() + 1);
        if calls.get() > 7 {
            anyhow::bail!("down")
        }
        fixture_matcher(q, t)
    };
    assert_eq!(
        line_fixture(flaky),
        "line check fixture: skipped (Verdict not answering)"
    );
}

#[test]
fn finish_keeps_the_fixture_after_the_scorecard_in_one_write() {
    let conn = open_memory().unwrap();
    let replayed = replay(load(&conn, now()).unwrap(), &Stub::new(&[]));
    let c = finish(
        &conn,
        replayed,
        rule(),
        false,
        Some("line check fixture: 10/10 right"),
    )
    .unwrap();
    // catches: replacing the scorecard instead of appending, or dropping the fixture
    assert_eq!(
        last_summary(&conn).unwrap().unwrap(),
        format!("{}; line check fixture: 10/10 right", c.summary())
    );
}
