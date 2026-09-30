//! Gap-timeout block clustering.
//!
//! `build_blocks()` is a pure function over an event list;
//! `persist_blocks()` writes them to the db. Separated so tests can exercise
//! the clustering algorithm without any SQLite setup.
//!
//! Invariants:
//! * Calendar events are authoritative closed units — they never absorb
//!   code/commit activity and never get absorbed by it.
//! * Re-inference preserves `tempo_worklog_id`, `description`, and
//!   `estimated_by` per-block-start so syncing and AI estimates survive.
//! * Blocks shorter than MIN_BLOCK are dropped; longer than MAX_BLOCK are
//!   flagged for human review.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// Bumped from 20 → 30. AI-paired coding has long autonomous stretches
// where Claude runs tools for 20+ min without firing a UserPromptSubmit/Stop
// pair; 20 split those into dropped sub-MIN_BLOCK slivers. 30 keeps the
// session intact while still closing on genuine breaks. Paired with
// PreToolUse/PostToolUse hooks (see hook::EVENTS) which add heartbeats
// during autonomous work.
const TIMEOUT_MINUTES: i64 = 30;
const CREDIT_MINUTES: i64 = 2;
const MIN_BLOCK_MINUTES: i64 = 5;
const MAX_BLOCK_MINUTES: i64 = 4 * 60;

/// Event sources that are treated as authoritative calendar blocks.
pub fn is_calendar_source(s: &str) -> bool {
    matches!(s, "gcal")
}

/// A single event as consumed by the clustering algorithm. Kept separate
/// from [`crate::models::Event`] so we can drive tests without an owned
/// connection.
#[derive(Debug, Clone)]
pub struct InferEvent {
    pub ts: DateTime<Utc>,
    pub source: String,
    pub duration_seconds: Option<i64>,
    pub jira_issue: Option<String>,
    pub event_id: Option<i64>,
    /// cwd captured by the Claude hook (NULL for github/jira/gcal events).
    /// Drives the project-aware split inside `build_blocks` so two
    /// concurrent projects don't get fused into a single worklog entry.
    pub project_path: Option<String>,
    /// Claude session this event belongs to (NULL for non-Claude sources).
    /// Drives `infer_evidence::drop_isolated_claude_work` (R3): a
    /// background `claude_work` heartbeat is judged against its own
    /// session's prompts and siblings, never another session's.
    pub session_id: Option<String>,
    /// Event title. For `claude_turn`, this is the owner's prompt text
    /// read from `raw_json` when it deserialises, else the DB title —
    /// every `claude_turn` row is titled "prompt" in the DB. Drives
    /// `infer_evidence::dedupe_shell_events` (a flaky collector logging
    /// the exact same `shell` command twice).
    pub title: Option<String>,
    /// The Claude session's resolved customer, when its repo folder splits into separate lanes per customer (`infer_lanes::lane_key`); `None` otherwise.
    pub lane_tag: Option<String>,
}

impl InferEvent {
    pub fn end(&self) -> DateTime<Utc> {
        if is_calendar_source(&self.source) {
            if let Some(d) = self.duration_seconds {
                return self.ts + Duration::seconds(d);
            }
        }
        self.ts + Duration::minutes(CREDIT_MINUTES)
    }

    pub fn is_calendar(&self) -> bool {
        is_calendar_source(&self.source)
    }
}

/// A finalized block ready for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferBlock {
    pub day: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_seconds: i64,
    pub event_count: u32,
    pub event_ids: Vec<i64>,
    pub jira_issue: Option<String>,
    pub flagged: bool,
    /// Track the source kind so the clustering pass can refuse to extend
    /// a calendar block. Skipped in serialization; not needed on disk.
    /// `pub(crate)` so `infer_lanes` can refuse to place a folderless
    /// event into a calendar block.
    #[serde(skip)]
    pub(crate) is_calendar: bool,
    #[serde(skip)]
    pub(crate) events: Vec<InferEvent>,
}

