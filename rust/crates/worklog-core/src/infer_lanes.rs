//! Parallel projects share the day minute by minute.
//!
//! The owner often has two sessions going at once (vitinn-infra in one
//! terminal, a background job building worklog in another). One interleaved
//! timeline fused both into a single block; separate per-project lanes
//! double-counted the same hour for two customers. Instead every minute is
//! owned by exactly ONE project: the one with the most activity within
//! ±`WINDOW_MINUTES`. Short silent stretches between the same owner are
//! bridged, and a run judged too weak on its own evidence
//! (`infer_evidence::merge_by_evidence`) joins its neighbour or is
//! dropped. Each run becomes one block spanning exactly the minutes it
//! owns, so every owned minute is counted once and blocks never overlap.

use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

use crate::billing::work_folder_for_path;
use crate::infer::{InferBlock, InferEvent};

/// Background activity within this many minutes of a minute counts toward its owner.
/// Also the max gap `overlaps::project_intervals` bridges inside one
/// project's activity — the two "5 minutes of quiet is still the same
/// stretch of work" rules should agree. Also how long a non-prompt human
/// action (a shell command, a browser tab, Slack, the owner's own git
/// commits/PRs) holds a lane's focus (R1) — a quick touch, not sustained
/// attention.
pub(crate) const WINDOW_MINUTES: i64 = 5;
/// Only an owner PROMPT (`claude_turn`) reflects sustained attention on a
/// project; its focus reaches this much further (R1).
const HUMAN_WINDOW_MINUTES: i64 = 15;
/// A run shorter than this (a quick hop to another project) joins its
/// neighbour before the evidence-based pass (R4) even runs — a plain
/// length check for the truly tiny runs, same threshold `overlaps`
/// bridges on, so "5 minutes of quiet is still the same stretch" agrees
/// everywhere.
const MIN_RUN_MINUTES: i64 = 5;

/// Sources that are the owner acting, not a tool working on their behalf.
/// Exposed to `overlaps` so its per-project human/background event split
/// uses the exact same rule this module owns and elsewhere every minute
/// belongs to one project.
///
/// `git_reflog` is deliberately excluded (R2): agents make the vast
/// majority of reflog events (checkouts/commits fired by an autonomous
/// tool run), and the owner's own git commands already arrive as `shell`
/// — so reflog is background evidence, never a focus-holding action.
pub(crate) fn is_human(source: &str) -> bool {
    matches!(
        source,
        "claude_turn" | "shell" | "github_commit" | "github_pr" | "slack" | "firefox"
    )
}

/// How long a human action holds a lane's focus once it happens (R1).
/// Only called for sources `is_human` already accepted.
fn focus_window_minutes(source: &str) -> i64 {
    if source == "claude_turn" {
        HUMAN_WINDOW_MINUTES
    } else {
        WINDOW_MINUTES
    }
}

/// Claude hook lifecycle events (source `claude`) are titled after the
/// hook event name (`hook_run::title_for`) — SessionStart/Stop/SessionEnd
/// are the session's own bookkeeping, not the owner acting or the tool
/// working (R3): they never vote on a lane's owner and never hold a
/// minute open. Ordinary hook heartbeats on the same source
/// (UserPromptSubmit, PreToolUse/PostToolUse) are real activity signal
/// and keep voting exactly as before.
///
/// `pub(crate)` so `InferBlock::dominant_project_path` and
/// `infer_evidence::single_project` can exclude the same events from a
/// block's project-identity vote — a rider that never voted on which
/// lane owns a minute must never vote on which project a block IS,
/// either (otherwise a handful of unrelated SessionStart/SessionEnd
/// pings can outnumber the block's real events and flip its class).
pub(crate) fn is_lifecycle(e: &InferEvent) -> bool {
    is_lifecycle_row(&e.source, e.title.as_deref())
}

