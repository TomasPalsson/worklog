//! Parallel projects share the day minute by minute.
//!
//! The owner often has two sessions going at once (vitinn-infra in one
//! terminal, a background job building worklog in another). One interleaved
//! timeline fused both into a single block; separate per-project lanes
//! double-counted the same hour for two customers. Instead every minute is
//! owned by exactly ONE project: the one with the most activity within
//! ±`WINDOW_MINUTES`. Short silent stretches between the same owner are
//! bridged, and a run under `MIN_RUN_MINUTES` joins its neighbour. Each
//! run becomes one block spanning exactly the minutes it owns, so every
//! owned minute is counted once and blocks never overlap.

use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

use crate::billing::work_folder_for_path;
use crate::infer::{InferBlock, InferEvent};

/// Background activity within this many minutes of a minute counts toward its owner.
/// Also the max gap `overlaps::project_intervals` bridges inside one
/// project's activity — the two "5 minutes of quiet is still the same
/// stretch of work" rules should agree.
pub(crate) const WINDOW_MINUTES: i64 = 5;
/// The owner's own actions reach further: attention stays on a project for a
/// while after typing into it.
const HUMAN_WINDOW_MINUTES: i64 = 15;
/// A run shorter than this (a quick hop to another project) joins its
/// neighbour instead of becoming a sliver block that gets dropped.
const MIN_RUN_MINUTES: i64 = 5;

/// Sources that are the owner acting, not a tool working on their behalf.
/// Exposed to `overlaps` so its per-project human/background event split
/// uses the exact same rule this module owns and elsewhere every minute
/// belongs to one project.
pub(crate) fn is_human(source: &str) -> bool {
    matches!(
        source,
        "claude_turn"
            | "shell"
            | "git_reflog"
            | "github_commit"
            | "github_pr"
            | "slack"
            | "firefox"
    )
}
/// Silent runs shorter than this between the same owner are bridged.
const BRIDGE_MINUTES: i64 = 10;

/// Lane key for an event: its repo folder, `None` for folderless events.
/// Personal (non-`~/Desktop/Work`) keys carry a marker so they can never
/// be mistaken for client work — `work_folder_for_path` basenames them too.
/// `pub(crate)` so `overlaps` folds project keys identically instead of
/// duplicating this logic.
pub(crate) fn lane_key(e: &InferEvent) -> Option<String> {
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

/// (minute, lane key, is the owner acting).
type Keyed = (i64, String, bool);

pub(crate) fn build_blocks_by_project(
    events: Vec<InferEvent>,
    build: fn(Vec<InferEvent>) -> Vec<InferBlock>,
) -> Vec<InferBlock> {
    let keyed: Vec<Keyed> = events
        .iter()
        .filter(|e| !e.is_calendar())
        .filter_map(|e| lane_key(e).map(|k| (minute(e.ts), k, is_human(&e.source))))
        .collect();
    if keyed
        .iter()
        .map(|(_, k, _)| k)
        .collect::<BTreeSet<_>>()
        .len()
        < 2
    {
        return build(events);
    }
    let runs = fold_short_runs(owner_runs(&keyed));
    let by_key = crate::infer_allocations::events_by_key(&events);

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
    // One block per run, spanning the minutes it owns; a run holding none
    // of its owner's events links the nearest one so it reads as theirs.
    let mut blocks: Vec<InferBlock> = runs
        .iter()
        .enumerate()
        .filter_map(|(i, (owner, s, e))| {
            let evs = buckets.remove(&i).unwrap_or_default();
            let own = by_key.get(owner).map(Vec::as_slice).unwrap_or(&[]);
            let at = |m: i64| DateTime::from_timestamp(m * 60, 0);
            crate::infer_allocations::span_block(evs, at(*s)?, at(*e + 1)?, own)
        })
        .collect();
    blocks.extend(build(calendar));
    blocks.extend(build(leftovers));
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

/// Client work lives under `~/Desktop/Work`; everything else (e.g.
/// `~/Desktop/Projects`) is the owner's own and yields to it. `pub(crate)`
/// so `overlaps` only ever considers WORK projects for a split.
pub(crate) fn is_work(key: &str) -> bool {
    !key.starts_with(PERSONAL_MARK)
}

pub(crate) fn minute(ts: DateTime<Utc>) -> i64 {
    ts.timestamp().div_euclid(60)
}

/// Contiguous (owner, first minute, last minute) runs over the day.
fn owner_runs(keyed: &[Keyed]) -> Vec<(String, i64, i64)> {
    let first = keyed.iter().map(|(m, _, _)| *m).min().unwrap_or(0);
    let last = keyed.iter().map(|(m, _, _)| *m).max().unwrap_or(0);
    let mut owners: Vec<Option<String>> = Vec::new();
    let mut prev: Option<String> = None;
    for m in first..=last {
        // Focus follows the owner's latest action: the project of the most
        // recent human event at or before this minute (within the human
        // window) owns it outright — work before personal.
        if let Some(focus) = latest_human(keyed, m) {
            prev = Some(focus.clone());
            owners.push(Some(focus));
            continue;
        }
        let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
        for (t, k, _) in keyed {
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

/// Project of the owner's most recent action in (m − HUMAN_WINDOW, m],
/// preferring work: the latest work action wins; a personal action only
/// when no work action is in the window.
fn latest_human(keyed: &[Keyed], m: i64) -> Option<String> {
    let recent = |work: bool| {
        keyed
            .iter()
            .filter(|(t, k, human)| {
                *human && *t <= m && m - t <= HUMAN_WINDOW_MINUTES && is_work(k) == work
            })
            .max_by_key(|(t, _, _)| *t)
            .map(|(_, k, _)| k.clone())
    };
    recent(true).or_else(|| recent(false))
}

/// Fill silent stretches shorter than BRIDGE_MINUTES between the same owner.
/// Fold runs under `MIN_RUN_MINUTES` into a touching neighbour of the same
/// kind (work into work, personal into personal — never across), then
/// merge touching runs that now share an owner.
fn fold_short_runs(runs: Vec<(String, i64, i64)>) -> Vec<(String, i64, i64)> {
    let short = |r: &(String, i64, i64)| r.2 - r.1 + 1 < MIN_RUN_MINUTES;
    let joins = |a: &(String, i64, i64), b: &(String, i64, i64)| {
        a.2 + 1 == b.1 && is_work(&a.0) == is_work(&b.0)
    };
    let mut out: Vec<(String, i64, i64)> = Vec::new();
    for r in runs {
        match out.last_mut() {
            Some(prev) if joins(prev, &r) && (short(&r) || prev.0 == r.0) => prev.2 = r.2,
            // A short run with nothing before it hands its minutes forward.
            Some(prev) if joins(prev, &r) && short(prev) => {
                prev.0 = r.0.clone();
                prev.2 = r.2;
            }
            _ => out.push(r),
        }
    }
    out
}

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
#[path = "infer_lanes_test.rs"]
mod tests;
