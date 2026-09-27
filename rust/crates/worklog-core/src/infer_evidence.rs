//! Evidence-based passes over the minute-ownership lanes (`infer_lanes`):
//! background noise is filtered before it can vote or add time (R3), and a
//! short run's own evidence — not just its length — decides whether it
//! survives, folds into a neighbour, or is dropped outright (R4). A
//! finished run also needs real evidence of its own before it's billed at
//! all (R5).

use std::collections::{BTreeSet, HashSet};

use crate::infer::InferEvent;
use crate::infer_lanes::{is_work, minute, Keyed};

/// Minutes after a session's last prompt (`claude_turn`) that a
/// `claude_work` background marker still counts as "still working the
/// prompt", not noise.
const HEARTBEAT_RECENT_MINUTES: i64 = 60;
/// A `claude_work` marker with another marker of the same session within
/// this many minutes is "dense" — real, continuing background work, not an
/// isolated ping.
const HEARTBEAT_DENSE_MINUTES: i64 = 3;
/// A run shorter than this many minutes is judged on its own evidence
/// instead of surviving purely on length (R4).
const SLIVER_MINUTES: i64 = 15;
/// A sliver holding this many or fewer of its own project's owner
/// (`is_human`) events is still weak enough to fold into a neighbour.
const MAX_OWN_HUMAN_EVENTS: usize = 2;
/// A finished block needs its own project's events in at least this many
/// distinct minutes to be billed at all (R5).
const MIN_EVIDENCE_MINUTES: usize = 2;

/// R3: a `claude_work` heartbeat with no sibling marker of its own session
/// within ±3 min AND more than 60 min after that session's last prompt (or
/// no prompt that day) never happened as far as the lane algorithm is
/// concerned — it can't vote and it can't hold a minute open.
pub(crate) fn drop_isolated_claude_work(events: Vec<InferEvent>) -> Vec<InferEvent> {
    let mut turns: std::collections::HashMap<String, Vec<i64>> = Default::default();
    let mut work: std::collections::HashMap<String, Vec<i64>> = Default::default();
    for e in &events {
        let Some(sid) = e.session_id.clone() else {
            continue;
        };
        match e.source.as_str() {
            "claude_turn" => turns.entry(sid).or_default().push(minute(e.ts)),
            "claude_work" => work.entry(sid).or_default().push(minute(e.ts)),
            _ => {}
        }
    }
    for v in turns.values_mut() {
        v.sort_unstable();
    }
    for v in work.values_mut() {
        v.sort_unstable();
    }
    events
        .into_iter()
        .filter(|e| {
            if e.source != "claude_work" {
                return true;
            }
            let Some(sid) = e.session_id.as_deref() else {
                return true;
            };
            let m = minute(e.ts);
            let dense = work
                .get(sid)
                .map(|siblings| {
                    siblings
                        .iter()
                        .filter(|&&x| (x - m).abs() <= HEARTBEAT_DENSE_MINUTES)
                        .count()
                        > 1
                })
                .unwrap_or(false);
            let recent = turns
                .get(sid)
                .and_then(|t| t.iter().rev().find(|&&x| x <= m))
                .is_some_and(|&t| m - t <= HEARTBEAT_RECENT_MINUTES);
            dense || recent
        })
        .collect()
}

/// R3 (data hygiene): a flaky `shell` collector sometimes logs the exact
/// same command (same second, same title, same cwd) more than once. Keep
/// only the first so duplicate rows can't inflate a run's own-event count
/// past `MAX_OWN_HUMAN_EVENTS` and save it from folding away.
pub(crate) fn dedupe_shell_events(events: Vec<InferEvent>) -> Vec<InferEvent> {
    let mut seen: HashSet<(i64, String, Option<String>)> = HashSet::new();
    events
        .into_iter()
        .filter(|e| {
            if e.source != "shell" {
                return true;
            }
            let key = (
                e.ts.timestamp(),
                e.title.clone().unwrap_or_default(),
                e.project_path.clone(),
            );
            seen.insert(key)
        })
        .collect()
}

/// R5: a run's block is only billed if its own project's events land in at
/// least `MIN_EVIDENCE_MINUTES` distinct minutes. `evs` is the run's
/// already-filtered own-project bucket (see `infer_lanes::build_project_blocks`).
pub(crate) fn has_evidence_floor(evs: &[InferEvent]) -> bool {
    evs.iter()
        .map(|e| minute(e.ts))
        .collect::<BTreeSet<_>>()
        .len()
        >= MIN_EVIDENCE_MINUTES
}

