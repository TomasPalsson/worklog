//! Tests for minute-ownership lanes (`infer_lanes`).

use super::*;
use crate::infer::build_blocks;
use chrono::{TimeZone, Utc};

fn ev(h: u32, m: u32, source: &str, project: Option<&str>) -> InferEvent {
    InferEvent {
        ts: Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap(),
        source: source.into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: project.map(str::to_string),
    }
}

const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const A_WT: &str = "/Users/dev/Desktop/Work/vitinn-infra/.claude/worktrees/sandbox-runner";
const B: &str = "/Users/dev/Desktop/Projects/worklog";

fn total_minutes(blocks: &[InferBlock]) -> i64 {
    blocks.iter().map(|b| b.duration_seconds / 60).sum()
}

fn assert_no_overlap(blocks: &[InferBlock]) {
    for w in blocks.windows(2) {
        assert!(
            w[0].ended_at <= w[1].started_at,
            "blocks overlap: {:?}–{:?} vs {:?}",
            w[0].started_at,
            w[0].ended_at,
            w[1].started_at
        );
    }
}

#[test]
fn parallel_sessions_never_overlap_or_double_count() {
    // A busy (every 2 min, worktree + main) while B pings every 7 min for the same hour.
    let mut events: Vec<InferEvent> = (0..30)
        .map(|i| {
            ev(
                12,
                i * 2,
                "claude_turn",
                Some(if i % 2 == 0 { A } else { A_WT }),
            )
        })
        .collect();
    events.extend((0..9).map(|i| ev(12, i * 7, "claude_turn", Some(B))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    assert!(
        total_minutes(&blocks) <= 62,
        "never more than the wall-clock hour, got {}",
        total_minutes(&blocks)
    );
    let a_minutes: i64 = blocks
        .iter()
        .filter(|b| {
            b.dominant_project_path()
                .as_deref()
                .is_some_and(|p| p.starts_with(A))
        })
        .map(|b| b.duration_seconds / 60)
        .sum();
    assert!(
        a_minutes >= 45,
        "the busy project owns most of the hour, got {a_minutes}"
    );
}

#[test]
fn handover_between_projects_splits_the_time() {
    let mut events: Vec<InferEvent> = (0..15)
        .map(|i| ev(9, i * 2, "claude_turn", Some(A)))
        .collect();
    events.extend((0..15).map(|i| ev(9, 30 + i * 2, "claude_turn", Some(B))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].dominant_project_path().as_deref() == Some(A));
}

#[test]
fn folderless_event_joins_the_owning_project() {
    let mut events: Vec<InferEvent> = (0..10)
        .map(|i| ev(9, i * 3, "claude_turn", Some(A)))
        .collect();
    events.extend((0..10).map(|i| ev(14, i * 3, "claude_turn", Some(B))));
    events.push(ev(9, 20, "github_pr", None));
    let blocks = build_blocks(events);
    assert_eq!(blocks.len(), 2);
    let morning = blocks
        .iter()
        .find(|b| b.started_at.format("%H").to_string() == "09")
        .unwrap();
    assert_eq!(morning.dominant_project_path().as_deref(), Some(A));
}

#[test]
fn work_outranks_personal_in_the_same_minutes() {
    // A light work session (every 6 min) beside a very busy personal one (every minute).
    let mut events: Vec<InferEvent> = (0..10)
        .map(|i| ev(12, i * 6, "claude_turn", Some(A)))
        .collect();
    events.extend((0..60).map(|i| ev(12, i, "claude", Some(B))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    let work: i64 = blocks
        .iter()
        .filter(|b| b.dominant_project_path().as_deref() == Some(A))
        .map(|b| b.duration_seconds / 60)
        .sum();
    assert!(
        work >= 50,
        "any work activity claims the minute, got {work}m"
    );
}

#[test]
fn your_typing_outweighs_claude_working_elsewhere() {
    // Owner types into A every 8 min for 2 h; Claude works in another work
    // repo every minute the whole time. The owner's attention wins.
    const C: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
    let mut events: Vec<InferEvent> = (0..16)
        .map(|i| ev(10 + (i * 8) / 60, (i * 8) % 60, "claude_turn", Some(A)))
        .collect();
    events.extend((0..120).map(|i| ev(10 + i / 60, i % 60, "claude_work", Some(C))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    let a: i64 = blocks
        .iter()
        .filter(|b| b.dominant_project_path().as_deref() == Some(A))
        .map(|b| b.duration_seconds / 60)
        .sum();
    assert!(a >= 110, "attention decides, got {a}m of 120");
}

#[test]
fn a_single_project_day_is_unchanged() {
    let events: Vec<InferEvent> = (0..10).map(|i| ev(9, i * 2, "shell", Some(A))).collect();
    assert_eq!(build_blocks(events).len(), 1);
}

/// What the owner saw on 2026-09-23: they act in A, then Claude works in
/// the background in C for ten minutes. A owns those minutes (focus), so
/// they must count as A's — not vanish because A had few events of its own.
#[test]
fn owned_minutes_count_even_when_the_owner_has_few_events_there() {
    let mut events = vec![
        ev(9, 0, "shell", Some(A_WORK)),
        ev(9, 2, "shell", Some(A_WORK)),
    ];
    events.extend((3..13).map(|m| ev(9, m, "claude_work", Some(C_WORK))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    assert!(
        total_minutes(&blocks) >= 12,
        "got {}",
        total_minutes(&blocks)
    );
    assert!(blocks
        .iter()
        .all(|b| b.dominant_project_path().as_deref() == Some(A_WORK)));
}

/// A 3-minute hop to another project joins its neighbour instead of
/// becoming a sliver that gets dropped along with its minutes.
#[test]
fn a_quick_hop_joins_its_neighbour_instead_of_vanishing() {
    let mut events: Vec<InferEvent> = (0..10)
        .map(|i| ev(9, i * 2, "shell", Some(A_WORK)))
        .collect();
    events.push(ev(9, 21, "shell", Some(C_WORK)));
    events.extend((12..20).map(|i| ev(9, i * 2, "shell", Some(A_WORK))));
    let blocks = build_blocks(events);
    assert_no_overlap(&blocks);
    assert!(
        total_minutes(&blocks) >= 38,
        "got {}",
        total_minutes(&blocks)
    );
}

const A_WORK: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const C_WORK: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
const D_WORK: &str = "/Users/dev/Desktop/Work/otherproj";

/// A folderless event landing inside an established block's span rides
/// along with it, but must never move that block's project or bounds
/// (D-08, FR-07, FR-08, B4).
#[test]
fn folderless_never_votes_or_extends() {
    let mut events: Vec<InferEvent> = (0..10)
        .map(|i| ev(9, i * 3, "claude_turn", Some(A)))
        .collect();
    events.extend((0..10).map(|i| ev(14, i * 3, "claude_turn", Some(B))));
    let baseline = build_blocks(events.clone());

    // Squarely inside A's morning span.
    events.push(ev(9, 15, "github_pr", None));
    let blocks = build_blocks(events);

    assert_eq!(blocks.len(), baseline.len());
    let morning = |bs: &[InferBlock]| {
        bs.iter()
            .find(|b| b.started_at.format("%H").to_string() == "09")
            .unwrap()
            .clone()
    };
    let (m, base_m) = (morning(&blocks), morning(&baseline));
    assert_eq!(m.dominant_project_path().as_deref(), Some(A));
    assert_eq!(m.started_at, base_m.started_at, "folderless event must not move the start");
    assert_eq!(m.ended_at, base_m.ended_at, "folderless event must not extend the end");
}

/// A folderless event nowhere near any established span joins no block
/// and creates none of its own (FR-09).
#[test]
fn folderless_event_outside_every_span_joins_no_block() {
    let mut events: Vec<InferEvent> = (0..10)
        .map(|i| ev(9, i * 3, "claude_turn", Some(A)))
        .collect();
    events.extend((0..10).map(|i| ev(14, i * 3, "claude_turn", Some(B))));
    let baseline = build_blocks(events.clone());

    // Nowhere near either span, and no project of its own.
    events.push(ev(20, 0, "github_pr", None));
    let blocks = build_blocks(events);

    assert_eq!(
        blocks.len(),
        baseline.len(),
        "a folderless event outside every span must create no block"
    );
    assert_eq!(total_minutes(&blocks), total_minutes(&baseline));
}

/// Helper activity (claude_helper) and session messages (claude_message)
/// must never vote on a lane's owner and must add no time to any block
/// (D-05, FR-16, FR-17, B7) — dense helper/message rows tagged to a
/// project the owner never touched must leave the day identical to the
/// same day without them.
#[test]
fn helper_adds_no_time() {
    let base_events = || vec![ev(9, 0, "claude_turn", Some(A)), ev(9, 30, "claude_turn", Some(A))];
    let baseline = build_blocks(base_events());

    let mut events = base_events();
    events.extend(
        (5..20).map(|m| ev(9, m, crate::clues_contract::SOURCE_CLAUDE_HELPER, Some(D_WORK))),
    );
    events.extend(
        (20..26).map(|m| ev(9, m, crate::clues_contract::SOURCE_CLAUDE_MESSAGE, Some(D_WORK))),
    );
    let blocks = build_blocks(events);

    assert!(
        blocks
            .iter()
            .all(|b| b.dominant_project_path().as_deref() != Some(D_WORK)),
        "helper/message activity must never decide a block's project"
    );
    assert_eq!(blocks.len(), baseline.len());
    assert_eq!(
        total_minutes(&blocks),
        total_minutes(&baseline),
        "helper/message activity must add no time to any block"
    );
}
