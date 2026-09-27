//! Manual overrides for the automatic project split.
//!
//! The owner sometimes disagrees with how a stretch of the day was split
//! between work projects — "put more of that afternoon on vitinn-infra,
//! less on lyfjastofnun". An [`AllocationWindow`] records that choice.
//! [`apply_split`] runs after the automatic blocks are built: inside the
//! window it takes the WORK time those blocks cover and re-cuts it by
//! share. So a split only moves time between work projects — it never adds
//! time, never fills idle stretches, and never touches personal blocks.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use crate::infer::{InferBlock, InferEvent};

/// A user-chosen split of `[started_at, ended_at)`. The work time inside
/// is handed out to `shares` in alphabetical project order (a `BTreeMap`
/// iterates that way); the last project absorbs any rounding remainder.
#[derive(Debug, Clone)]
pub struct AllocationWindow {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub shares: BTreeMap<String, f64>,
}

/// A day's blocks with its saved splits applied — the ONE rebuild every
/// caller uses (daemon, `worklog infer`, `worklog day`), so no path can
/// quietly undo the owner's choice.
pub fn build_day_blocks(
    conn: &rusqlite::Connection,
    day: chrono::NaiveDate,
) -> anyhow::Result<Vec<InferBlock>> {
    let events = crate::infer::load_day_events(conn, day)?;
    let windows: Vec<AllocationWindow> = crate::overlaps::load_allocations(conn, day)?
        .into_iter()
        .map(|(started_at, ended_at, shares)| AllocationWindow {
            started_at,
            ended_at,
            shares,
        })
        .collect();
    Ok(crate::infer::build_blocks_with_allocations(
        events, &windows,
    ))
}

/// Every event of the day per lane folder, so a re-cut piece can link real
/// events of the project it was given (a block's project is read from its
/// linked events everywhere — billing, estimates, the day page). Allocation
/// windows are a folder-level concept, so this is folder-keyed, not
/// tagged-lane-keyed.
pub(crate) fn events_by_key(events: &[InferEvent]) -> BTreeMap<String, Vec<InferEvent>> {
    let mut by_key: BTreeMap<String, Vec<InferEvent>> = BTreeMap::new();
    for e in events {
        if let Some(k) = crate::infer_lanes::lane_folder(e) {
            by_key.entry(k).or_default().push(e.clone());
        }
    }
    by_key
}