type Run = (String, i64, i64);

/// Count of a run's own-project events (`n`) and how many of those are the
/// owner acting (`h`, `is_human`), restricted to the run's own bounds.
fn own_stats(r: &Run, keyed: &[Keyed]) -> (usize, usize) {
    let mut n = 0usize;
    let mut h = 0usize;
    for (t, k, human, _) in keyed {
        if k == &r.0 && *t >= r.1 && *t <= r.2 {
            n += 1;
            if *human {
                h += 1;
            }
        }
    }
    (n, h)
}

/// R4: replace the length-only sliver fold with one driven by a run's own
/// evidence. A run shorter than `SLIVER_MINUTES` holding at most
/// `MAX_OWN_HUMAN_EVENTS` owner events of its own project merges into a
/// neighbour — into the shared owner for an A|b|A sandwich, otherwise into
/// the longer touching neighbour of the same class (work vs personal); a
/// run with zero events of its own project is dropped outright when no
/// such neighbour exists. A short run holding more owner events than that
/// stays untouched. Finishes by merging any now-touching same-owner runs.
pub(crate) fn merge_by_evidence(mut runs: Vec<Run>, keyed: &[Keyed]) -> Vec<Run> {
    while (0..runs.len()).any(|i| try_fold_sliver(&mut runs, i, keyed)) {}
    merge_touching_same_owner(runs)
}

/// Try to fold/drop the sliver at `i` in place. Returns `true` (and mutates
/// `runs`) the moment it makes a change, so the caller re-scans from the
/// top — indices shift on every merge/removal.
fn try_fold_sliver(runs: &mut Vec<Run>, i: usize, keyed: &[Keyed]) -> bool {
    let r = runs[i].clone();
    if r.2 - r.1 + 1 >= SLIVER_MINUTES {
        return false;
    }
    let (n, h) = own_stats(&r, keyed);
    if h > MAX_OWN_HUMAN_EVENTS {
        return false;
    }
    let prv = (i > 0 && runs[i - 1].2 + 1 == r.1).then(|| runs[i - 1].clone());
    let nxt = (i + 1 < runs.len() && r.2 + 1 == runs[i + 1].1).then(|| runs[i + 1].clone());

    // A|b|A sandwich: the same project on both sides absorbs the short run
    // between them, provided the sandwich is either the same work/personal
    // class as the short run or the short run has no human evidence at all.
    if let (Some(p), Some(nx)) = (&prv, &nxt) {
        if p.0 == nx.0 && (is_work(p.0.as_str()) == is_work(r.0.as_str()) || h == 0) {
            runs[i - 1].2 = nx.2;
            runs.remove(i + 1);
            runs.remove(i);
            return true;
        }
    }

    // Fold into the longer touching neighbour of the same class.
    // Python-`max`-style tie-break: keep the first (prv) on a tie.
    if let Some(idx) = longer_same_class_neighbour(i, &r, prv.as_ref(), nxt.as_ref()) {
        if idx < i {
            runs[idx].2 = r.2;
        } else {
            runs[idx].1 = r.1;
        }
        runs.remove(i);
        return true;
    }

    // No viable neighbour: a run with no evidence of its own is unowned
    // time — drop it. One with some evidence (just not enough to fold
    // anywhere) stays as-is.
    if n == 0 {
        runs.remove(i);
        return true;
    }
    false
}

/// Index of the longer touching neighbour that shares `r`'s work/personal
/// class, if any (ties keep `prv`, matching the prototype's `max()`).
fn longer_same_class_neighbour(
    i: usize,
    r: &Run,
    prv: Option<&Run>,
    nxt: Option<&Run>,
) -> Option<usize> {
    let mut best: Option<(usize, i64)> = None;
    for (idx, run) in [prv.map(|p| (i - 1, p)), nxt.map(|n| (i + 1, n))]
        .into_iter()
        .flatten()
    {
        if is_work(run.0.as_str()) != is_work(r.0.as_str()) {
            continue;
        }
        let len = run.2 - run.1;
        if best.is_none_or(|(_, b)| len > b) {
            best = Some((idx, len));
        }
    }
    best.map(|(idx, _)| idx)
}

/// Two touching runs (end + 1 == start) that ended up with the same owner
/// after evidence folding are one run, not two.
fn merge_touching_same_owner(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for r in runs {
        match out.last_mut() {
            Some(prev) if prev.0 == r.0 && r.1 - prev.2 <= 1 => prev.2 = r.2,
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
#[path = "infer_evidence_test.rs"]
mod tests;
