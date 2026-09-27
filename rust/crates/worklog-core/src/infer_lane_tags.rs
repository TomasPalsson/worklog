//! Splits a folder-level run into per-customer sub-runs (B9): a customer
//! tag may only divide a folder's own minutes, never take a minute away
//! from — or hand one to — a competing folder.

use std::collections::BTreeSet;

use crate::infer::InferEvent;
use crate::infer_evidence::MIN_EVIDENCE_MINUTES;
use crate::infer_lanes::{fold_short_runs, keyed_by, lane_folder, lane_key, owner_runs, Keyed};

type Run = (String, i64, i64);

pub(crate) fn split_runs_by_tag(runs: Vec<Run>, events: &[InferEvent]) -> Vec<Run> {
    runs.into_iter()
        .flat_map(|(folder, s, e)| split_one(&folder, s, e, events))
        .collect()
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
    merge_touching(runs)
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
    merge_touching(runs)
}

fn merge_touching(runs: Vec<Run>) -> Vec<Run> {
    let mut merged: Vec<Run> = Vec::new();
    for r in runs {
        match merged.last_mut() {
            Some(prev) if prev.0 == r.0 && r.1 - prev.2 <= 1 => prev.2 = r.2,
            _ => merged.push(r),
        }
    }
    merged
}

#[cfg(test)]
#[path = "infer_lane_tags_test.rs"]
mod tests;