/// Re-cut the automatic blocks inside every saved window (see module doc).
pub(crate) fn apply_split(
    mut blocks: Vec<InferBlock>,
    windows: &[AllocationWindow],
    by_key: &BTreeMap<String, Vec<InferEvent>>,
) -> Vec<InferBlock> {
    for w in windows {
        blocks = split_window(blocks, w, by_key);
    }
    // R6: undo a spurious cut a window's own before/inside/after slicing
    // can leave behind when the window's share is 100% the project that
    // already owned the time on both sides.
    blocks = crate::infer_evidence::merge_touching_same_project(blocks);
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

fn is_work_block(b: &InferBlock) -> bool {
    b.dominant_project_path()
        .is_some_and(|p| p.contains("/Desktop/Work/"))
}

fn events_between(events: &[InferEvent], s: DateTime<Utc>, e: DateTime<Utc>) -> Vec<InferEvent> {
    events
        .iter()
        .filter(|x| x.ts >= s && x.ts < e)
        .cloned()
        .collect()
}

fn split_window(
    blocks: Vec<InferBlock>,
    w: &AllocationWindow,
    by_key: &BTreeMap<String, Vec<InferEvent>>,
) -> Vec<InferBlock> {
    let (ws, we) = (w.started_at, w.ended_at);
    let mut out = Vec::new();
    let mut inside: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut folderless: Vec<InferEvent> = Vec::new();
    for b in blocks {
        if !is_work_block(&b) || b.ended_at <= ws || b.started_at >= we {
            out.push(b);
            continue;
        }
        // The parts outside the window stay the block's own. `keep_thin`:
        // this piece is a remainder of an already-approved block, not a
        // fresh lane run — it must never vanish for being short (R4/R5's
        // "never lose owned minutes" invariant applies after a split too).
        if b.started_at < ws {
            let evs = events_between(&b.events, b.started_at, ws);
            out.extend(span_block(evs, b.started_at, ws, &b.events, true));
        }
        if b.ended_at > we {
            let evs = events_between(&b.events, we, b.ended_at);
            out.extend(span_block(evs, we, b.ended_at, &b.events, true));
        }
        let (s, e) = (b.started_at.max(ws), b.ended_at.min(we));
        inside.push((s, e));
        folderless.extend(
            events_between(&b.events, s, e)
                .into_iter()
                .filter(|x| x.project_path.is_none()),
        );
    }
    for (project, s, e) in split_intervals(&inside, &w.shares) {
        let own = by_key.get(&project).map(Vec::as_slice).unwrap_or(&[]);
        let mut evs = events_between(own, s, e);
        evs.extend(events_between(&folderless, s, e));
        out.extend(span_block(evs, s, e, own, true));
    }
    out
}

/// Hand the concatenated time of `intervals` out by share, in time order:
/// the first project gets the first `share` of it, and so on. Gaps between
/// intervals stay gaps. Returns `(project, start, end)` pieces.
fn split_intervals(
    intervals: &[(DateTime<Utc>, DateTime<Utc>)],
    shares: &BTreeMap<String, f64>,
) -> Vec<(String, DateTime<Utc>, DateTime<Utc>)> {
    let mut iv = intervals.to_vec();
    iv.sort();
    let total: i64 = iv.iter().map(|(s, e)| (*e - *s).num_seconds()).sum();
    let mut out = Vec::new();
    let (mut from, mut cum) = (0i64, 0.0);
    for (i, (project, frac)) in shares.iter().enumerate() {
        cum += frac;
        let to = if i + 1 == shares.len() {
            total
        } else {
            ((cum * total as f64).round() as i64).clamp(from, total)
        };
        // Map [from, to) of the concatenated time back onto real intervals.
        let mut offset = 0i64;
        for (s, e) in &iv {
            let len = (*e - *s).num_seconds();
            let (a, b) = (from.max(offset), to.min(offset + len));
            if a < b {
                out.push((
                    project.clone(),
                    *s + Duration::seconds(a - offset),
                    *s + Duration::seconds(b - offset),
                ));
            }
            offset += len;
        }
        from = to;
    }
    out
}

/// A block spanning exactly `[s, e)` with `events` linked. A piece with
/// none of its own links the nearest of `fallback` (the project's other
/// events), so it still reads as that project's. Behind the original
/// lane-run gate (`keep_thin=false`) a too-short piece is still dropped —
/// that's the one point deciding whether a run has enough evidence to
/// exist at all. Everywhere a *already-approved* block gets re-cut
/// afterwards (an allocation window's before/after remainder, a
/// ticket-edge cut) must pass `keep_thin=true`: that block's minutes
/// already cleared every evidence/length gate once, so re-slicing it
/// must never make a remainder vanish (R4/R5's "never lose owned
/// minutes" invariant applies after a split exactly as it does before
/// one).
pub(crate) fn span_block(
    mut events: Vec<InferEvent>,
    s: DateTime<Utc>,
    e: DateTime<Utc>,
    fallback: &[InferEvent],
    keep_thin: bool,
) -> Option<InferBlock> {
    if !events.iter().any(|x| x.project_path.is_some()) {
        let nearest = fallback
            .iter()
            .filter(|x| x.project_path.is_some())
            .min_by_key(|x| (x.ts - s).num_seconds().abs())?;
        events.push(nearest.clone());
    }
    events.sort_by_key(|x| x.ts);
    let (first, rest) = events.split_first()?;
    let mut block = crate::infer::new_block(first);
    rest.iter()
        .for_each(|x| crate::infer::extend_block(&mut block, x));
    block.started_at = s;
    block.ended_at = e;
    block.duration_seconds = (e - s).num_seconds();
    crate::infer::finalize_ext(block, !keep_thin)
}

#[cfg(test)]
#[path = "infer_allocations_test.rs"]
mod tests;

#[cfg(test)]
#[path = "infer_allocations_db_test.rs"]
mod db_tests;