impl InferBlock {
    /// Most common `project_path` across the block's events. Used by
    /// `persist_blocks` to feed `personal::PersonalConfig::classify`.
    /// Events with `project_path = None` (gcal/github/jira) don't count —
    /// they ride with whichever cwd dominates from Claude hooks.
    pub fn dominant_project_path(&self) -> Option<String> {
        let mut counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        for e in &self.events {
            // A lifecycle rider (R3) never voted on which lane owns a
            // minute; it must never vote on which project the block IS
            // either, or a handful of unrelated SessionStart/SessionEnd
            // pings can outnumber the block's real events.
            if crate::infer_lanes::is_lifecycle(e) {
                continue;
            }
            if let Some(p) = &e.project_path {
                *counts.entry(p.as_str()).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(p, _)| p.to_owned())
    }
}

pub(crate) fn new_block(e: &InferEvent) -> InferBlock {
    let end = e.end();
    InferBlock {
        // Bucket the block on the user's LOCAL day (driven by
        // `$WORKLOG_TZ`; defaults to UTC). Without this a developer in
        // UTC-5 sees 23:00-local work land on tomorrow's page.
        day: crate::tz::local_date(e.ts).to_string(),
        started_at: e.ts,
        ended_at: end,
        duration_seconds: (end - e.ts).num_seconds(),
        event_count: 1,
        event_ids: e.event_id.into_iter().collect(),
        jira_issue: e.jira_issue.clone(),
        flagged: false,
        is_calendar: e.is_calendar(),
        events: vec![e.clone()],
    }
}

pub(crate) fn extend_block(block: &mut InferBlock, e: &InferEvent) {
    let end = e.end().max(block.ended_at);
    block.ended_at = end;
    block.duration_seconds = (end - block.started_at).num_seconds();
    block.event_count += 1;
    if let Some(id) = e.event_id {
        block.event_ids.push(id);
    }
    block.events.push(e.clone());
}

/// Link `e` into `block` without moving `started_at`/`ended_at`/
/// `duration_seconds` — for an event that must ride along inside a span
/// it never gets to decide (a folderless event placed by `infer_lanes`,
/// D-08, FR-07, FR-08, FR-09).
pub(crate) fn attach_riding_event(block: &mut InferBlock, e: InferEvent) {
    block.event_count += 1;
    if let Some(id) = e.event_id {
        block.event_ids.push(id);
    }
    block.events.push(e);
    block.jira_issue = unique_jira_issue(&block.events);
}

/// The one `jira_issue` shared by every event that carries one; `None`
/// if there isn't exactly one. Shared by `finalize`, `build_sub_block`
/// and `attach_riding_event` so a block's ticket is always computed the
/// same way regardless of which pass last touched its events.
fn unique_jira_issue(events: &[InferEvent]) -> Option<String> {
    let issues: HashSet<String> = events.iter().filter_map(|e| e.jira_issue.clone()).collect();
    if issues.len() == 1 {
        issues.into_iter().next()
    } else {
        None
    }
}

pub(crate) fn finalize(block: InferBlock) -> Option<InferBlock> {
    finalize_ext(block, true)
}

/// `enforce_min=false` skips the `MIN_BLOCK_MINUTES` drop — for a piece
/// re-cut from an already-approved block (an allocation window's
/// before/after remainder, a ticket-edge cut) whose minutes must never
/// vanish just for being a short remainder (see `span_block`'s doc).
/// Still refuses a zero/negative span (a malformed cut, never real time).
pub(crate) fn finalize_ext(mut block: InferBlock, enforce_min: bool) -> Option<InferBlock> {
    let duration = block.ended_at - block.started_at;
    if duration <= Duration::zero() {
        return None;
    }
    if enforce_min && duration < Duration::minutes(MIN_BLOCK_MINUTES) {
        return None;
    }
    if duration > Duration::minutes(MAX_BLOCK_MINUTES) {
        block.flagged = true;
    }
    block.jira_issue = unique_jira_issue(&block.events);
    Some(block)
}

/// Pure clustering over a day's events. Input order doesn't matter.
/// One lane per repo (see `infer_lanes`), each clustered by
/// [`build_blocks_sequential`].
pub fn build_blocks(events: Vec<InferEvent>) -> Vec<InferBlock> {
    build_blocks_with_allocations(events, &[])
}

/// Same as [`build_blocks`], then the work time inside each saved
/// [`crate::infer_allocations::AllocationWindow`] is re-cut by its shares.
pub fn build_blocks_with_allocations(
    events: Vec<InferEvent>,
    allocations: &[crate::infer_allocations::AllocationWindow],
) -> Vec<InferBlock> {
    if allocations.is_empty() {
        return crate::infer_lanes::build_blocks_by_project(events, build_blocks_sequential);
    }
    let by_key = crate::infer_allocations::events_by_key(&events);
    let auto = crate::infer_lanes::build_blocks_by_project(events, build_blocks_sequential);
    crate::infer_allocations::apply_split(auto, allocations, &by_key)
}

fn build_blocks_sequential(events: Vec<InferEvent>) -> Vec<InferBlock> {
    let mut usable = events;
    usable.sort_by_key(|e| e.ts);

    let mut blocks: Vec<InferBlock> = Vec::new();
    let mut current: Option<InferBlock> = None;

    for e in usable {
        let Some(mut c) = current.take() else {
            current = Some(new_block(&e));
            continue;
        };

        let gap = e.ts - c.ended_at;
        let closed = e.is_calendar() || c.is_calendar || gap > Duration::minutes(TIMEOUT_MINUTES);
        if closed {
            if let Some(f) = finalize(c) {
                blocks.extend(split_by_project(f));
            }
            current = Some(new_block(&e));
        } else {
            extend_block(&mut c, &e);
            current = Some(c);
        }
    }
    if let Some(c) = current {
        if let Some(f) = finalize(c) {
            blocks.extend(split_by_project(f));
        }
    }
    blocks.sort_by_key(|b| b.started_at);
    blocks
}

/// Split a gap-cluster at project_path transitions where both sides have
/// at least `MIN_BLOCK_MINUTES` of work. Brief project ping-pongs (e.g.
/// a single message into a second project while waiting on the first)
/// absorb into the dominant block because their side fails the threshold.
///
/// Events with `project_path = None` (github/jira/gcal — sources that
/// don't carry a cwd) don't trigger a split; they ride with whatever
/// project they're sandwiched between.
fn split_by_project(block: InferBlock) -> Vec<InferBlock> {
    if block.is_calendar {
        return vec![block];
    }
    let segments = project_segments(&block.events);
    if segments.len() < 2 {
        return vec![block];
    }

    // Greedy: scan transitions in order, cut whenever the duration on
    // *both* sides crosses MIN_BLOCK_MINUTES. The "left side" duration
    // is the sum of seconds since the most recent cut; the "right side"
    // is the segment-runlength of the new project starting at this
    // transition.
    let min_seconds = MIN_BLOCK_MINUTES * 60;
    let mut cut_indices: Vec<usize> = Vec::new();
    let mut last_cut_event_idx = 0;

    for i in 1..segments.len() {
        // i indexes into segments; the first event of segment[i] is
        // segments[i].start_event_idx.
        let cut_event_idx = segments[i].start_event_idx;

        // Left = span from the previous cut to just before this event.
        let left_first_idx = last_cut_event_idx;
        let left_last_idx = cut_event_idx - 1;
        let left_secs = event_span_seconds(&block.events, left_first_idx, left_last_idx);

        // Right = span over segments[i..] until the *next* project
        // change (or end-of-block). That's the candidate new block's
        // bounded length — anything past that is a future split
        // decision.
        let mut right_last_seg = i;
        while right_last_seg + 1 < segments.len()
            && segments[right_last_seg + 1].project == segments[i].project
        {
            right_last_seg += 1;
        }
        let right_first_idx = segments[i].start_event_idx;
        let right_last_idx = if right_last_seg + 1 < segments.len() {
            segments[right_last_seg + 1].start_event_idx - 1
        } else {
            block.events.len() - 1
        };
        let right_secs = event_span_seconds(&block.events, right_first_idx, right_last_idx);

        if left_secs >= min_seconds && right_secs >= min_seconds {
            cut_indices.push(cut_event_idx);
            last_cut_event_idx = cut_event_idx;
        }
    }

    if cut_indices.is_empty() {
        return vec![block];
    }

    // Materialize sub-blocks from the cut indices.
    let mut starts = vec![0usize];
    starts.extend(cut_indices.iter().copied());
    let mut ends: Vec<usize> = starts[1..].iter().map(|&i| i - 1).collect();
    ends.push(block.events.len() - 1);

    starts
        .iter()
        .zip(ends.iter())
        .filter_map(|(&s, &e)| build_sub_block(&block, s, e))
        .collect()
}

/// Bucket consecutive events by their `project_path`. Events with
/// `None` project_path inherit the previous segment's project (so a
/// github commit in the middle of a sjukra session doesn't split
/// anything).
#[derive(Debug)]
struct ProjectSegment {
    project: Option<String>,
    start_event_idx: usize,
}

fn project_segments(events: &[InferEvent]) -> Vec<ProjectSegment> {
    let mut out: Vec<ProjectSegment> = Vec::new();
    let mut current: Option<String> = None;
    for (idx, e) in events.iter().enumerate() {
        let proj = e.project_path.clone().or_else(|| current.clone());
        if out.is_empty() || out.last().unwrap().project != proj {
            out.push(ProjectSegment {
                project: proj.clone(),
                start_event_idx: idx,
            });
        }
        current = proj;
    }

    // Collapse brief sandwiched interruptions: A(long) → B(<MIN) → A
    // means B was a quick ping into another project while the real work
    // stayed in A. Drop B's segment boundary so it stays with the
    // surrounding A run. Without this, a 2-min detour to another repo
    // creates spurious project transitions.
    collapse_brief_interruptions(events, out)
}

fn collapse_brief_interruptions(
    events: &[InferEvent],
    segments: Vec<ProjectSegment>,
) -> Vec<ProjectSegment> {
    let min_seconds = MIN_BLOCK_MINUTES * 60;
    let mut out: Vec<ProjectSegment> = Vec::with_capacity(segments.len());
    let mut i = 0;
    while i < segments.len() {
        // Sandwich check: need at least one segment before and after.
        if i > 0 && i + 1 < segments.len() {
            let before = &out.last().unwrap_or(&segments[i - 1]);
            let after = &segments[i + 1];
            let same_project_around = before.project == after.project;
            // Duration of segment i = span until segment i+1 begins.
            let seg_first = segments[i].start_event_idx;
            let seg_last = segments[i + 1].start_event_idx - 1;
            let seg_secs = event_span_seconds(events, seg_first, seg_last);
            if same_project_around && seg_secs < min_seconds {
                // Drop segment i; segment i+1 is identical to `before`,
                // so drop it too — its events extend the previous run.
                i += 2;
                continue;
            }
        }
        out.push(ProjectSegment {
            project: segments[i].project.clone(),
            start_event_idx: segments[i].start_event_idx,
        });
        i += 1;
    }
    out
}

fn event_span_seconds(events: &[InferEvent], first: usize, last: usize) -> i64 {
    if first > last {
        return 0;
    }
    let start = events[first].ts;
    let end = events[last].end();
    (end - start).num_seconds().max(0)
}

fn build_sub_block(parent: &InferBlock, first: usize, last: usize) -> Option<InferBlock> {
    if first > last {
        return None;
    }
    let slice: Vec<InferEvent> = parent.events[first..=last].to_vec();
    if slice.is_empty() {
        return None;
    }
    let started = slice.first().unwrap().ts;
    let ended = slice
        .iter()
        .map(|e| e.end())
        .max()
        .unwrap_or(started)
        .max(started);
    let jira_issue = unique_jira_issue(&slice);
    let duration = ended - started;
    if duration < Duration::minutes(MIN_BLOCK_MINUTES) {
        return None;
    }
    let event_ids: Vec<i64> = slice.iter().filter_map(|e| e.event_id).collect();
    let event_count = slice.len() as u32;
    Some(InferBlock {
        day: parent.day.clone(),
        started_at: started,
        ended_at: ended,
        duration_seconds: duration.num_seconds(),
        event_count,
        event_ids,
        jira_issue,
        flagged: duration > Duration::minutes(MAX_BLOCK_MINUTES),
        is_calendar: false,
        events: slice,
    })
}

// ───────────────────────── db glue ─────────────────────────

pub fn load_day_events(conn: &Connection, day: NaiveDate) -> Result<Vec<InferEvent>> {
    // `day` is interpreted in the user's local TZ (`$WORKLOG_TZ`); the
    // SQL range is the UTC window that covers it.
    let (start_utc, end_utc) = crate::tz::utc_window_for_local_day(day);
    let start = start_utc.to_rfc3339();
    let end = end_utc.to_rfc3339();
    // Firefox/Slack events without a label are unsorted (spec 003, routing
    // decision 3): they must never inherit a neighbour's project_path, so
    // they're excluded here rather than let through with project_path NULL.
    // Dismissed and noise events (thrown away by the owner or by the
    // end-of-day absorb step) are excluded the same way. An org commit/PR
    // whose sha is in no local clone is flagged `elsewhere` (FR-04) and
    // must never reach inference (FR-09, D-08, B3). Helper/tool/message
    // rows (D-05, FR-17) are the owner's tool working on its own behalf,
    // never the owner acting — excluded here, at the one loader every
    // caller (block-building AND overlaps/activity) shares, so a run of
    // subagent activity in another project can never open an overlap
    // window or show up as that project's activity either.
    let mut stmt = conn.prepare(
        "SELECT id, source, started_at, duration_seconds, jira_issue, project_path,
                session_id, title, raw_json
           FROM events
          WHERE started_at >= ?1 AND started_at < ?2
            AND NOT (source IN (?3, ?4) AND (label_origin IS NULL OR label_origin IN (?5, ?6)))
            AND elsewhere = 0
            AND source NOT IN (?7, ?8, ?9)
          ORDER BY started_at",
    )?;
    // started_at is ISO-8601 string; we compare lexicographically which works
    // because the format is fixed-width. Use the fixed-width prefix to match
    // the Python code exactly.
    let start = iso_prefix(&start);
    let end = iso_prefix(&end);
    let iter = stmt.query_map(
        params![
            start,
            end,
            crate::routing_contract::SOURCE_FIREFOX,
            crate::routing_contract::SOURCE_SLACK,
            crate::routing_contract::LabelOrigin::Dismissed.as_str(),
            crate::routing_contract::LabelOrigin::Noise.as_str(),
            crate::clues_contract::SOURCE_CLAUDE_HELPER,
            crate::clues_contract::SOURCE_CLAUDE_MESSAGE,
            crate::clues_contract::SOURCE_CLAUDE_TOOL,
        ],
        infer_event_row,
    )?;
    let mut events: Vec<InferEvent> = iter.collect::<Result<_, _>>()?;
    crate::infer_session_folder::fill_session_folders(&mut events);
    Ok(events)
}

/// Row mapper shared by `load_day_events`'s query — split out to keep the
/// query function itself under the line-count cap.
fn infer_event_row(r: &rusqlite::Row) -> rusqlite::Result<InferEvent> {
    let iso: String = r.get(2)?;
    let ts = chrono::DateTime::parse_from_rfc3339(&iso)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let source: String = r.get(1)?;
    let db_title: Option<String> = r.get(7)?;
    let raw_json = crate::raw_json::decode_raw_json(r, 8)?;
    let title = if source == "claude_turn" {
        raw_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<crate::clues_contract::RawRecord>(j).ok())
            .and_then(|rec| match rec {
                crate::clues_contract::RawRecord::ClaudePrompt { text, .. } => Some(text),
                _ => None,
            })
            .or(db_title)
    } else {
        db_title
    };
    Ok(InferEvent {
        event_id: Some(r.get(0)?),
        source,
        ts,
        duration_seconds: r.get(3)?,
        jira_issue: r.get(4)?,
        project_path: r.get(5)?,
        session_id: r.get(6)?,
        title,
        lane_tag: None,
    })
}

