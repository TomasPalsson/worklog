//! Tests for `fill_session_folders`.

use super::*;
use crate::infer::build_blocks;
use chrono::{Duration, TimeZone, Utc};

const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const B: &str = "/Users/dev/Desktop/Projects/worklog";

fn ev(min: i64, session: Option<&str>, project: Option<&str>) -> InferEvent {
    InferEvent {
        ts: Utc.with_ymd_and_hms(2026, 9, 23, 7, 0, 0).unwrap() + Duration::minutes(min),
        source: "claude_turn".into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: project.map(str::to_string),
        session_id: session.map(str::to_string),
        title: Some("prompt".into()),
        lane_tag: None,
    }
}

fn paths(events: &[InferEvent]) -> Vec<Option<&str>> {
    events.iter().map(|e| e.project_path.as_deref()).collect()
}

#[test]
fn leading_nones_get_first_known_folder() {
    let mut v = vec![
        ev(0, Some("a"), None),
        ev(1, Some("a"), None),
        ev(2, Some("a"), Some(A)),
    ];
    fill_session_folders(&mut v);
    assert_eq!(paths(&v), vec![Some(A); 3]);
}

#[test]
fn none_after_known_takes_previous_not_later() {
    let mut v = vec![
        ev(0, Some("a"), Some(A)),
        ev(1, Some("a"), None),
        ev(2, Some("a"), Some(B)),
    ];
    fill_session_folders(&mut v);
    assert_eq!(paths(&v), vec![Some(A), Some(A), Some(B)]);
}

#[test]
fn never_borrows_another_sessions_folder() {
    let mut v = vec![ev(0, Some("a"), None), ev(1, Some("b"), Some(B))];
    fill_session_folders(&mut v);
    assert_eq!(paths(&v), vec![None, Some(B)]);
}

#[test]
fn no_session_stays_none() {
    let mut v = vec![ev(0, None, None), ev(1, Some("a"), Some(A))];
    fill_session_folders(&mut v);
    assert_eq!(paths(&v), vec![None, Some(A)]);
}

#[test]
fn session_without_folder_stays_none() {
    let mut v = vec![ev(0, Some("a"), None), ev(1, Some("a"), None)];
    fill_session_folders(&mut v);
    assert_eq!(paths(&v), vec![None, None]);
}

/// Prompts every 3 min at NULL path, then in a project; plus another
/// session elsewhere so the lane path is exercised.
fn day() -> Vec<InferEvent> {
    let mut v: Vec<InferEvent> = (0..7).map(|i| ev(i * 3, Some("a"), None)).collect();
    v.extend((7..14).map(|i| ev(i * 3, Some("a"), Some(A))));
    v.push(ev(120, Some("b"), Some(B)));
    v
}

fn covers_early_stretch(events: Vec<InferEvent>) -> bool {
    let first = events[0].ts;
    build_blocks(events)
        .iter()
        .any(|b| b.started_at <= first + Duration::minutes(1))
}

#[test]
fn early_stretch_becomes_a_block_only_with_fill() {
    assert!(
        !covers_early_stretch(day()),
        "premise: bug reproduces without fill"
    );
    let mut v = day();
    fill_session_folders(&mut v);
    assert!(covers_early_stretch(v));
}