/// Row-level primitive behind [`is_lifecycle`], usable where only a raw
/// `(source, title)` pair is on hand (a SQL row, not a full `InferEvent`) —
/// `billing::work_folder_for_block` skips the same lifecycle riders from
/// its folder vote via this.
pub(crate) fn is_lifecycle_row(source: &str, title: Option<&str>) -> bool {
    source == "claude"
        && title.is_some_and(|t| {
            t.starts_with("SessionStart") || t.starts_with("Stop") || t.starts_with("SessionEnd")
        })
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

/// (minute, lane key, is the owner acting, minutes that action holds focus).
pub(crate) type Keyed = (i64, String, bool, i64);

pub(crate) fn build_blocks_by_project(
    mut events: Vec<InferEvent>,
    build: fn(Vec<InferEvent>) -> Vec<InferBlock>,
) -> Vec<InferBlock> {
    // Background noise (R3) never creates time: an isolated `claude_work`
    // heartbeat, and duplicate `shell` rows a flaky collector logged twice,
    // are dropped/collapsed before anything else sees the day's events.
    events = crate::infer_evidence::drop_isolated_claude_work(events);
    events = crate::infer_evidence::dedupe_shell_events(events);

    // Helper/session-message activity (D-05, FR-16, FR-17) is the owner's
    // tool working on its own behalf, not the owner acting — it must never
    // vote on a lane's owner and must add no time to any block, so it is
    // dropped before any clustering pass sees it.
    events.retain(|e| {
        !matches!(
            e.source.as_str(),
            crate::clues_contract::SOURCE_CLAUDE_HELPER
                | crate::clues_contract::SOURCE_CLAUDE_MESSAGE
                | crate::clues_contract::SOURCE_CLAUDE_TOOL
        )
    });

    // With no project-tagged event anywhere in the day there is no lane
    // to speak of — every event is folderless, and D-08's "ride inside an
    // existing span" has no span to ride inside. Cluster them all the
    // ordinary way, exactly as before spec 006.
    if !events
        .iter()
        .any(|e| !e.is_calendar() && lane_key(e).is_some())
    {
        return build(events);
    }

    // A folderless event (D-08, FR-07, FR-08, FR-09) never decides a
    // block's project or bounds — it can only ride inside a span a
    // project event already established. Pull every one out before any
    // clustering pass can see it, and place it afterward without moving
    // anything.
    // A lifecycle event (R3) rides like a folderless one — pulled out here
    // so it never votes and never holds a run's bounds open, but still
    // links into whatever block ends up covering its timestamp.
    let (folderless, events): (Vec<InferEvent>, Vec<InferEvent>) = events
        .into_iter()
        .partition(|e| !e.is_calendar() && (lane_key(e).is_none() || is_lifecycle(e)));

    place_folderless(build_project_blocks(events, build), folderless)
}

fn build_project_blocks(
    events: Vec<InferEvent>,
    build: fn(Vec<InferEvent>) -> Vec<InferBlock>,
) -> Vec<InferBlock> {
    let keyed: Vec<Keyed> = events
        .iter()
        .filter(|e| !e.is_calendar())
        .filter_map(|e| {
            lane_key(e).map(|k| {
                let human = is_human(&e.source);
                let window = if human {
                    focus_window_minutes(&e.source)
                } else {
                    0
                };
                (minute(e.ts), k, human, window)
            })
        })
        .collect();
    if keyed
        .iter()
        .map(|(_, k, _, _)| k)
        .collect::<BTreeSet<_>>()
        .len()
        < 2
    {
        return build(events);
    }
    let runs =
        crate::infer_evidence::merge_by_evidence(fold_short_runs(owner_runs(&keyed)), &keyed);
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
    // A run whose own project shows up in fewer than 2 distinct minutes
    // has no real evidence behind it (R5) and is dropped instead.
    let mut blocks: Vec<InferBlock> = runs
        .iter()
        .enumerate()
        .filter_map(|(i, (owner, s, e))| {
            let evs = buckets.remove(&i).unwrap_or_default();
            if !crate::infer_evidence::has_evidence_floor(&evs) {
                return None;
            }
            let own = by_key.get(owner).map(Vec::as_slice).unwrap_or(&[]);
            let at = |m: i64| DateTime::from_timestamp(m * 60, 0);
            crate::infer_allocations::span_block(evs, at(*s)?, at(*e + 1)?, own, false)
        })
        .collect();
    blocks.extend(build(calendar));
    blocks.extend(build(leftovers));
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

/// Link each folderless event into the non-calendar block whose span
/// already contains its timestamp, never moving that block's start, end
/// or duration (D-08, FR-07, FR-08). One outside every span joins no
/// block and creates none of its own (FR-09).
fn place_folderless(mut blocks: Vec<InferBlock>, folderless: Vec<InferEvent>) -> Vec<InferBlock> {
    for e in folderless {
        if let Some(b) = blocks
            .iter_mut()
            .find(|b| !b.is_calendar && e.ts >= b.started_at && e.ts < b.ended_at)
        {
            crate::infer::attach_riding_event(b, e);
        }
    }
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
    let first = keyed.iter().map(|(m, _, _, _)| *m).min().unwrap_or(0);
    let last = keyed.iter().map(|(m, _, _, _)| *m).max().unwrap_or(0);
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
        for (t, k, _, _) in keyed {
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

/// Project of the owner's most recent action in (m − window, m], preferring
/// work: the latest work action wins; a personal action only when no work
/// action is in the window. Each action's own window is per-source (R1).
fn latest_human(keyed: &[Keyed], m: i64) -> Option<String> {
    let recent = |work: bool| {
        keyed
            .iter()
            .filter(|(t, k, human, window)| {
                *human && *t <= m && m - t <= *window && is_work(k) == work
            })
            .max_by_key(|(t, _, _, _)| *t)
            .map(|(_, k, _, _)| k.clone())
    };
    recent(true).or_else(|| recent(false))
}

/// Fold runs under `MIN_RUN_MINUTES` into a touching neighbour of the same
/// kind (work into work, personal into personal — never across) purely by
/// length, before `infer_evidence::merge_by_evidence`'s 15-min
/// evidence-based pass runs on the result. A tiny multi-minute cluster of
/// alternating short runs (a genai-infra blip inside a LibreChat session,
/// say) collapses into one run here first, so the evidence pass judges it
/// as a whole instead of as several sub-5-minute fragments.
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