/// Use the same string form Python emits (`datetime.isoformat()` without
/// offset suffix) so the text comparison works either way. rfc3339 adds
/// `+00:00`; strip it for parity with the Python code.
fn iso_prefix(s: &str) -> String {
    s.trim_end_matches("+00:00").to_owned()
}

pub fn persist_blocks(conn: &Connection, day: NaiveDate, blocks: &[InferBlock]) -> Result<()> {
    let day_iso = day.to_string();
    // Load once per persist pass — cheap and avoids re-reading the TOML
    // for every block. classify() is pure given the config.
    let personal_cfg = crate::personal::PersonalConfig::load();

    // Collect "carry" state so re-inference preserves tempo_worklog_id,
    // description, estimated_by, and manual ticket edits. We keep two
    // views of the prior state: a strict started_at→CarryRow map for the
    // common case (no time shift), and an ordered Vec of (start, end, row)
    // for the fallback case where a backfilled earlier event shifts the
    // block's started_at. See `carry_for_block` for the matching rules.
    let mut prior: std::collections::HashMap<String, CarryRow> = std::collections::HashMap::new();
    let mut prior_list: Vec<CarryRow> = Vec::new();
    {
        // ORDER BY started_at matters: the overlap-fallback claims rows
        // in iteration order, so two new blocks that both overlap the
        // same prior would otherwise race nondeterministically for its
        // carry state (tempo_worklog_id in particular). Stable order
        // ensures the earliest-starting new block claims the earliest
        // prior.
        let mut stmt = conn.prepare(
            "SELECT started_at, ended_at, jira_issue, description, estimated_by, tempo_worklog_id, exported_at, described_seconds, ignored_at
               FROM blocks WHERE day = ?1 ORDER BY started_at",
        )?;
        let iter = stmt.query_map(params![day_iso], |r| {
            Ok(CarryRow {
                started_at: r.get(0)?,
                ended_at: r.get(1)?,
                jira_issue: r.get(2)?,
                description: r.get(3)?,
                estimated_by: r.get(4)?,
                tempo_worklog_id: r.get(5)?,
                exported_at: r.get(6)?,
                described_seconds: r.get(7)?,
                ignored_at: r.get(8)?,
            })
        })?;
        for row in iter {
            let row = row?;
            prior.insert(row.started_at.clone(), row.clone());
            prior_list.push(row);
        }
    }
    // Old blocks' spans, captured before the delete below, so hand-set
    // owner rows (block_customer_shares, block_resolution_snapshots) can
    // be carried onto whichever new block covers them most (see
    // infer_carry_shares).
    let old_spans: Vec<(String, String)> = prior_list
        .iter()
        .map(|c| (c.started_at.clone(), c.ended_at.clone()))
        .collect();
    // Two ticketed blocks fused into one would keep only one ticket: cut
    // such a block where each later ticketed block began (see infer_carry).
    let ticketed: Vec<crate::infer_carry::Ticketed> = prior_list
        .iter()
        .filter_map(|c| {
            let (s, e) = parse_pair(&c.started_at, &c.ended_at)?;
            Some((s, e, c.jira_issue.clone()?))
        })
        .collect();
    let blocks = crate::infer_carry::cut_at_ticket_edges(blocks, &ticketed);
    // Track which fallback rows we've already claimed so two new blocks
    // can't both inherit the same prior state.
    let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();

    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM blocks WHERE day = ?1", params![day_iso])
        .context("clearing stale blocks")?;

    let mut new_spans: Vec<(String, String)> = Vec::new();
    for b in &blocks {
        let started_key = block_iso(b.started_at);
        let ended_key = block_iso(b.ended_at);
        new_spans.push((started_key.clone(), ended_key.clone()));
        let carry: Option<&CarryRow> = prior.get(&started_key).or_else(|| {
            // Overlap fallback: if no exact-start match, find one prior
            // block whose time range overlaps the new block's — a ticketed
            // one first. Must not already be claimed by a different new block.
            let open = |c: &&CarryRow| {
                !claimed.contains(&c.started_at)
                    && ranges_overlap(&c.started_at, &c.ended_at, &started_key, &ended_key)
            };
            prior_list
                .iter()
                .filter(open)
                .find(|c| c.jira_issue.is_some())
                .or_else(|| prior_list.iter().find(open))
        });
        if let Some(c) = carry {
            claimed.insert(c.started_at.clone());
        }
        let tempo_id = carry.and_then(|c| c.tempo_worklog_id.clone());
        // A block whose length changed a lot on rebuild no longer matches
        // its stale description — drop it so the block is described again.
        let keeps_description = carry.is_none_or(|c| {
            // A description remembers the length it was written for
            // (`described_seconds`) — comparing against that survives a
            // block that grows in small steps every rebuild, and a same-
            // ticket merge that widens the claimed prior row. A NULL
            // (old rows, manual text) falls back to the prior row's own
            // span, as before.
            let prior_minutes = match c.described_seconds {
                Some(secs) => (0, secs.div_euclid(60)),
                None => {
                    let Some((cs, ce)) = parse_pair(&c.started_at, &c.ended_at) else {
                        return true;
                    };
                    (cs.timestamp().div_euclid(60), ce.timestamp().div_euclid(60))
                }
            };
            let new_minutes = (
                b.started_at.timestamp().div_euclid(60),
                b.ended_at.timestamp().div_euclid(60),
            );
            crate::infer_carry::keeps_description(
                prior_minutes,
                new_minutes,
                c.estimated_by.as_deref(),
            )
        });
        let description = carry
            .filter(|_| keeps_description)
            .and_then(|c| c.description.clone());
        let estimated_by = carry
            .filter(|_| keeps_description)
            .and_then(|c| c.estimated_by.clone());
        let described_seconds = carry
            .filter(|_| keeps_description)
            .and_then(|c| c.described_seconds);
        let exported_at = carry.and_then(|c| c.exported_at.clone());
        // Preserve manual ticket override if present; otherwise trust inference.
        let jira_issue = carry
            .and_then(|c| c.jira_issue.clone())
            .or_else(|| b.jira_issue.clone());

        // path-based classifier gives the first signal, but a jira_issue
        // that's actually a cached ticket (R7) is a stronger one — a spec
        // ID that merely looks like a Jira key (`FR-09`) must not flip a
        // personal path to work.
        let path_personal = personal_cfg.classify(b.dominant_project_path().as_deref());
        let has_real_ticket = jira_issue
            .as_deref()
            .is_some_and(|k| jira_ticket_known(&tx, k));
        // Carry an ignore only when the block starts at exactly the same
        // instant (strict map hit, not the overlap fallback) and did not
        // grow past the prior end. A block that grew may now hold real work,
        // so the ignore is dropped rather than silently swallowing it.
        let ignored_at = prior.get(&started_key).and_then(|c| {
            let ign = c.ignored_at.clone()?;
            let (_, prior_end) = parse_pair(&c.started_at, &c.ended_at)?;
            (b.ended_at <= prior_end).then_some(ign)
        });
        let is_personal = ignored_at.is_some() || (path_personal && !has_real_ticket);
        tx.execute(
            "INSERT INTO blocks (
                day, jira_issue, started_at, ended_at,
                duration_seconds, description, estimated_by, flagged,
                tempo_worklog_id, is_personal, exported_at, described_seconds, ignored_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                b.day,
                jira_issue,
                block_iso(b.started_at),
                block_iso(b.ended_at),
                b.duration_seconds,
                description,
                estimated_by,
                if b.flagged { 1 } else { 0 },
                tempo_id,
                if is_personal { 1 } else { 0 },
                exported_at,
                described_seconds,
                ignored_at,
            ],
        )
        .context("inserting block")?;
        let block_id = tx.last_insert_rowid();
        for eid in &b.event_ids {
            tx.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![block_id, eid],
            )
            .context("inserting block_events row")?;
        }
    }
    // Re-key hand-set owner rows onto whichever new block covers them
    // most, before an unmatched old started_at is lost for good.
    crate::infer_carry_shares::carry_owner_tables(&tx, &day_iso, &old_spans, &new_spans)?;
    // FR-06: re-link every owner-moved event of this day into a fresh
    // block now that the deletes+inserts above rebuilt the day's blocks.
    crate::elsewhere::relink_moved_events(&tx, day)?;
    tx.commit().context("committing block persistence")?;

    // One batch for this rebuild (D-07); a refresh failure must not fail
    // the rebuild — the write above already committed.
    crate::change_log::refresh_day_logged(
        conn,
        &day_iso,
        crate::deild_contract::ChangeSource::Rebuild,
    );

    Ok(())
}

