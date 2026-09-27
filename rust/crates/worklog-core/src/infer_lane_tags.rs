//! Splits a folder-level run into per-customer sub-runs (B9): a customer
//! tag may only divide a folder's own minutes, never take a minute away
//! from — or hand one to — a competing folder. The folder-level block is
//! built and judged exactly as on an untagged day first; only then is it
//! divided.

use chrono::DateTime;
use std::collections::BTreeSet;

use crate::infer::{InferBlock, InferEvent};
use crate::infer_evidence::{merge_touching_same_owner, Run, MIN_EVIDENCE_MINUTES};
use crate::infer_lanes::{
    fold_short_runs, keyed_by, lane_folder, lane_key, minute, owner_runs, Keyed,
};

pub(crate) fn divide_blocks(blocks: Vec<InferBlock>, day: &[InferEvent]) -> Vec<InferBlock> {
    blocks
        .into_iter()
        .flat_map(|b| divide_block(b, day))
        .collect()
}

/// Divides a folder-level block into one block per customer sub-run. An
/// untagged event joins the sub-run covering its minute; a tagged one joins
/// the nearest sub-run of its own customer, or no block at all — so no block
/// ever holds two customers' tagged events. A block with no tagged event is
/// returned as-is.
fn divide_block(block: InferBlock, day: &[InferEvent]) -> Vec<InferBlock> {
    if block.events.iter().all(|e| e.lane_tag.is_none()) {
        return vec![block];
    }
    let Some(folder) = block.events.iter().find_map(lane_folder) else {
        return vec![block];
    };
    let last_minute = (block.ended_at.timestamp() - 1).div_euclid(60);
    let subs = split_one(&folder, minute(block.started_at), last_minute, day);
    let mut shares: Vec<Vec<InferEvent>> = vec![Vec::new(); subs.len()];
    for e in &block.events {
        if let Some(i) = sub_run_for(&subs, e) {
            shares[i].push(e.clone());
        }
    }
    if subs.len() == 1 && shares[0].len() == block.events.len() {
        return vec![block];
    }
    let folder_events: Vec<InferEvent> = day
        .iter()
        .filter(|e| lane_folder(e).as_deref() == Some(folder.as_str()))
        .cloned()
        .collect();
    let at = |m: i64| DateTime::from_timestamp(m * 60, 0);
    let last = subs.len() - 1;
    subs.iter()
        .zip(shares)
        .enumerate()
        .filter_map(|(i, ((owner, s, e), evs))| {
            let lo = if i == 0 { block.started_at } else { at(*s)? };
            let hi = if i == last {
                block.ended_at
            } else {
                at(*e + 1)?
            };
            let own: Vec<InferEvent> = folder_events
                .iter()
                .filter(|x| lane_key(x).as_ref() == Some(owner))
                .cloned()
                .collect();
            let own = if own.is_empty() { &folder_events } else { &own };
            crate::infer_allocations::span_block(evs, lo, hi, own, true)
        })
        .collect()
}

fn sub_run_for(subs: &[Run], e: &InferEvent) -> Option<usize> {
    let m = minute(e.ts);
    let dist = |r: &Run| (r.1 - m).max(m - r.2).max(0);
    let nearest = |pick: &dyn Fn(&Run) -> bool| {
        (0..subs.len())
            .filter(|&i| pick(&subs[i]))
            .min_by_key(|&i| dist(&subs[i]))
    };
    let covering = nearest(&|_| true)?;
    let key = lane_key(e);
    if e.lane_tag.is_none() || key.as_ref() == Some(&subs[covering].0) {
        return Some(covering);
    }
    nearest(&|r| key.as_ref() == Some(&r.0))
}

fn split_one(folder: &str, s: i64, e: i64, events: &[InferEvent]) -> Vec<Run> {
    let folder_events: Vec<InferEvent> = events
        .iter()
        .filter(|ev| lane_folder(ev).as_deref() == Some(folder))
        .cloned()
        .collect();
    let subset = keyed_by(&folder_events, lane_key);
    let single = |subset: &[crate::infer_lanes::Keyed]| {
        let owner = subset
            .first()
            .map(|(_, k, _, _)| k.clone())
            .unwrap_or_else(|| folder.to_string());
        vec![(owner, s, e)]
    };
    if subset
        .iter()
        .map(|(_, k, _, _)| k)
        .collect::<BTreeSet<_>>()
        .len()
        < 2
    {
        return single(&subset);
    }
    let sub_runs =
        crate::infer_evidence::merge_by_evidence(fold_short_runs(owner_runs(&subset)), &subset);
    let clamped: Vec<Run> = sub_runs
        .into_iter()
        .filter_map(|(k, rs, re)| {
            let (cs, ce) = (rs.max(s), re.min(e));
            (cs <= ce).then_some((k, cs, ce))
        })
        .collect();
    if clamped.is_empty() {
        return single(&subset);
    }
    fold_weak_sub_runs(tile(clamped, s, e), &subset)
}

/// Clamped sub-runs land inside `[s, e]` but rarely touch its edges or each
/// other — a gap goes to the preceding sub-run, so the sub-runs still tile
/// `[s, e]` exactly, the way the un-split run used to.
fn tile(mut runs: Vec<Run>, s: i64, e: i64) -> Vec<Run> {
    runs.sort_by_key(|r| r.1);
    let last = runs.len() - 1;
    runs[0].1 = s;
    runs[last].2 = e;
    for i in 1..runs.len() {
        if runs[i].1 > runs[i - 1].2 + 1 {
            runs[i - 1].2 = runs[i].1 - 1;
        }
    }
    merge_touching_same_owner(runs)
}

/// Distinct minutes `subset` has under `r`'s own key, inside `r`'s own
/// bounds — the same floor `infer_evidence::has_evidence_floor` checks.
fn own_minutes(r: &Run, subset: &[Keyed]) -> usize {
    subset
        .iter()
        .filter(|(t, k, _, _)| k == &r.0 && *t >= r.1 && *t <= r.2)
        .map(|(t, _, _, _)| *t)
        .collect::<BTreeSet<_>>()
        .len()
}

/// A sub-run below `MIN_EVIDENCE_MINUTES` would fail R5 on its own once
/// `build_project_blocks` gets to it — fold it into a neighbour (the one
/// after it, when it's first) instead of losing its minutes outright.
fn fold_weak_sub_runs(mut runs: Vec<Run>, subset: &[Keyed]) -> Vec<Run> {
    while runs.len() > 1 {
        let weak = runs
            .iter()
            .position(|r| own_minutes(r, subset) < MIN_EVIDENCE_MINUTES);
        let Some(i) = weak else {
            break;
        };
        if i == 0 {
            let r = runs.remove(0);
            runs[0].1 = r.1;
        } else {
            let r = runs.remove(i);
            runs[i - 1].2 = r.2;
        }
    }
    merge_touching_same_owner(runs)
}

#[cfg(test)]
#[path = "infer_lane_tags_test.rs"]
mod tests;
