//! Manual overrides for `infer_lanes::owner_runs`.
//!
//! The owner sometimes disagrees with the automatic per-minute project
//! split inside an overlap window (see `overlaps`) — "put more of that hour
//! on vitinn-infra, less on lyfjastofnun". An [`AllocationWindow`] records
//! that choice; `apply_allocations` overwrites the automatic owner for every
//! minute in its range, split into contiguous chunks by share.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::infer::{InferBlock, InferEvent};
use crate::infer_lanes::minute;

/// A user-chosen split of an overlap window — every worked minute in
/// `[started_at, ended_at)` is handed out to `shares` in alphabetical
/// project order (a `BTreeMap` iterates that way), each getting
/// `share * worked_minutes` minutes (the last project absorbs any
/// rounding remainder). Overrides `owner_runs`'s automatic decision for
/// those minutes outright.
#[derive(Debug, Clone)]
pub struct AllocationWindow {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub shares: BTreeMap<String, f64>,
}

/// Overwrite `owners[i]` for every ALREADY-OWNED minute inside an
/// allocation window with the chunk its share assigns. `owners[i]`
/// corresponds to minute `first + i`. A minute that's `None` (no project
/// had any activity there at all — genuinely idle) is left `None`: the
/// owner can rebalance a window's real activity between projects, but an
/// allocation must never invent time nobody actually spent.
pub(crate) fn apply_allocations(
    owners: &mut [Option<String>],
    first: i64,
    allocations: &[AllocationWindow],
) {
    for alloc in allocations {
        let start = minute(alloc.started_at) - first;
        let end = minute(alloc.ended_at) - first;
        // Only WORK minutes someone actually worked are split: a share is
        // a share of real work time, and personal time is never touched.
        let worked: Vec<usize> = (start.max(0)..end.min(owners.len() as i64))
            .map(|i| i as usize)
            .filter(|&i| {
                owners[i]
                    .as_deref()
                    .is_some_and(crate::infer_lanes::is_work)
            })
            .collect();
        let total = worked.len();
        if total == 0 || alloc.shares.is_empty() {
            continue;
        }
        let mut assigned = 0usize;
        let mut cum_frac = 0.0;
        let n = alloc.shares.len();
        for (i, (project, frac)) in alloc.shares.iter().enumerate() {
            cum_frac += frac;
            let target = if i + 1 == n {
                total
            } else {
                ((cum_frac * total as f64).round() as usize).min(total)
            };
            for &idx in &worked[assigned..target.max(assigned)] {
                owners[idx] = Some(project.clone());
            }
            assigned = target.max(assigned);
        }
    }
}

/// Does the inclusive minute run `[start, end]` touch a saved split?
pub(crate) fn in_allocation(allocations: &[AllocationWindow], start: i64, end: i64) -> bool {
    allocations
        .iter()
        .any(|a| minute(a.started_at) <= end && start < minute(a.ended_at))
}

/// A real folder path per lane key, so an allocated block's events can be
/// retagged to the project they were handed to.
pub(crate) fn owner_paths(events: &[InferEvent]) -> BTreeMap<String, String> {
    let mut paths = BTreeMap::new();
    for e in events {
        if let (Some(k), Some(p)) = (crate::infer_lanes::lane_key(e), &e.project_path) {
            paths.entry(k).or_insert_with(|| p.clone());
        }
    }
    paths
}