/// Format timestamps the way the Python code does (`datetime.isoformat()`
/// with offset). Python emits sub-seconds iff microsecond != 0; we use
/// `SecondsFormat::AutoSi` which behaves the same way (no fractional
/// when zero). This maximises the odds of exact-string match with rows
/// Python wrote; the overlap fallback handles the remaining cases.
/// The `false` argument forces `+00:00` instead of `Z` for the offset.
fn block_iso(dt: DateTime<Utc>) -> String {
    use chrono::SecondsFormat;
    dt.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

/// R7: does `key` exist in `jira_tickets`? A spec ID that merely looks
/// like a Jira key (`FR-09`) has no row here — only a real cached ticket
/// flips a personal-path block to work.
fn jira_ticket_known(conn: &Connection, key: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM jira_tickets WHERE key = ?1",
        params![key],
        |_| Ok(()),
    )
    .is_ok()
}

#[derive(Debug, Clone)]
struct CarryRow {
    started_at: String,
    ended_at: String,
    jira_issue: Option<String>,
    description: Option<String>,
    estimated_by: Option<String>,
    tempo_worklog_id: Option<String>,
    exported_at: Option<String>,
    described_seconds: Option<i64>,
    ignored_at: Option<String>,
}

/// Overlap check on ISO-8601 timestamps. Parses each string to a
/// `DateTime` before comparing so we're robust to cross-format
/// differences — e.g. Python emits `T09:00:00+00:00` (no sub-seconds
/// when microsecond == 0) while Rust's `block_iso` historically emitted
/// `T09:00:00.000000+00:00` (forced microseconds). Lexicographic
/// comparison of those two strings gives the wrong answer because
/// `.` (0x2E) sorts after `+` (0x2B). Parsing them both sidesteps the
/// issue entirely.
fn ranges_overlap(a_start: &str, a_end: &str, b_start: &str, b_end: &str) -> bool {
    let Some((a_s, a_e)) = parse_pair(a_start, a_end) else {
        return false;
    };
    let Some((b_s, b_e)) = parse_pair(b_start, b_end) else {
        return false;
    };
    a_s < b_e && b_s < a_e
}

/// `pub(crate)` so `infer_carry_shares` can measure overlap between an old
/// and a new block span without re-implementing the same cross-format
/// timestamp parsing.
pub(crate) fn parse_pair(
    start: &str,
    end: &str,
) -> Option<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
    // Accept the common variants our codebase writes:
    //   * `+00:00` offset (what block_iso emits)
    //   * `Z` (chrono's default to_rfc3339 on some builds)
    //   * naive (no offset) — treat as UTC
    let s = parse_maybe_utc(start)?;
    let e = parse_maybe_utc(end)?;
    Some((s, e))
}

