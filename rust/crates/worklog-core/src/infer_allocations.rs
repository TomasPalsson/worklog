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

/// Every event of the day per lane key, so a re-cut piece can link real
/// events of the project it was given (a block's project is read from its
/// linked events everywhere — billing, estimates, the day page).
pub(crate) fn events_by_key(events: &[InferEvent]) -> BTreeMap<String, Vec<InferEvent>> {
    let mut by_key: BTreeMap<String, Vec<InferEvent>> = BTreeMap::new();
    for e in events {
        if let Some(k) = crate::infer_lanes::lane_key(e) {
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
        // The parts outside the window stay the block's own.
        if b.started_at < ws {
            let evs = events_between(&b.events, b.started_at, ws);
            out.extend(piece(evs, b.started_at, ws, &b.events));
        }
        if b.ended_at > we {
            let evs = events_between(&b.events, we, b.ended_at);
            out.extend(piece(evs, we, b.ended_at, &b.events));
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
        out.extend(piece(evs, s, e, own));
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
/// events), so it still reads as that project's.
// ponytail: a piece under MIN_BLOCK (a 1–2% sliver) is dropped by
// finalize; fold slivers into a neighbour if that ever matters.
fn piece(
    mut events: Vec<InferEvent>,
    s: DateTime<Utc>,
    e: DateTime<Utc>,
    fallback: &[InferEvent],
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
    crate::infer::finalize(block)
}

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
#[path = "infer_allocations_db_test.rs"]
mod db_tests;
