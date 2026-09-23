//! Parallel projects get parallel blocks.
//!
//! The owner often has two sessions going at once (e.g. vitinn-infra in one
//! terminal while a background job builds worklog in another). One timeline
//! of interleaved events fused both into a single block owned by whichever
//! project happened to dominate. Here each repo gets its own lane: events are
//! grouped by repo root (worktrees folded in), each lane is clustered with
//! the existing gap-timeout algorithm, and the lanes' blocks are merged back
//! in time order. Overlap between lanes is fine — billing unions intervals.

use chrono::Duration;
use std::collections::BTreeMap;

use crate::billing::work_folder_for_path;
use crate::infer::{InferBlock, InferEvent};

/// How far an event with no folder (github, jira) may sit from a project
/// event and still join that project's lane.
const ATTACH_WINDOW_MINUTES: i64 = 30;

/// Lane key for an event: its repo folder, `None` for folderless events.
fn lane_key(e: &InferEvent) -> Option<String> {
    e.project_path
        .as_deref()
        .map(|p| work_folder_for_path(p).unwrap_or_else(|| p.to_string()))
}

/// Group events into per-repo lanes, attach folderless events to the lane
/// active nearest in time, build each lane with `build`, and merge.
pub(crate) fn build_blocks_by_project(
    events: Vec<InferEvent>,
    build: fn(Vec<InferEvent>) -> Vec<InferBlock>,
) -> Vec<InferBlock> {
    let keyed: Vec<(i64, String)> = events
        .iter()
        .filter_map(|e| lane_key(e).map(|k| (e.ts.timestamp(), k)))
        .collect();
    let distinct: std::collections::BTreeSet<&String> = keyed.iter().map(|(_, k)| k).collect();
    if distinct.len() < 2 {
        return build(events);
    }

    let mut lanes: BTreeMap<String, Vec<InferEvent>> = BTreeMap::new();
    for e in events {
        let key = if e.is_calendar() {
            "\u{0}calendar".to_string()
        } else if let Some(k) = lane_key(&e) {
            k
        } else {
            nearest_lane(e.ts.timestamp(), &keyed).unwrap_or_else(|| "\u{0}none".to_string())
        };
        lanes.entry(key).or_default().push(e);
    }

    let mut blocks: Vec<InferBlock> = lanes.into_values().flat_map(build).collect();
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

fn nearest_lane(ts: i64, keyed: &[(i64, String)]) -> Option<String> {
    let window = Duration::minutes(ATTACH_WINDOW_MINUTES).num_seconds();
    keyed
        .iter()
        .map(|(t, k)| ((t - ts).abs(), k))
        .filter(|(d, _)| *d <= window)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k.clone())
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

    #[test]
    fn interleaved_projects_get_one_block_each() {
        let mut events = Vec::new();
        for i in 0..30 {
            events.push(ev(12, i * 2, "claude_turn", Some(if i % 2 == 0 { A } else { A_WT })));
            events.push(ev(12, i * 2 + 1, "claude_turn", Some(B)));
        }
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 2, "one block per repo, worktree folded into its repo");
        for b in &blocks {
            let mins = b.duration_seconds / 60;
            assert!((50..=65).contains(&mins), "each lane spans the hour, got {mins}m");
        }
    }

    #[test]
    fn folderless_event_joins_the_nearest_lane() {
        let mut events: Vec<InferEvent> = (0..10).map(|i| ev(9, i * 3, "claude_turn", Some(A))).collect();
        events.extend((0..10).map(|i| ev(14, i * 3, "claude_turn", Some(B))));
        events.push(ev(9, 20, "github_pr", None));
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 2);
        let morning = blocks.iter().find(|b| b.started_at.format("%H").to_string() == "09").unwrap();
        assert_eq!(morning.dominant_project_path().as_deref(), Some(A));
    }

    #[test]
    fn a_single_project_day_is_unchanged() {
        let events: Vec<InferEvent> = (0..10).map(|i| ev(9, i * 2, "shell", Some(A))).collect();
        assert_eq!(build_blocks(events).len(), 1);
    }
}