fn parse_maybe_utc(s: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Fallback: naive ISO → assume UTC.
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(naive.and_utc());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo;
    use chrono::TimeZone;

    #[test]
    fn ranges_overlap_handles_cross_format_timestamps() {
        // The concrete bug: Python emits `09:00:00+00:00` (no
        // sub-seconds when microsecond==0), Rust's old block_iso emitted
        // `09:00:00.000000+00:00`. Lexicographic compare said "Python
        // start > Rust end" because `.` > `+`, so the overlap fallback
        // silently failed and the carry state was lost.
        //
        // Parsed-DateTime compare doesn't care about formatting.
        assert!(ranges_overlap(
            "2026-04-18T09:00:00.000000+00:00", // Rust micro-precision
            "2026-04-18T09:30:00.000000+00:00",
            "2026-04-18T09:00:00+00:00", // Python second-precision
            "2026-04-18T09:30:00+00:00",
        ));

        // And Z suffix vs +00:00 suffix, both should parse fine.
        assert!(ranges_overlap(
            "2026-04-18T09:00:00Z",
            "2026-04-18T09:30:00Z",
            "2026-04-18T09:15:00+00:00",
            "2026-04-18T09:45:00+00:00",
        ));

        // Non-overlapping ranges still return false.
        assert!(!ranges_overlap(
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            "2026-04-18T09:30:00.000000+00:00",
            "2026-04-18T10:00:00.000000+00:00",
        ));

        // Unparseable strings fail closed (no overlap).
        assert!(!ranges_overlap("garbage", "garbage", "also", "also"));
    }

    #[test]
    fn block_iso_matches_python_for_whole_seconds() {
        // Python: `datetime(2026,4,18,9,0,0,tzinfo=UTC).isoformat()`
        //   → "2026-04-18T09:00:00+00:00"
        // Rust block_iso with SecondsFormat::AutoSi must produce the
        // same string (no `.000000` padding).
        let dt = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        assert_eq!(block_iso(dt), "2026-04-18T09:00:00+00:00");
    }

    #[test]
    fn block_iso_emits_sub_seconds_when_non_zero() {
        // Matches Python: isoformat emits sub-seconds iff non-zero.
        use chrono::Timelike;
        let dt = Utc
            .with_ymd_and_hms(2026, 4, 18, 9, 0, 0)
            .unwrap()
            .with_nanosecond(123_456_789)
            .unwrap();
        let s = block_iso(dt);
        assert!(
            s.starts_with("2026-04-18T09:00:00.") && s.ends_with("+00:00"),
            "block_iso should emit sub-seconds for non-zero nanos: {s}"
        );
    }

    fn ib(start: (u32, u32), end: (u32, u32)) -> InferBlock {
        let mut b = new_block(&ev(start.0, start.1, "claude_turn"));
        b.ended_at = at(end.0, end.1);
        b.duration_seconds = (b.ended_at - b.started_at).num_seconds();
        b
    }

    fn ignored_state(conn: &rusqlite::Connection) -> (Option<String>, i64) {
        conn.query_row("SELECT ignored_at, is_personal FROM blocks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap()
    }

    #[test]
    fn persist_blocks_keeps_ignore_when_block_unchanged() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        persist_blocks(&conn, day, &[ib((9, 0), (9, 30))]).unwrap();
        conn.execute(
            "UPDATE blocks SET ignored_at = '2026-04-18T12:00:00Z', is_personal = 1",
            [],
        )
        .unwrap();
        persist_blocks(&conn, day, &[ib((9, 0), (9, 30))]).unwrap();
        assert_eq!(
            ignored_state(&conn),
            (Some("2026-04-18T12:00:00Z".to_string()), 1)
        );
    }

    #[test]
    fn persist_blocks_drops_ignore_when_block_grows() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        persist_blocks(&conn, day, &[ib((9, 0), (9, 30))]).unwrap();
        conn.execute(
            "UPDATE blocks SET ignored_at = '2026-04-18T12:00:00Z', is_personal = 1",
            [],
        )
        .unwrap();
        persist_blocks(&conn, day, &[ib((9, 0), (10, 0))]).unwrap();
        assert_eq!(ignored_state(&conn).0, None);
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 18, h, m, 0).unwrap()
    }

    fn ev(h: u32, m: u32, source: &str) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: source.into(),
            duration_seconds: None,
            jira_issue: None,
            event_id: None,
            project_path: None,
            session_id: None,
            title: None,
            lane_tag: None,
        }
    }

    fn ev_project(h: u32, m: u32, source: &str, project: &str) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: source.into(),
            duration_seconds: None,
            jira_issue: None,
            event_id: None,
            project_path: Some(project.into()),
            session_id: None,
            title: None,
            lane_tag: None,
        }
    }

    fn calendar(h: u32, m: u32, secs: i64) -> InferEvent {
        InferEvent {
            ts: at(h, m),
            source: "gcal".into(),
            duration_seconds: Some(secs),
            jira_issue: None,
            event_id: None,
            project_path: None,
            session_id: None,
            title: None,
            lane_tag: None,
        }
    }

    #[test]
    fn single_event_blocks_are_dropped_when_shorter_than_min() {
        // Point event extends by CREDIT (2m) < MIN_BLOCK (5m) → dropped.
        let blocks = build_blocks(vec![ev(10, 0, "github_commit")]);
        assert!(blocks.is_empty());
    }

    #[test]
    fn close_events_form_one_block() {
        let events: Vec<_> = (0..5)
            .map(|i| ev(10, 5 * i as u32, "github_commit"))
            .collect();
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].event_count, 5);
    }

    #[test]
    fn gap_over_timeout_starts_new_block() {
        let a = ev(9, 0, "github_commit");
        let b = ev(9, 5, "github_commit");
        // 40-min gap (9:05 → 9:45) clearly exceeds TIMEOUT (30m).
        let c = ev(9, 45, "github_commit");
        let d = ev(9, 50, "github_commit");
        let blocks = build_blocks(vec![a, b, c, d]);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn long_claude_turn_does_not_fragment_block() {
        // Regression: user reported 3-hour Claude sessions showing as
        // 15-30 min slivers. Cause: with the old TIMEOUT (20m) and the
        // old hook event set (no PreToolUse), a Claude turn running tools
        // autonomously for 25 min between UserPromptSubmit and Stop
        // looked like two separate point-events 25 min apart, each
        // getting 2 min of credit → two 2-min "blocks" both dropped
        // under MIN_BLOCK (5m). With TIMEOUT=30 (and PreToolUse hooks
        // emitting heartbeats in real Claude sessions), this stays as
        // one continuous block.
        let events = vec![
            ev(9, 0, "claude"),   // UserPromptSubmit
            ev(9, 25, "claude"),  // Stop (25 min autonomous run)
            ev(9, 50, "claude"),  // next UserPromptSubmit (25 min reading)
            ev(10, 15, "claude"), // Stop
            ev(10, 40, "claude"), // UserPromptSubmit
            ev(11, 5, "claude"),  // Stop
            ev(11, 30, "claude"), // UserPromptSubmit
            ev(11, 35, "claude"), // Stop (quick reply)
        ];
        let blocks = build_blocks(events);
        assert_eq!(
            blocks.len(),
            1,
            "long Claude session must stay as one block — got {} blocks of lengths {:?}",
            blocks.len(),
            blocks
                .iter()
                .map(|b| b.duration_seconds)
                .collect::<Vec<_>>()
        );
        let dur_minutes = blocks[0].duration_seconds / 60;
        assert!(
            dur_minutes >= 2 * 60 + 30,
            "block duration {} min should reflect the full 2h35m span",
            dur_minutes
        );
    }

    #[test]
    fn calendar_event_is_authoritative_and_isolated() {
        let code_before = ev(9, 0, "github_commit");
        let meeting = calendar(9, 10, 45 * 60); // 45-min meeting
        let code_after = ev(10, 0, "github_commit");
        let blocks = build_blocks(vec![code_before, meeting, code_after]);
        // Three separate blocks: code block before is <MIN so dropped;
        // meeting is its own block; code after is <MIN so dropped.
        // Result: exactly one block (the meeting).
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].duration_seconds, 45 * 60);
    }

    #[test]
    fn jira_issue_is_kept_when_all_events_agree() {
        let mut a = ev(10, 0, "github_commit");
        a.jira_issue = Some("PROJ-1".into());
        let mut b = ev(10, 5, "github_commit");
        b.jira_issue = Some("PROJ-1".into());
        let blocks = build_blocks(vec![a, b]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].jira_issue.as_deref(), Some("PROJ-1"));
    }

    #[test]
    fn split_creates_two_blocks_on_sustained_project_switch() {
        // Mirrors today's real scenario: 50 min on project A, then 25 min
        // on project B, both within the gap-timeout (no natural cluster
        // boundary). Expected: 2 blocks.
        let mut events = Vec::new();
        for i in 0..11 {
            events.push(ev_project(9, i * 5, "claude", "/work/sjukra"));
        }
        for i in 0..6 {
            // start at 10:00 (5 min after last sjukra event at 09:50)
            events.push(ev_project(10, i * 5, "claude", "/work/pdf"));
        }
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 2, "should split on sustained project switch");
        assert!(blocks[0].duration_seconds >= 30 * 60);
        assert!(blocks[1].duration_seconds >= 20 * 60);
    }

    #[test]
    fn split_absorbs_brief_project_interruption() {
        // 30 min of sjukra with one stray pdf event in the middle.
        // The pdf run is too short (<5 min on each side as a standalone)
        // to justify a split — stays as one block.
        let mut events = Vec::new();
        for i in 0..6 {
            events.push(ev_project(9, i * 5, "claude", "/work/sjukra"));
        }
        events.push(ev_project(9, 17, "claude", "/work/pdf"));
        for i in 0..5 {
            // resume sjukra after the brief pdf detour
            events.push(ev_project(9, 20 + i * 5, "claude", "/work/sjukra"));
        }
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 1, "brief interruption must absorb");
    }

    #[test]
    fn split_preserves_calendar_blocks() {
        // A calendar block is authoritative and should never split even
        // if its (hypothetical) events spanned multiple project_paths.
        let meeting = calendar(10, 0, 45 * 60);
        let blocks = build_blocks(vec![meeting]);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn split_ignores_null_project_path_events() {
        // A github_commit (project_path = NULL) inside a sjukra session
        // must NOT trigger a project split. The NULL event inherits the
        // surrounding project.
        let mut events = Vec::new();
        for i in 0..6 {
            events.push(ev_project(9, i * 5, "claude", "/work/sjukra"));
        }
        // github commit with no project_path, mid-stream
        events.push(ev(9, 17, "github_commit"));
        for i in 0..5 {
            events.push(ev_project(9, 20 + i * 5, "claude", "/work/sjukra"));
        }
        let blocks = build_blocks(events);
        assert_eq!(blocks.len(), 1, "NULL project_path event must not split");
    }

    #[test]
    fn dominant_project_path_picks_most_common() {
        let mut a = ev_project(9, 0, "claude", "/work/sjukra");
        a.event_id = Some(1);
        let mut b = ev_project(9, 5, "claude", "/work/sjukra");
        b.event_id = Some(2);
        let mut c = ev_project(9, 10, "claude", "/work/pdf");
        c.event_id = Some(3);
        let block = new_block(&a);
        let mut block = block;
        extend_block(&mut block, &b);
        extend_block(&mut block, &c);
        assert_eq!(
            block.dominant_project_path().as_deref(),
            Some("/work/sjukra")
        );
    }

    #[test]
    fn dominant_project_path_ignores_null_paths() {
        let a = ev_project(9, 0, "claude", "/work/sjukra");
        let b = ev(9, 5, "github_commit"); // project_path = None
        let block = new_block(&a);
        let mut block = block;
        extend_block(&mut block, &b);
        assert_eq!(
            block.dominant_project_path().as_deref(),
            Some("/work/sjukra")
        );
    }

    /// R3 regression (found via the 2026-09-25 real-data check): a run
    /// legitimately owned by apro-skills can have MORE SessionStart/
    /// SessionEnd lifecycle riders from unrelated concurrent sessions
    /// (a different project's `claude` hook pings, riding in like
    /// folderless events per R3) than real apro-skills events. Those
    /// riders must never outvote the block's real project and flip its
    /// `is_personal` classification.
    #[test]
    fn dominant_project_path_ignores_lifecycle_riders() {
        let mut a = ev_project(10, 36, "claude_work", "/Users/dev/Desktop/Work/apro-skills");
        a.event_id = Some(1);
        let mut block = new_block(&a);
        let mut b = ev_project(10, 37, "claude_turn", "/Users/dev/Desktop/Work/apro-skills");
        b.event_id = Some(2);
        extend_block(&mut block, &b);

        // Five lifecycle riders from an unrelated personal project — more
        // than the two real apro-skills events above.
        for i in 0..5 {
            let mut rider = ev_project(10, 40 + i, "claude", "/Users/dev/Desktop/Projects/worklog");
            rider.title = Some("SessionStart".into());
            rider.event_id = Some(10 + i64::from(i));
            attach_riding_event(&mut block, rider);
        }

        assert_eq!(
            block.dominant_project_path().as_deref(),
            Some("/Users/dev/Desktop/Work/apro-skills"),
            "lifecycle riders must never outvote the block's real project"
        );
    }

    #[test]
    fn jira_issue_cleared_when_events_disagree() {
        let mut a = ev(10, 0, "github_commit");
        a.jira_issue = Some("PROJ-1".into());
        let mut b = ev(10, 5, "github_commit");
        b.jira_issue = Some("PROJ-2".into());
        let blocks = build_blocks(vec![a, b]);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].jira_issue.is_none());
    }

    #[test]
    fn flags_blocks_over_max() {
        let start = Utc.with_ymd_and_hms(2026, 4, 18, 9, 0, 0).unwrap();
        let end = start + Duration::hours(5); // > MAX_BLOCK
        let events = vec![
            InferEvent {
                ts: start,
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
                project_path: None,
                session_id: None,
                title: None,
                lane_tag: None,
            },
            InferEvent {
                ts: end - Duration::minutes(1),
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
                project_path: None,
                session_id: None,
                title: None,
                lane_tag: None,
            },
        ];
        // Gap is > TIMEOUT, so these become two separate blocks.
        // Change: use densely packed events to form a single long block.
        let mut dense = Vec::new();
        let mut t = start;
        while t < end {
            dense.push(InferEvent {
                ts: t,
                source: "github_commit".into(),
                duration_seconds: None,
                jira_issue: None,
                event_id: None,
                project_path: None,
                session_id: None,
                title: None,
                lane_tag: None,
            });
            t += Duration::minutes(10);
        }
        let blocks = build_blocks(dense);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].flagged, "long block must be flagged");
        // Unused test vars — silence compiler.
        let _ = events;
    }

    #[test]
    fn persist_round_trip_creates_rows() {
        let conn = open_memory().unwrap();
        // Insert two raw events for the day.
        let e1 = repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "aaa", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        let e2 = repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "bbb",
                "2026-04-18T10:05:00+00:00",
                "second",
            ),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        assert_eq!(events.len(), 2);
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].duration_seconds, blocks[0].duration_seconds);

        // block_events must have both rows.
        let junction: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM block_events WHERE block_id = ?1",
                params![stored[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(junction, 2);
        let _ = (e1, e2);
    }

    #[test]
    fn reinference_preserves_tempo_id_and_description() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x2", "2026-04-18T10:05:00+00:00", "second"),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        // Simulate the user hand-editing.
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '98765', description = 'custom', \
             jira_issue = 'PROJ-7', estimated_by = 'manual' WHERE day = ?1",
            params!["2026-04-18"],
        )
        .unwrap();

        // Add another event and re-infer.
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "x3", "2026-04-18T10:08:00+00:00", "third"),
        )
        .unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].tempo_worklog_id.as_deref(), Some("98765"));
        assert_eq!(stored[0].description.as_deref(), Some("custom"));
        assert_eq!(stored[0].jira_issue.as_deref(), Some("PROJ-7"));
        assert_eq!(stored[0].estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn grown_block_is_described_again() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "y1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "y2", "2026-04-18T10:28:00+00:00", "second"),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();
        assert_eq!(blocks[0].duration_seconds, 30 * 60);

        conn.execute(
            "UPDATE blocks SET description = 'old', estimated_by = 'claude_p', \
             tempo_worklog_id = '555' WHERE day = ?1",
            params!["2026-04-18"],
        )
        .unwrap();

        // Grow the same block from 30 minutes to 3h02m — every added event
        // stays within the 30-minute clustering timeout so it's one block,
        // not a new one.
        for (id, hm) in [
            ("y3", "10:50"),
            ("y4", "11:12"),
            ("y5", "11:34"),
            ("y6", "11:56"),
            ("y7", "12:18"),
            ("y8", "12:40"),
            ("y9", "13:00"),
        ] {
            repo::upsert_event(
                &conn,
                &Event::minimal("github_commit", id, format!("2026-04-18T{hm}:00+00:00"), id),
            )
            .unwrap();
        }
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].duration_seconds, 182 * 60);
        assert_eq!(stored[0].description, None);
        assert_eq!(stored[0].estimated_by, None);
        assert_eq!(stored[0].tempo_worklog_id.as_deref(), Some("555"));
    }

    /// B11: a description remembers the length it was written for
    /// (`described_seconds`), not the prior row's current span — so it
    /// survives a block that grows in small steps every 15-minute
    /// rebuild, one step below both thresholds at a time.
    #[test]
    fn gradual_growth_is_described_again() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "z1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "z2", "2026-04-18T10:28:00+00:00", "second"),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();
        assert_eq!(blocks[0].duration_seconds, 30 * 60);

        conn.execute(
            "UPDATE blocks SET description = 'short', estimated_by = 'claude_p', \
             described_seconds = 1800 WHERE day = ?1",
            params!["2026-04-18"],
        )
        .unwrap();

        // Grow to 45 minutes (10:00-10:45): diff from the described 30
        // min is 15 (< 30) and 45*2 <= 30*3 — kept.
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "z3", "2026-04-18T10:43:00+00:00", "third"),
        )
        .unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].duration_seconds, 45 * 60);
        assert_eq!(
            stored[0].description.as_deref(),
            Some("short"),
            "45 min is within the described-30 thresholds"
        );
        let described_seconds: Option<i64> = conn
            .query_row(
                "SELECT described_seconds FROM blocks WHERE day = ?1",
                params!["2026-04-18"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            described_seconds,
            Some(1800),
            "a kept description carries its described length unchanged"
        );

        // Grow to 60 minutes (10:00-11:00): diff from the described 30
        // min is now exactly 30 — dropped.
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "z4", "2026-04-18T10:58:00+00:00", "fourth"),
        )
        .unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].duration_seconds, 60 * 60);
        assert_eq!(stored[0].description, None);
        let described_seconds: Option<i64> = conn
            .query_row(
                "SELECT described_seconds FROM blocks WHERE day = ?1",
                params!["2026-04-18"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(described_seconds, None);
    }

    /// B11: the estimator merges adjacent same-ticket blocks into one wide
    /// row (`estimate::merge_block_into`), widening only the destination
    /// row's span — its `described_seconds` stays whatever it was written
    /// for. On the next rebuild the first piece splits back out at its
    /// original span and must keep its description.
    #[test]
    fn merged_piece_keeps_described_text() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "m1", "2026-04-18T10:00:00+00:00", "first"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "m2", "2026-04-18T10:28:00+00:00", "second"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "m3", "2026-04-18T11:30:00+00:00", "third"),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal("github_commit", "m4", "2026-04-18T11:58:00+00:00", "fourth"),
        )
        .unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();
        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 2, "60-minute gap keeps the two pieces apart");

        let first_id = stored[0].id;
        let second_id = stored[1].id;

        // The first piece got a description written for its own 30-minute
        // span, then estimate::merge_block_into widened it to cover both
        // pieces (dst keeps its own started_at; ended_at/duration_seconds
        // become the wider span) and dropped the second row.
        conn.execute(
            "UPDATE blocks SET description = 'desc', estimated_by = 'claude_p', \
             jira_issue = 'TICK-1', described_seconds = 1800 WHERE id = ?1",
            params![first_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE blocks SET ended_at = '2026-04-18T12:00:00+00:00', \
             duration_seconds = ?1 WHERE id = ?2",
            params![120 * 60, first_id],
        )
        .unwrap();
        conn.execute("DELETE FROM blocks WHERE id = ?1", params![second_id])
            .unwrap();

        // Rebuild: the underlying events split back into the same two
        // pieces they always were.
        let events = load_day_events(&conn, day).unwrap();
        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 2);
        let first_piece = stored
            .iter()
            .find(|b| b.duration_seconds == 30 * 60 && b.started_at.starts_with("2026-04-18T10"))
            .expect("first piece present");
        assert_eq!(
            first_piece.description.as_deref(),
            Some("desc"),
            "the 10:00-10:30 piece keeps its description"
        );
    }

    /// R7: a spec ID that merely looks like a Jira key (`FR-09`) must not
    /// flip a personal-path block to work — only a key that actually
    /// exists in `jira_tickets` does.
    #[test]
    fn spec_id_on_personal_path_does_not_flip_to_work() {
        let conn = open_memory().unwrap();
        let mut a = Event::minimal(
            "claude_turn",
            "e1",
            "2026-04-18T10:00:00+00:00",
            "spec work",
        );
        a.project_path = Some("/Users/dev/Desktop/Projects/worklog".into());
        a.jira_issue = Some("FR-09".into());
        repo::upsert_event(&conn, &a).unwrap();
        let mut b = Event::minimal(
            "claude_turn",
            "e2",
            "2026-04-18T10:05:00+00:00",
            "spec work",
        );
        b.project_path = Some("/Users/dev/Desktop/Projects/worklog".into());
        b.jira_issue = Some("FR-09".into());
        repo::upsert_event(&conn, &b).unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert!(
            stored[0].is_personal,
            "FR-09 is not a real cached Jira ticket — the personal path must stick"
        );
    }

    /// R7 inverse: a jira_issue that DOES exist in `jira_tickets` flips a
    /// personal-path block to work, same as before.
    #[test]
    fn cached_jira_ticket_on_personal_path_flips_to_work() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO jira_tickets (key, summary) VALUES ('PROJ-1', 'x')",
            [],
        )
        .unwrap();
        let mut a = Event::minimal(
            "claude_turn",
            "e1",
            "2026-04-18T10:00:00+00:00",
            "real work",
        );
        a.project_path = Some("/Users/dev/Desktop/Projects/worklog".into());
        a.jira_issue = Some("PROJ-1".into());
        repo::upsert_event(&conn, &a).unwrap();
        let mut b = Event::minimal(
            "claude_turn",
            "e2",
            "2026-04-18T10:05:00+00:00",
            "real work",
        );
        b.project_path = Some("/Users/dev/Desktop/Projects/worklog".into());
        b.jira_issue = Some("PROJ-1".into());
        repo::upsert_event(&conn, &b).unwrap();

        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert!(
            !stored[0].is_personal,
            "a real cached ticket must flip a personal-path block to work"
        );
    }

    #[test]
    fn load_day_events_window_respects_worklog_tz() {
        // The round-1 phase-5 test only exercised new_block's local_date
        // bucketing. The actual query path (utc_window_for_local_day
        // driving the SQL range) was untested with a non-UTC TZ. This
        // test covers it: an event at 04:30 UTC lands on local Apr 18
        // in UTC-5, so asking for local Apr 18 must return that event.
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let conn = open_memory().unwrap();
        // 04:30 UTC = 23:30 local on Apr 18 in UTC-5.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "late-commit",
                "2026-04-19T04:30:00+00:00",
                "late night work",
            ),
        )
        .unwrap();
        // 14:00 UTC on Apr 18 = 09:00 local — also on Apr 18.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "morning-commit",
                "2026-04-18T14:00:00+00:00",
                "morning work",
            ),
        )
        .unwrap();

        let events = load_day_events(&conn, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap())
            .expect("load_day_events");
        assert_eq!(events.len(), 2, "both events must land on local Apr 18");

        // And asking for local Apr 19 returns NOTHING (the 04:30Z event
        // is local Apr 18, not Apr 19).
        let none = load_day_events(&conn, NaiveDate::from_ymd_opt(2026, 4, 19).unwrap())
            .expect("load_day_events");
        assert_eq!(
            none.len(),
            0,
            "Apr 19 local should be empty — the 04:30Z event is Apr 18 local"
        );

        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn load_day_events_excludes_dismissed_firefox_and_slack_events() {
        // Dismissed noise (a random DM, a news site) must never become block
        // time, same as an unlabelled event — but here the event HAS a
        // label_origin (it's just "dismissed"), so the existing
        // `label_origin IS NULL` check alone would let it through.
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let firefox_id = repo::upsert_event(
            &conn,
            &Event::minimal(
                crate::routing_contract::SOURCE_FIREFOX,
                "f1",
                "2026-04-18T09:00:00+00:00",
                "news site",
            ),
        )
        .unwrap();
        let slack_id = repo::upsert_event(
            &conn,
            &Event::minimal(
                crate::routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-18T09:05:00+00:00",
                "#random",
            ),
        )
        .unwrap();
        crate::routing_dismiss::dismiss_event(&conn, firefox_id, None).unwrap();
        crate::routing_dismiss::dismiss_event(&conn, slack_id, None).unwrap();

        let events = load_day_events(&conn, day).unwrap();
        assert!(
            events.is_empty(),
            "dismissed firefox/slack events must never reach inference"
        );
    }

    #[test]
    fn load_day_events_excludes_noise_firefox_and_slack_events() {
        // Noise (the absorb step's last resort, spec 004) must be treated
        // exactly like dismissed: never block time, even though its
        // label_origin is non-NULL.
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let firefox_id = repo::upsert_event(
            &conn,
            &Event::minimal(
                crate::routing_contract::SOURCE_FIREFOX,
                "f1",
                "2026-04-18T09:00:00+00:00",
                "news site",
            ),
        )
        .unwrap();
        conn.execute(
            "UPDATE events SET label_origin = 'noise' WHERE id = ?1",
            [firefox_id],
        )
        .unwrap();

        let events = load_day_events(&conn, day).unwrap();
        assert!(
            events.is_empty(),
            "noise firefox/slack events must never reach inference"
        );
    }

    #[test]
    fn load_day_events_falls_back_to_db_title_on_malformed_raw_json() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let mut e = Event::minimal("claude_turn", "p1", "2026-04-18T09:00:00+00:00", "prompt");
        e.raw_json = Some("not json".into());
        repo::upsert_event(&conn, &e).unwrap();

        let events = load_day_events(&conn, day).unwrap();
        assert_eq!(events[0].title.as_deref(), Some("prompt"));
    }

    #[test]
    fn load_day_events_keeps_shell_title_even_with_raw_json() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let mut e = Event::minimal("shell", "s1", "2026-04-18T09:00:00+00:00", "git status");
        e.raw_json = Some(
            serde_json::to_string(&crate::clues_contract::RawRecord::Shell {
                command: "git status".into(),
                cwd: None,
            })
            .unwrap(),
        );
        repo::upsert_event(&conn, &e).unwrap();

        let events = load_day_events(&conn, day).unwrap();
        assert_eq!(events[0].title.as_deref(), Some("git status"));
    }

    #[test]
    fn elsewhere_event_joins_no_block() {
        // events.elsewhere = 1 marks an org commit whose sha is in no
        // local clone (FR-04) — it must never reach inference and must
        // never join a block (D-08, FR-09, B3).
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "elsewhere1",
                "2026-04-18T09:00:00+00:00",
                "org commit, no local clone",
            ),
        )
        .unwrap();
        conn.execute("UPDATE events SET elsewhere = 1 WHERE id = ?1", [id])
            .unwrap();

        let events = load_day_events(&conn, day).unwrap();
        assert!(
            events.is_empty(),
            "an elsewhere-flagged event must never reach inference"
        );

        let blocks = build_blocks(events);
        persist_blocks(&conn, day, &blocks).unwrap();
        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert!(stored.is_empty(), "elsewhere event must join no block");
    }

    #[test]
    fn block_day_respects_worklog_tz() {
        // Regression for H4: without WORKLOG_TZ, a 23:30 local event in
        // UTC-5 (=04:30Z the next day) would land on the WRONG day's
        // review page. With WORKLOG_TZ=-05:00, it lands on the local day.
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let ts = chrono::Utc.with_ymd_and_hms(2026, 4, 19, 4, 30, 0).unwrap();
        let event = InferEvent {
            event_id: None,
            source: "manual".into(),
            ts,
            duration_seconds: Some(600),
            jira_issue: None,
            project_path: None,
            session_id: None,
            title: None,
            lane_tag: None,
        };
        let block = new_block(&event);
        assert_eq!(
            block.day, "2026-04-18",
            "04:30 UTC in UTC-5 is 23:30 on Apr 18 local"
        );
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn reinference_preserves_state_when_start_shifts_backward() {
        // Regression test for the infer carry bug: a backfilled earlier
        // event shifts a block's started_at. Strict-key equality loses
        // tempo_worklog_id (and other carry state) — the CLAUDE.md
        // canary invariant "tempo_worklog_id MUST NEVER be cleared" is
        // violated. The carry logic must fall back to overlap matching.
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "late1",
                "2026-04-18T10:00:00+00:00",
                "first",
            ),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "late2",
                "2026-04-18T10:05:00+00:00",
                "second",
            ),
        )
        .unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        // User reviews and syncs this block.
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = '42424', jira_issue = 'PROJ-3', \
             description = 'reviewed', estimated_by = 'manual' WHERE day = '2026-04-18'",
            [],
        )
        .unwrap();

        // A GitHub backfill now adds an EARLIER event, shifting the
        // block's started_at by several minutes. Strict-key match misses.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "early1",
                "2026-04-18T09:55:00+00:00",
                "earlier",
            ),
        )
        .unwrap();
        let blocks = build_blocks(load_day_events(&conn, day).unwrap());
        persist_blocks(&conn, day, &blocks).unwrap();

        let stored = repo::list_blocks_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].tempo_worklog_id.as_deref(),
            Some("42424"),
            "tempo_worklog_id MUST be preserved across started_at shift"
        );
        assert_eq!(stored[0].jira_issue.as_deref(), Some("PROJ-3"));
        assert_eq!(stored[0].description.as_deref(), Some("reviewed"));
        assert_eq!(stored[0].estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn shell_and_reflog_events_form_a_project_block() {
        let project = "/Users/dev/Desktop/Work/vitinn-infra";
        let mut events: Vec<InferEvent> = (0..10)
            .map(|i| ev_project(9, i * 2, "shell", project))
            .collect();
        events.push(ev_project(9, 19, "git_reflog", project));
        let blocks = build_blocks(events);
        assert_eq!(
            blocks.len(),
            1,
            "shell + reflog events under one project must form a single block"
        );
        let dur_min = blocks[0].duration_seconds / 60;
        assert!(
            (15..=25).contains(&dur_min),
            "block duration should be 15-25 min, got {dur_min}"
        );
        assert_eq!(blocks[0].dominant_project_path().as_deref(), Some(project));
    }

    #[test]
    fn reflog_checkout_switches_project() {
        // R2: git_reflog is background evidence now, not the owner acting —
        // it no longer grabs focus the instant it fires. The handover to
        // repo-b waits for repo-b's own first `shell` command (10:16)
        // instead of the reflog event itself (10:14); repo-a's focus window
        // (R1: 5 min for non-prompt human sources) still covers 10:10-10:15.
        let repo_a = "/Users/dev/Desktop/Work/repo-a";
        // Both client repos: a switch between work repos must split. (A switch
        // from work to a personal ~/Desktop/Projects repo deliberately does
        // not — work outranks personal, see infer_lanes.)
        let repo_b = "/Users/dev/Desktop/Work/repo-b";
        let mut events: Vec<InferEvent> = vec![
            ev_project(9, 40, "shell", repo_a),
            ev_project(9, 45, "shell", repo_a),
            ev_project(9, 50, "shell", repo_a),
            ev_project(9, 55, "shell", repo_a),
            ev_project(10, 0, "shell", repo_a),
            ev_project(10, 5, "shell", repo_a),
            ev_project(10, 10, "shell", repo_a),
        ];
        events.push(ev_project(10, 14, "git_reflog", repo_b));
        events.extend([
            ev_project(10, 16, "shell", repo_b),
            ev_project(10, 18, "shell", repo_b),
            ev_project(10, 20, "shell", repo_b),
            ev_project(10, 22, "shell", repo_b),
        ]);

        let blocks = build_blocks(events);
        assert_eq!(
            blocks.len(),
            2,
            "reflog checkout into a different repo must split the block"
        );
        assert!(
            blocks[0].ended_at <= at(10, 16),
            "repo A's block should end by 10:16, ended at {:?}",
            blocks[0].ended_at
        );
        assert_eq!(blocks[0].dominant_project_path().as_deref(), Some(repo_a));
        assert_eq!(blocks[1].dominant_project_path().as_deref(), Some(repo_b));
        assert!(blocks[1].started_at >= at(10, 14));
    }
}
