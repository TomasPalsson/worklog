//! Parallel projects share the day minute by minute.
//!
//! The owner often has two sessions going at once (vitinn-infra in one
//! terminal, a background job building worklog in another). One interleaved
//! timeline fused both into a single block; separate per-project lanes
//! double-counted the same hour for two customers. Instead every minute is
//! owned by exactly ONE project: the one with the most activity within
//! ±`WINDOW_MINUTES`. Short silent stretches between the same owner are
//! bridged; each run of one owner is then clustered by the normal
//! gap-timeout builder. Blocks never overlap, so nothing is billed twice.

use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

use crate::billing::work_folder_for_path;
use crate::infer::{InferBlock, InferEvent};

/// Activity within this many minutes of a minute counts toward its owner.
const WINDOW_MINUTES: i64 = 5;
/// Silent runs shorter than this between the same owner are bridged.
const BRIDGE_MINUTES: i64 = 10;

/// Lane key for an event: its repo folder, `None` for folderless events.
/// Personal (non-`~/Desktop/Work`) keys carry a marker so they can never
/// be mistaken for client work — `work_folder_for_path` basenames them too.
fn lane_key(e: &InferEvent) -> Option<String> {
    e.project_path.as_deref().map(|p| {
        let folder = work_folder_for_path(p).unwrap_or_else(|| p.to_string());
        if p.contains("/Desktop/Work/") {
            folder
        } else {
            format!("{PERSONAL_MARK}{folder}")
        }
    })
}

const PERSONAL_MARK: &str = "personal:";

pub(crate) fn build_blocks_by_project(
    events: Vec<InferEvent>,
    build: fn(Vec<InferEvent>) -> Vec<InferBlock>,
) -> Vec<InferBlock> {
    let keyed: Vec<(i64, String)> = events
        .iter()
        .filter(|e| !e.is_calendar())
        .filter_map(|e| lane_key(e).map(|k| (minute(e.ts), k)))
        .collect();
    if keyed.iter().map(|(_, k)| k).collect::<BTreeSet<_>>().len() < 2 {
        return build(events);
    }
    let runs = owner_runs(&keyed);

    // Every event lands in at most one bucket: calendar alone, a run whose
    // window contains it (project events only into their own project's run),
    // or leftovers (minutes nobody owns) built on their own.
    let mut buckets: BTreeMap<usize, Vec<InferEvent>> = BTreeMap::new();
    let mut calendar = Vec::new();
    let mut leftovers = Vec::new();
    for e in events {
        if e.is_calendar() {
            calendar.push(e);
            continue;
        }
        let m = minute(e.ts);
        let key = lane_key(&e);
        let hit = runs.iter().position(|(owner, start, end)| {
            m >= *start && m <= *end && key.as_ref().is_none_or(|k| k == owner)
        });
        match hit {
            Some(i) => buckets.entry(i).or_default().push(e),
            // Another project owns this minute: its time is already counted
            // there, so this event builds nothing (no double billing).
            None if runs.iter().any(|(_, s, end)| m >= *s && m <= *end) => {}
            None => leftovers.push(e),
        }
    }
    let mut blocks: Vec<InferBlock> = buckets.into_values().flat_map(build).collect();
    blocks.extend(build(calendar));
    blocks.extend(build(leftovers));
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

/// Client work lives under `~/Desktop/Work`; everything else (e.g.
/// `~/Desktop/Projects`) is the owner's own and yields to it.
fn is_work(key: &str) -> bool {
    !key.starts_with(PERSONAL_MARK)
}

fn minute(ts: DateTime<Utc>) -> i64 {
    ts.timestamp().div_euclid(60)
}

/// Contiguous (owner, first minute, last minute) runs over the day.
fn owner_runs(keyed: &[(i64, String)]) -> Vec<(String, i64, i64)> {
    let first = keyed.iter().map(|(m, _)| *m).min().unwrap_or(0);
    let last = keyed.iter().map(|(m, _)| *m).max().unwrap_or(0);
    let mut owners: Vec<Option<String>> = Vec::new();
    let mut prev: Option<String> = None;
    for m in first..=last {
        let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
        for (t, k) in keyed {
            if (t - m).abs() <= WINDOW_MINUTES {
                *counts.entry(k).or_default() += 1;
            }
        }
        // Work first: a minute with any work activity belongs to work, so a
        // busy personal side project never swallows client time.
        if counts.keys().any(|k| is_work(k)) {
            counts.retain(|k, _| is_work(k));
        }
        let best = counts.values().copied().max();
        let owner = best.and_then(|b| {
            let tied: Vec<&String> = counts
                .iter()
                .filter(|(_, c)| **c == b)
                .map(|(k, _)| *k)
                .collect();
            // Ties keep the current owner so a split never flickers.
            match &prev {
                Some(p) if tied.contains(&p) => Some(p.clone()),
                _ => tied.first().map(|k| (*k).clone()),
            }
        });
        if owner.is_some() {
            prev = owner.clone();
        }
        owners.push(owner);
    }
    bridge(&mut owners);

    let mut runs: Vec<(String, i64, i64)> = Vec::new();
    for (i, o) in owners.iter().enumerate() {
        let m = first + i as i64;
        match (o, runs.last_mut()) {
            (Some(k), Some((rk, _, end))) if rk == k && *end == m - 1 => *end = m,
            (Some(k), _) => runs.push((k.clone(), m, m)),
            (None, _) => {}
        }
    }
    runs
}

/// Fill silent stretches shorter than BRIDGE_MINUTES between the same owner.
fn bridge(owners: &mut [Option<String>]) {
    let bridge = Duration::minutes(BRIDGE_MINUTES).num_minutes() as usize;
    let mut i = 0;
    while i < owners.len() {
        if owners[i].is_some() {
            i += 1;
            continue;
        }
        let start = i;
        while i < owners.len() && owners[i].is_none() {
            i += 1;
        }
        if start > 0 && i < owners.len() && i - start < bridge && owners[start - 1] == owners[i] {
            let fill = owners[i].clone();
            owners[start..i].iter_mut().for_each(|o| *o = fill.clone());
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn a_single_project_day_is_unchanged() {
        let events: Vec<InferEvent> = (0..10).map(|i| ev(9, i * 2, "shell", Some(A))).collect();
        assert_eq!(build_blocks(events).len(), 1);
    }
}
