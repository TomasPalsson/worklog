//! Unit tests for the evidence-based passes (R3/R4/R5).

use super::*;
use chrono::{TimeZone, Utc};

fn at(h: u32, m: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
}

fn ev(h: u32, m: u32, source: &str, session: Option<&str>) -> InferEvent {
    InferEvent {
        ts: at(h, m),
        source: source.into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: None,
        session_id: session.map(str::to_string),
        title: None,
        lane_tag: None,
    }
}

// ───────────────────────── R3: background noise ─────────────────────────

#[test]
fn infer_evidence_isolated_claude_work_is_dropped() {
    // A single claude_work heartbeat, no sibling within 3 min, no prompt
    // within 60 min (no prompt at all) — pure background noise.
    let events = vec![ev(9, 0, "claude_work", Some("s1"))];
    let kept = drop_isolated_claude_work(events);
    assert!(kept.is_empty(), "an isolated heartbeat must vanish");
}

#[test]
fn infer_evidence_dense_claude_work_survives() {
    // Two claude_work markers 2 min apart in the same session: dense.
    let events = vec![
        ev(9, 0, "claude_work", Some("s1")),
        ev(9, 2, "claude_work", Some("s1")),
    ];
    let kept = drop_isolated_claude_work(events);
    assert_eq!(kept.len(), 2, "sibling markers within 3 min are real work");
}

#[test]
fn infer_evidence_claude_work_soon_after_a_prompt_survives() {
    let events = vec![
        ev(9, 0, "claude_turn", Some("s1")),
        ev(9, 30, "claude_work", Some("s1")), // 30 min after the prompt
    ];
    let kept = drop_isolated_claude_work(events);
    assert_eq!(
        kept.len(),
        2,
        "within 60 min of the last prompt is not noise"
    );
}

#[test]
fn infer_evidence_claude_work_long_after_a_prompt_is_dropped() {
    // Regression: heartbeats 15 min apart with no prompt in the last 60
    // min must own zero minutes.
    let events = vec![
        ev(9, 0, "claude_turn", Some("s1")),
        ev(11, 30, "claude_work", Some("s1")), // 150 min after the prompt
        ev(11, 45, "claude_work", Some("s2")), // different session, no prompt at all
    ];
    let kept = drop_isolated_claude_work(events);
    assert_eq!(kept.len(), 1, "only the prompt itself survives");
    assert_eq!(kept[0].source, "claude_turn");
}

#[test]
fn infer_evidence_dedupe_shell_drops_exact_duplicates() {
    let mut a = ev(9, 0, "shell", None);
    a.title = Some("git status".into());
    a.project_path = Some("/Users/dev/Desktop/Work/vitinn-infra".into());
    let dup = a.clone();
    let events = vec![a, dup];
    let kept = dedupe_shell_events(events);
    assert_eq!(kept.len(), 1, "duplicate shell rows must collapse to one");
}

#[test]
fn infer_evidence_dedupe_shell_keeps_distinct_commands() {
    let mut a = ev(9, 0, "shell", None);
    a.title = Some("git status".into());
    let mut b = ev(9, 0, "shell", None);
    b.title = Some("ls".into());
    let kept = dedupe_shell_events(vec![a, b]);
    assert_eq!(
        kept.len(),
        2,
        "distinct commands at the same second are not duplicates"
    );
}

// ───────────────────────── R5: evidence floor ─────────────────────────

#[test]
fn infer_evidence_floor_needs_two_distinct_minutes() {
    let one_minute = vec![ev(9, 0, "shell", None), ev(9, 0, "shell", None)];
    assert!(
        !has_evidence_floor(&one_minute),
        "same minute twice is 1 minute of evidence"
    );

    let two_minutes = vec![ev(9, 0, "shell", None), ev(9, 1, "shell", None)];
    assert!(has_evidence_floor(&two_minutes));
}

// ───────────────────────── R4: evidence-based merging ─────────────────────────

fn run(owner: &str, s: i64, e: i64) -> (String, i64, i64) {
    (owner.to_string(), s, e)
}

fn keyed_event(m: i64, key: &str, human: bool, window: i64) -> Keyed {
    (m, key.to_string(), human, window)
}

#[test]
fn infer_evidence_zero_evidence_run_is_dropped() {
    // A 3-min run of project B with no B events at all (pure bridge fill)
    // between two A runs of DIFFERENT lengths so no sandwich/class fold
    // applies — it just vanishes.
    // "personal:C" is a different (personal) class so it never folds with
    // A/B — this run's only viable fate is dropping outright.
    let keyed = vec![
        keyed_event(0, "A", true, 15),
        keyed_event(20, "A", true, 15),
        keyed_event(24, "personal:C", true, 15),
        keyed_event(40, "personal:C", true, 15),
    ];
    let runs = vec![run("A", 0, 20), run("B", 21, 23), run("personal:C", 24, 40)];
    let out = merge_by_evidence(runs, &keyed);
    assert!(
        out.iter().all(|(k, _, _)| k != "B"),
        "a run with zero own events must be dropped: {out:?}"
    );
}

#[test]
fn infer_evidence_sandwich_merges_into_shared_owner() {
    // A|b|A: same project both sides absorbs the short middle run.
    let runs = vec![run("A", 0, 20), run("B", 21, 25), run("A", 26, 40)];
    let keyed = vec![
        keyed_event(0, "A", true, 15),
        keyed_event(40, "A", true, 15),
        keyed_event(22, "B", true, 15),
    ];
    let out = merge_by_evidence(runs, &keyed);
    assert_eq!(out.len(), 1, "sandwich must merge into one run: {out:?}");
    assert_eq!(out[0], ("A".to_string(), 0, 40));
}

#[test]
fn infer_evidence_three_owner_events_stays_its_own_block() {
    // A 3-min run holding 3 owner (human) events of its own project stays,
    // even though it's shorter than the 15-min sliver threshold.
    let runs = vec![run("A", 0, 30), run("B", 31, 33), run("A", 34, 60)];
    let keyed = vec![
        keyed_event(0, "A", true, 15),
        keyed_event(60, "A", true, 15),
        keyed_event(31, "B", true, 5),
        keyed_event(32, "B", true, 5),
        keyed_event(33, "B", true, 5),
    ];
    let out = merge_by_evidence(runs, &keyed);
    assert!(
        out.iter().any(|(k, s, e)| k == "B" && *s == 31 && *e == 33),
        "a sliver with >= 3 owner events must survive untouched: {out:?}"
    );
}
