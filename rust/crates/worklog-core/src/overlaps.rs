//! Where two work projects were both active at once.
//!
//! `infer_lanes` picks ONE owner per minute so blocks never double-bill —
//! but the owner sometimes wants to see the runner-up and rebalance the
//! split themselves ("I was mostly on vitinn-infra that hour, but put 30%
//! on lyfjastofnun too"). `day_overlaps` surfaces those windows; saved
//! choices live in `overlap_allocations` and are read back by
//! `infer_allocations` when blocks are (re-)built.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::infer::{load_day_events, InferEvent};
use crate::infer_lanes::{is_human, is_work, lane_key, WINDOW_MINUTES};

// Re-exported so daemon.rs (the only other caller) needs just `overlaps::*`
// for detecting overlaps, persisting the owner's chosen split, and feeding
// saved allocations back into `infer_allocations` on re-infer.
pub use crate::overlap_store::{delete_allocation, load_allocations, save_allocation};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OverlapProject {
    pub project: String,
    pub human_events: usize,
    pub background_events: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Allocation {
    pub shares: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Overlap {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub minutes: i64,
    pub projects: Vec<OverlapProject>,
    pub allocation: Option<Allocation>,
}

/// Every maximal ≥10-minute window on `day` where ≥2 WORK projects each had
/// activity, with the day's saved allocation (if any) attached.
pub fn day_overlaps(conn: &Connection, day: NaiveDate) -> Result<Vec<Overlap>> {
    let events = load_day_events(conn, day)?;
    let mut overlaps = compute_overlaps(&events);
    let allocations = load_allocations(conn, day)?;
    for overlap in &mut overlaps {
        if let Some((_, _, shares)) = allocations
            .iter()
            .find(|(s, e, _)| *s == overlap.started_at && *e == overlap.ended_at)
        {
            overlap.allocation = Some(Allocation {
                shares: shares.clone(),
            });
        }
    }
    Ok(overlaps)
}

/// One project's merged activity span — gaps inside it of `WINDOW_MINUTES`
/// or less are bridged, same threshold `infer_lanes` uses for "still the
/// same stretch of work".
struct Interval {
    project: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

/// Pure function so it's unit-testable without sqlite.
fn compute_overlaps(events: &[InferEvent]) -> Vec<Overlap> {
    let intervals = project_intervals(events);
    let windows = overlap_windows(&intervals);
    windows
        .into_iter()
        .filter(|(start, end)| (*end - *start).num_minutes() >= 10)
        .map(|(start, end)| {
            let projects: BTreeSet<&str> = intervals
                .iter()
                .filter(|iv| iv.start < end && iv.end > start)
                .map(|iv| iv.project.as_str())
                .collect();
            let projects = projects
                .into_iter()
                .map(|project| project_activity(events, project, start, end))
                .collect();
            Overlap {
                started_at: start,
                ended_at: end,
                minutes: (end - start).num_minutes(),
                projects,
                allocation: None,
            }
        })
        .collect()
}

fn project_activity(
    events: &[InferEvent],
    project: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> OverlapProject {
    let (human_events, background_events) = events
        .iter()
        .filter(|e| !e.is_calendar())
        .filter(|e| e.ts >= start && e.ts < end)
        .filter(|e| lane_key(e).as_deref() == Some(project))
        .fold((0usize, 0usize), |(h, b), e| {
            if is_human(&e.source) {
                (h + 1, b)
            } else {
                (h, b + 1)
            }
        });
    OverlapProject {
        project: project.to_string(),
        human_events,
        background_events,
    }
}

/// Merge each WORK project's events into gap-bridged activity intervals.
fn project_intervals(events: &[InferEvent]) -> Vec<Interval> {
    let mut by_project: BTreeMap<&str, Vec<&InferEvent>> = BTreeMap::new();
    let mut keys: BTreeMap<usize, String> = BTreeMap::new();
    // lane_key returns an owned String; keep them alive alongside the
    // borrow-by-index into `events` so `by_project` can key on `&str`.
    for (i, e) in events.iter().enumerate() {
        if e.is_calendar() {
            continue;
        }
        let Some(key) = lane_key(e) else { continue };
        if !is_work(&key) {
            continue;
        }
        keys.insert(i, key);
    }
    for (i, e) in events.iter().enumerate() {
        if let Some(key) = keys.get(&i) {
            by_project.entry(key.as_str()).or_default().push(e);
        }
    }

    let gap = chrono::Duration::minutes(WINDOW_MINUTES);
    let mut out = Vec::new();
    for (project, mut evs) in by_project {
        evs.sort_by_key(|e| e.ts);
        let mut cur: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
        for e in evs {
            let end = e.end();
            match cur {
                Some((s, prev_end)) if e.ts - prev_end <= gap => {
                    cur = Some((s, end.max(prev_end)));
                }
                Some((s, prev_end)) => {
                    out.push(Interval {
                        project: project.to_string(),
                        start: s,
                        end: prev_end,
                    });
                    cur = Some((e.ts, end));
                }
                None => cur = Some((e.ts, end)),
            }
        }
        if let Some((s, e)) = cur {
            out.push(Interval {
                project: project.to_string(),
                start: s,
                end: e,
            });
        }
    }
    out
}

/// Sweep the interval boundaries and merge every elementary sub-window with
/// ≥2 simultaneously-active projects into maximal windows.
fn overlap_windows(intervals: &[Interval]) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut points: BTreeSet<DateTime<Utc>> = BTreeSet::new();
    for iv in intervals {
        points.insert(iv.start);
        points.insert(iv.end);
    }
    let points: Vec<DateTime<Utc>> = points.into_iter().collect();

    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        let active = intervals
            .iter()
            .filter(|iv| iv.start <= a && iv.end >= b)
            .count();
        if active < 2 {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.1 == a {
                last.1 = b;
                continue;
            }
        }
        merged.push((a, b));
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
    }

    const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
    const B: &str = "/Users/dev/Desktop/Work/lyfjastofnun";
    const PERSONAL: &str = "/Users/dev/Desktop/Projects/worklog";
    // `lane_key`/`OverlapProject::project` are the folded folder name, not
    // the full path — same key `billing::work_folder_for_path` returns.
    const A_KEY: &str = "vitinn-infra";
    const B_KEY: &str = "lyfjastofnun";

    fn ev(h: u32, m: u32, source: &str, project: &str) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: source.into(),
            duration_seconds: None,
            jira_issue: None,
            event_id: None,
            project_path: Some(project.into()),
        }
    }

    #[test]
    fn two_work_projects_overlapping_for_an_hour_is_one_overlap() {
        let mut events: Vec<InferEvent> =
            (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
        events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", B)));
        let overlaps = compute_overlaps(&events);
        assert_eq!(overlaps.len(), 1, "got {overlaps:?}");
        let o = &overlaps[0];
        assert!(o.minutes >= 10);
        let projects: BTreeSet<&str> = o.projects.iter().map(|p| p.project.as_str()).collect();
        assert_eq!(projects, BTreeSet::from([A_KEY, B_KEY]));
    }

    #[test]
    fn a_single_project_day_has_no_overlap() {
        let events: Vec<InferEvent> = (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
        assert!(compute_overlaps(&events).is_empty());
    }

    #[test]
    fn personal_activity_never_counts_toward_an_overlap() {
        let mut events: Vec<InferEvent> =
            (0..20).map(|i| ev(10, i * 3, "claude_turn", A)).collect();
        events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", PERSONAL)));
        assert!(
            compute_overlaps(&events).is_empty(),
            "personal activity must not create a WORK/WORK overlap"
        );
    }

    #[test]
    fn overlap_shorter_than_ten_minutes_is_dropped() {
        let mut events: Vec<InferEvent> =
            vec![ev(10, 0, "claude_turn", A), ev(10, 5, "claude_turn", A)];
        events.extend([ev(10, 0, "claude_work", B), ev(10, 5, "claude_work", B)]);
        assert!(compute_overlaps(&events).is_empty());
    }

    #[test]
    fn human_and_background_events_are_split_per_project() {
        let mut events: Vec<InferEvent> = (0..20)
            .map(|i| ev(10, i * 3, "claude_turn", A)) // human
            .collect();
        events.extend((0..20).map(|i| ev(10, i * 3, "claude_work", B))); // background
        let overlaps = compute_overlaps(&events);
        let a = overlaps[0]
            .projects
            .iter()
            .find(|p| p.project == A_KEY)
            .unwrap();
        assert_eq!(a.human_events, 20);
        assert_eq!(a.background_events, 0);
        let b = overlaps[0]
            .projects
            .iter()
            .find(|p| p.project == B_KEY)
            .unwrap();
        assert_eq!(b.human_events, 0);
        assert_eq!(b.background_events, 20);
    }

    #[test]
    fn day_overlaps_attaches_the_saved_allocation() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        for e in (0..20).map(|i| ev(10, i * 3, "claude_turn", A)) {
            crate::repo::upsert_event(
                &conn,
                &crate::models::Event::minimal(
                    &e.source,
                    format!("a{}", e.ts),
                    e.ts.to_rfc3339(),
                    "work",
                ),
            )
            .unwrap();
        }
        for e in (0..20).map(|i| ev(10, i * 3, "claude_work", B)) {
            let mut ev_row = crate::models::Event::minimal(
                &e.source,
                format!("b{}", e.ts),
                e.ts.to_rfc3339(),
                "work",
            );
            ev_row.project_path = Some(B.to_string());
            crate::repo::upsert_event(&conn, &ev_row).unwrap();
        }
        // Set project_path on the A rows too (Event::minimal leaves it None).
        conn.execute(
            &format!("UPDATE events SET project_path = '{A}' WHERE source = 'claude_turn'"),
            [],
        )
        .unwrap();

        // Save against the overlap's REAL boundaries (event timestamps
        // extended by InferEvent::end()'s credit, not a round hour).
        let before = day_overlaps(&conn, day).unwrap();
        assert_eq!(before.len(), 1);
        let window = &before[0];

        let mut shares = BTreeMap::new();
        shares.insert(A_KEY.to_string(), 0.7);
        shares.insert(B_KEY.to_string(), 0.3);
        save_allocation(&conn, day, window.started_at, window.ended_at, &shares).unwrap();

        let overlaps = day_overlaps(&conn, day).unwrap();
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].allocation.as_ref().unwrap().shares, shares);
    }
}
