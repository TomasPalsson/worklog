//! Tests for B9: a customer tag only divides its own folder's minutes.

use crate::infer::{build_blocks, InferEvent};
use crate::infer_lanes::build_blocks_by_project;
use chrono::{TimeZone, Utc};
use std::collections::BTreeSet;

fn ev(off_min: u32, source: &str, project: Option<&str>) -> InferEvent {
    InferEvent {
        ts: Utc
            .with_ymd_and_hms(2026, 9, 23, 9 + off_min / 60, off_min % 60, 0)
            .unwrap(),
        source: source.into(),
        duration_seconds: None,
        jira_issue: None,
        event_id: None,
        project_path: project.map(str::to_string),
        session_id: None,
        title: None,
        lane_tag: None,
    }
}

fn tagged(mut e: InferEvent, tag: &str) -> InferEvent {
    e.lane_tag = Some(tag.to_string());
    e
}

const A: &str = "/Users/dev/Desktop/Work/a";
const B: &str = "/Users/dev/Desktop/Work/b";

fn minutes_for(blocks: &[crate::infer::InferBlock], folder: &str) -> BTreeSet<i64> {
    let mut mins = BTreeSet::new();
    for b in blocks {
        if b.events
            .iter()
            .any(|e| e.project_path.as_deref() == Some(folder))
        {
            let mut m = b.started_at.timestamp() / 60;
            let end = b.ended_at.timestamp() / 60;
            while m < end {
                mins.insert(m);
                m += 1;
            }
        }
    }
    mins
}

fn minutes_for_folder(blocks: &[crate::infer::InferBlock], folder: &str) -> i64 {
    blocks
        .iter()
        .filter(|b| {
            b.events
                .iter()
                .any(|e| e.project_path.as_deref() == Some(folder))
        })
        .map(|b| b.duration_seconds / 60)
        .sum()
}

/// Folder A: a weak, short Cust1 touch (2 events, 10 owned minutes) right
/// where folder B's own run ends, then five robust ~19-min Cust2/Cust1
/// stretches. Folder B brackets the whole thing on both sides with its own
/// longer runs — long enough to out-fold the weak touch if a tag were ever
/// allowed to compete against another folder for it.
fn scenario() -> (Vec<InferEvent>, Vec<InferEvent>) {
    let mut a_events: Vec<InferEvent> = Vec::new();
    a_events.push(tagged(ev(30, "claude_turn", Some(A)), "Cust1"));
    a_events.push(tagged(ev(39, "claude_turn", Some(A)), "Cust1"));
    let stretches = [
        ("Cust2", 40),
        ("Cust1", 60),
        ("Cust2", 80),
        ("Cust1", 100),
        ("Cust2", 120),
    ];
    for (tag, base) in stretches {
        for i in 0..10 {
            a_events.push(tagged(ev(base + i * 2, "claude_turn", Some(A)), tag));
        }
    }
    let mut b_events: Vec<InferEvent> =
        (0..15).map(|i| ev(i * 2, "claude_turn", Some(B))).collect();
    b_events.extend((0..15).map(|i| ev(140 + i * 2, "claude_turn", Some(B))));
    (a_events, b_events)
}

#[test]
fn split_keeps_folder_minutes() {
    let (a_events, b_events) = scenario();

    let mut tagged_events = a_events.clone();
    tagged_events.extend(b_events.clone());
    let tagged_blocks = build_blocks_by_project(tagged_events, build_blocks);

    let mut untagged_events: Vec<InferEvent> = a_events
        .into_iter()
        .map(|mut e| {
            e.lane_tag = None;
            e
        })
        .collect();
    untagged_events.extend(b_events);
    let untagged_blocks = build_blocks_by_project(untagged_events, build_blocks);

    assert_eq!(
        minutes_for(&tagged_blocks, B),
        minutes_for(&untagged_blocks, B),
        "folder B's owned minutes must not change when folder A's sessions get customer tags"
    );
    assert_eq!(
        minutes_for_folder(&tagged_blocks, A),
        minutes_for_folder(&untagged_blocks, A),
        "folder A's total minutes must not change when its sessions get customer tags"
    );

    let a_blocks: Vec<_> = tagged_blocks
        .iter()
        .filter(|b| {
            b.events
                .iter()
                .any(|e| e.project_path.as_deref() == Some(A))
        })
        .collect();
    assert!(
        a_blocks.len() >= 2,
        "folder A must split into at least two blocks by customer tag: {a_blocks:?}"
    );
    for b in &a_blocks {
        let tags: BTreeSet<&str> = b
            .events
            .iter()
            .filter_map(|e| e.lane_tag.as_deref())
            .collect();
        assert!(
            tags.len() <= 1,
            "a folder-A block must not mix customer tags: {b:?}"
        );
    }
}

