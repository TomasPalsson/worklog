//! Tests for the manual allocation split (`infer_allocations`).

use super::*;
use crate::infer::{build_blocks, build_blocks_with_allocations};
use chrono::TimeZone;

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
}

fn shares(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(p, f)| (p.to_string(), *f)).collect()
}

fn minutes_of(pieces: &[(String, DateTime<Utc>, DateTime<Utc>)], p: &str) -> i64 {
    pieces
        .iter()
        .filter(|(k, _, _)| k == p)
        .map(|(_, s, e)| (*e - *s).num_minutes())
        .sum()
}

#[test]
fn seventy_thirty_split_gives_forty_two_eighteen() {
    let pieces = split_intervals(&[(at(9, 0), at(10, 0))], &shares(&[("A", 0.7), ("B", 0.3)]));
    assert_eq!(minutes_of(&pieces, "A"), 42);
    assert_eq!(minutes_of(&pieces, "B"), 18);
    assert_eq!(pieces[0].2, pieces[1].1, "A's chunk comes first, then B's");
}

#[test]
fn gaps_between_worked_stretches_stay_gaps() {
    // Worked 9:00–9:10 and 9:20–9:40. 50/50 gives each 15 of the 30.
    let iv = [(at(9, 0), at(9, 10)), (at(9, 20), at(9, 40))];
    let pieces = split_intervals(&iv, &shares(&[("A", 0.5), ("B", 0.5)]));
    assert_eq!(minutes_of(&pieces, "A"), 15);
    assert_eq!(minutes_of(&pieces, "B"), 15);
    assert!(pieces
        .iter()
        .all(|(_, s, e)| *e <= at(9, 10) || *s >= at(9, 20)));
}

fn ev(m: u32, path: &str) -> InferEvent {
    InferEvent {
        ts: at(9, m),
        source: "claude_turn".into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: Some(path.into()),
        session_id: None,
        title: None,
        lane_tag: None,
    }
}

const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
const C: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
const P: &str = "/Users/dev/Desktop/Projects/worklog";

/// End to end through block building: the blocks follow the saved
/// split, and the total is exactly the automatic total.
#[test]
fn blocks_follow_a_saved_split() {
    // A typed 9:00–9:18, C typed 9:20–9:58.
    let events: Vec<InferEvent> = (0..10)
        .map(|i| ev(i * 2, A))
        .chain((10..30).map(|i| ev(i * 2, C)))
        .collect();
    let auto_total: i64 = build_blocks(events.clone())
        .iter()
        .map(|b| b.duration_seconds)
        .sum();
    let window = AllocationWindow {
        started_at: at(9, 0),
        ended_at: at(10, 0),
        shares: shares(&[("vitinn-infra", 1.0)]),
    };
    let blocks = build_blocks_with_allocations(events, &[window]);
    let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
    assert!(blocks
        .iter()
        .all(|b| b.dominant_project_path().as_deref() == Some(A)));
    assert_eq!(total, auto_total, "a split moves time, never adds it");
}

/// R6: a single-project allocation window (100% share to the project
/// that already dominates before and after it) is a spurious cut, not
/// a real split — apply_split must re-merge the resulting touching
/// pieces back into one block.
#[test]
fn single_project_allocation_window_does_not_fragment_the_block() {
    // 60 min of continuous vitinn-infra activity, no real project
    // switch anywhere.
    let events: Vec<InferEvent> = (0..30).map(|i| ev(i * 2, A)).collect();
    let auto = build_blocks(events.clone());
    assert_eq!(auto.len(), 1, "one continuous single-project block");
    // A window over the middle third, entirely re-allocated to the
    // SAME project — this used to still cut before/inside/after into
    // three pieces.
    let window = AllocationWindow {
        started_at: at(9, 20),
        ended_at: at(9, 40),
        shares: shares(&[("vitinn-infra", 1.0)]),
    };
    let blocks = apply_split(auto, &[window], &events_by_key(&events));
    assert_eq!(
        blocks.len(),
        1,
        "a single-project allocation must not fragment the block: {blocks:?}"
    );
}

/// R4/R5's "never lose owned minutes" invariant must survive a split
/// too: real 2026-09-25 data had an allocation window end 3m45s before
/// its block's own end, leaving a thin "after" remainder that
/// `finalize`'s MIN_BLOCK_MINUTES(5) drop silently deleted — ~30 min
/// of real work vanished from that day across several such windows.
/// A remainder produced by a split must survive even under 5 minutes.
#[test]
fn a_thin_split_remainder_is_never_dropped() {
    // 50 min of continuous vitinn-infra activity.
    let events: Vec<InferEvent> = (0..25).map(|i| ev(i * 2, A)).collect();
    let auto = build_blocks(events.clone());
    assert_eq!(auto.len(), 1);
    let auto_total: i64 = auto.iter().map(|b| b.duration_seconds).sum();

    // Window [9:10, 9:47) split 50/50 with lyfjastofnun leaves a 3-min
    // "after" remainder (9:47-9:50) that used to vanish. lyfjastofnun
    // needs one linkable event elsewhere in the day (same pattern as
    // `personal_blocks_are_never_touched`) so its half of the split
    // isn't dropped for having no events of its own at all.
    let window = AllocationWindow {
        started_at: at(9, 10),
        ended_at: at(9, 47),
        shares: shares(&[("lyfjastofnun", 0.5), ("vitinn-infra", 0.5)]),
    };
    let mut day = events.clone();
    day.push(ev(59, C));
    let blocks = apply_split(auto, &[window], &events_by_key(&day));
    let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
    assert_eq!(
        total, auto_total,
        "a split must never lose minutes, even a < 5 min remainder: {blocks:?}"
    );
    assert!(
        blocks.iter().any(|b| b.ended_at == at(9, 50)),
        "the thin after-remainder must survive up to the block's real end: {blocks:?}"
    );
}

#[test]
fn personal_blocks_are_never_touched() {
    // A 9:00–9:20 (work), then personal 9:40–9:58.
    let events: Vec<InferEvent> = (0..10)
        .map(|i| ev(i * 2, A))
        .chain((20..30).map(|i| ev(i * 2, P)))
        .collect();
    let auto = build_blocks(events.clone());
    let window = AllocationWindow {
        started_at: at(9, 0),
        ended_at: at(10, 0),
        shares: shares(&[("lyfjastofnun", 1.0)]),
    };
    // lyfjastofnun has one event elsewhere in the day to link.
    let mut day = events.clone();
    day.push(ev(59, C));
    let blocks = apply_split(auto.clone(), &[window], &events_by_key(&day));
    let personal = |bs: &[InferBlock]| -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
        bs.iter()
            .filter(|b| b.dominant_project_path().as_deref() == Some(P))
            .map(|b| (b.started_at, b.ended_at))
            .collect()
    };
    assert_eq!(personal(&blocks), personal(&auto));
    assert!(blocks
        .iter()
        .any(|b| b.dominant_project_path().as_deref() == Some(C)));
}