/// The block for a run a saved split handed to one project: it spans the
/// run's minutes exactly (the time follows the split, not the event
/// timestamps) and carries the events inside, retagged to `path` so the
/// block reads as that project's. `None` when the run holds no events.
// ponytail: a run with no events of its own, or one under MIN_BLOCK
// (a 1–2% sliver), builds nothing; fold slivers into a neighbour if it matters.
pub(crate) fn allocated_block(
    path: Option<&str>,
    start: i64,
    end: i64,
    mut events: Vec<InferEvent>,
) -> Option<InferBlock> {
    if let Some(p) = path {
        events
            .iter_mut()
            .for_each(|e| e.project_path = Some(p.to_string()));
    }
    events.sort_by_key(|e| e.ts);
    let (first, rest) = events.split_first()?;
    let mut block = crate::infer::new_block(first);
    rest.iter()
        .for_each(|e| crate::infer::extend_block(&mut block, e));
    block.started_at = DateTime::from_timestamp(start * 60, 0)?;
    block.ended_at = DateTime::from_timestamp((end + 1) * 60, 0)?;
    block.duration_seconds = (end + 1 - start) * 60;
    crate::infer::finalize(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
    }

    /// Every minute in `apply_allocations`'s tests starts already "owned"
    /// (`Some("orig")`) — that's the realistic precondition: allocations
    /// only ever run over minutes the automatic pass already assigned to
    /// SOME project. `idle_minutes_are_never_invented_into_activity` below
    /// covers the genuinely-idle (`None`) case.
    fn all_owned(n: usize) -> Vec<Option<String>> {
        vec![Some("orig".to_string()); n]
    }

    #[test]
    fn hundred_percent_to_one_project_fills_the_whole_window() {
        let first = minute(at(9, 0));
        let mut owners = all_owned(60);
        let mut shares = BTreeMap::new();
        shares.insert("A".to_string(), 1.0);
        let allocations = vec![AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(10, 0),
            shares,
        }];
        apply_allocations(&mut owners, first, &allocations);
        assert!(owners.iter().all(|o| o.as_deref() == Some("A")));
    }

    #[test]
    fn seventy_thirty_split_gives_forty_two_eighteen() {
        let first = minute(at(9, 0));
        let mut owners = all_owned(60);
        let mut shares = BTreeMap::new();
        shares.insert("A".to_string(), 0.7);
        shares.insert("B".to_string(), 0.3);
        let allocations = vec![AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(10, 0),
            shares,
        }];
        apply_allocations(&mut owners, first, &allocations);
        let a = owners.iter().filter(|o| o.as_deref() == Some("A")).count();
        let b = owners.iter().filter(|o| o.as_deref() == Some("B")).count();
        assert_eq!(a, 42, "70% of 60 minutes");
        assert_eq!(b, 18, "30% of 60 minutes");
        // Contiguous: A's minutes all come before B's.
        let last_a = owners.iter().rposition(|o| o.as_deref() == Some("A"));
        let first_b = owners.iter().position(|o| o.as_deref() == Some("B"));
        assert!(
            last_a < first_b,
            "each project's minutes must be one contiguous chunk"
        );
    }

    #[test]
    fn shares_split_the_worked_minutes_not_the_idle_ones() {
        let first = minute(at(9, 0));
        // Worked 0..10 and 20..40, idle 10..20. A 50/50 split must give
        // each project 15 of the 30 worked minutes — not A 0..20 (of
        // which only 10 are worked) and B 20..40 (all 20 worked).
        let mut owners: Vec<Option<String>> = (0..40)
            .map(|i| (!(10..20).contains(&i)).then(|| "orig".to_string()))
            .collect();
        let mut shares = BTreeMap::new();
        shares.insert("A".to_string(), 0.5);
        shares.insert("B".to_string(), 0.5);
        let allocations = vec![AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(9, 40),
            shares,
        }];
        apply_allocations(&mut owners, first, &allocations);
        let count = |p: &str| owners.iter().filter(|o| o.as_deref() == Some(p)).count();
        assert_eq!(count("A"), 15);
        assert_eq!(count("B"), 15);
        assert_eq!(owners.iter().filter(|o| o.is_none()).count(), 10);
    }

    #[test]
    fn personal_minutes_are_never_turned_into_work() {
        let first = minute(at(9, 0));
        // 0..10 work, 10..20 personal. 100% to A must only take the work.
        let mut owners: Vec<Option<String>> = (0..20)
            .map(|i| {
                Some(if i < 10 {
                    "orig".to_string()
                } else {
                    "personal:home".to_string()
                })
            })
            .collect();
        let mut shares = BTreeMap::new();
        shares.insert("A".to_string(), 1.0);
        let allocations = vec![AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(9, 20),
            shares,
        }];
        apply_allocations(&mut owners, first, &allocations);
        assert!(owners[..10].iter().all(|o| o.as_deref() == Some("A")));
        assert!(owners[10..]
            .iter()
            .all(|o| o.as_deref() == Some("personal:home")));
    }

    #[test]
    fn idle_minutes_are_never_invented_into_activity() {
        let first = minute(at(9, 0));
        // Active 0..10, genuinely idle (no activity at all) 10..20, active
        // again 20..30 — a 100%-to-A allocation over the whole [0,30)
        // window must skip the idle stretch outright.
        let mut owners: Vec<Option<String>> = (0..30)
            .map(|i| {
                if (10..20).contains(&i) {
                    None
                } else {
                    Some("orig".to_string())
                }
            })
            .collect();
        let mut shares = BTreeMap::new();
        shares.insert("A".to_string(), 1.0);
        let allocations = vec![AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(9, 30),
            shares,
        }];
        apply_allocations(&mut owners, first, &allocations);
        for (i, o) in owners.iter().enumerate() {
            if (10..20).contains(&i) {
                assert_eq!(*o, None, "idle minute {i} must stay idle");
            } else {
                assert_eq!(
                    o.as_deref(),
                    Some("A"),
                    "active minute {i} must be reassigned"
                );
            }
        }
    }

    /// End to end through block building: the blocks must follow the saved
    /// split, not just the per-minute labels.
    #[test]
    fn blocks_follow_a_saved_split() {
        use crate::infer::{build_blocks, build_blocks_with_allocations, InferEvent};
        const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
        const C: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
        let ev = |m: u32, path: &str| InferEvent {
            ts: at(9, m),
            source: "claude_turn".into(),
            duration_seconds: None,
            jira_issue: None,
            event_id: None,
            project_path: Some(path.into()),
        };
        // A typed 9:00–9:18, C typed 9:20–9:58.
        let events: Vec<InferEvent> = (0..10)
            .map(|i| ev(i * 2, A))
            .chain((10..30).map(|i| ev(i * 2, C)))
            .collect();
        let auto_total: i64 = build_blocks(events.clone())
            .iter()
            .map(|b| b.duration_seconds)
            .sum();

        let mut shares = BTreeMap::new();
        shares.insert("vitinn-infra".to_string(), 1.0);
        let window = AllocationWindow {
            started_at: at(9, 0),
            ended_at: at(10, 0),
            shares,
        };
        let blocks = build_blocks_with_allocations(events, &[window]);
        let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
        assert!(
            blocks
                .iter()
                .all(|b| b.dominant_project_path().as_deref() == Some(A)),
            "every block in the window must be vitinn-infra's"
        );
        assert!(
            total >= 55 * 60,
            "vitinn-infra gets the whole hour, got {total}s"
        );
        assert!(total <= auto_total.max(60 * 60), "no time is invented");
    }
}