/// FR-03: a plain (untagged) session inside an otherwise-tagged folder run
/// keeps its own stretch, mixing with neither neighbouring customer.
#[test]
fn untagged_session_inside_split_folder_keeps_its_own_stretch() {
    let mut a_events: Vec<InferEvent> = Vec::new();
    for i in 0..10 {
        a_events.push(tagged(ev(i * 2, "claude_turn", Some(A)), "Cust1"));
    }
    for i in 0..10 {
        a_events.push(ev(20 + i * 2, "claude_turn", Some(A)));
    }
    for i in 0..10 {
        a_events.push(tagged(ev(40 + i * 2, "claude_turn", Some(A)), "Cust2"));
    }
    // Folder B: sparse and offset from A's cadence, only to keep the day
    // multi-lane in both builds — never dense enough to win a minute of A's.
    let b_events: Vec<InferEvent> = (0..6)
        .map(|i| ev(i * 10 + 5, "claude_turn", Some(B)))
        .collect();

    let mut events = a_events.clone();
    events.extend(b_events.clone());
    let blocks = build_blocks_by_project(events, build_blocks);
    let a_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| {
            b.events
                .iter()
                .any(|e| e.project_path.as_deref() == Some(A))
        })
        .collect();
    assert_eq!(a_blocks.len(), 3, "three separate stretches: {a_blocks:?}");
    let middle = a_blocks[1];
    assert!(
        middle.events.iter().all(|e| e.lane_tag.is_none()),
        "the middle block must hold only the untagged session: {middle:?}"
    );

    let mut untagged_events: Vec<InferEvent> = a_events
        .into_iter()
        .map(|mut e| {
            e.lane_tag = None;
            e
        })
        .collect();
    untagged_events.extend(b_events);
    let untagged_blocks = build_blocks_by_project(untagged_events, build_blocks);
    assert_eq!(
        minutes_for_folder(&blocks, A),
        minutes_for_folder(&untagged_blocks, A),
        "tagging must not change the folder's total minutes"
    );
}

/// A folder run holding only one customer's tag (no untagged events) splits
/// into exactly the same block the all-untagged build produces (B9). Folder
/// B just keeps the day multi-lane so the split path actually runs.
#[test]
fn single_customer_run_is_one_block() {
    let a_events: Vec<InferEvent> = (0..10)
        .map(|i| tagged(ev(i * 2, "claude_turn", Some(A)), "Cust1"))
        .collect();
    let b_events: Vec<InferEvent> = (0..3)
        .map(|i| ev(100 + i * 2, "claude_turn", Some(B)))
        .collect();

    let mut tagged_events = a_events.clone();
    tagged_events.extend(b_events.clone());
    let tagged_blocks = build_blocks_by_project(tagged_events, build_blocks);

    let mut untagged_events: Vec<InferEvent> = a_events
        .into_iter()
        .map(|mut e| {
            e.lane_tag = None;
            e
        })
        .collect();
    untagged_events.extend(b_events);
    let untagged_blocks = build_blocks_by_project(untagged_events, build_blocks);

    let a_block = |blocks: &[crate::infer::InferBlock]| {
        blocks
            .iter()
            .find(|b| {
                b.events
                    .iter()
                    .any(|e| e.project_path.as_deref() == Some(A))
            })
            .cloned()
            .unwrap()
    };
    let (t, u) = (a_block(&tagged_blocks), a_block(&untagged_blocks));
    assert_eq!(t.started_at, u.started_at);
    assert_eq!(t.ended_at, u.ended_at);
}
