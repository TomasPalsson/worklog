//! Billing export — turn a day's blocks into per-(repo, task) line
//! items for manual copy-paste into an external invoicing system.
//!
//! Slice 1 (walking skeleton): `worklog export --day D` groups a
//! day's **Work** blocks by `(dominant repo, task)` and prints them as
//! text lines. Personal blocks, CSV/JSON rendering, and the
//! `exported_at` "billed" marker are later slices — this module only
//! grows new `Format` arms and a `Kind::Personal` inclusion, it never
//! changes the shape of an existing row.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::Connection;

use crate::collectors::tempo::{round_to_half_hour, HALF_HOUR_SECONDS};
use crate::models::Block;
use crate::repo;

/// Whether a billing row is billable Work or non-billable Personal
/// time. Mirrors `Block::is_personal` (`false` → `Work`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Kind {
    Work,
    Personal,
}

impl Kind {
    /// The label used everywhere a `Kind` is rendered: `"Work"` /
    /// `"Personal"`.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Work => "Work",
            Kind::Personal => "Personal",
        }
    }
}

/// One billable line item for a day — a group of blocks sharing the
/// same `(dominant repo, task)`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BillingRow {
    /// The block group's dominant repo, or `"—"` when none of the
    /// group's blocks carry a repo.
    pub repo: String,
    /// The shared description, or distinct descriptions joined with
    /// `"; "`, or the task string when no block in the group has a
    /// description.
    pub description: String,
    pub kind: Kind,
    /// Overlap-safe union of the group's block intervals, in seconds
    /// (unrounded).
    pub seconds: i64,
    /// `round_to_half_hour(seconds) / 3600.0` — the billable hours.
    pub hours: f64,
}

/// Output format for [`render`]. Only `Text` ships in slice 1; `Csv`
/// and `Json` arrive as new variants (and a matching `render` arm) in
/// a later slice — adding one must be a localized, additive change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
}

/// Last path segment of a filesystem path — the repo fallback when an
/// event carries `project_path` but no explicit `repo`.
fn basename(path: &str) -> Option<String> {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Most-frequent `COALESCE(events.repo, basename(events.project_path))`
/// across a block's events. `None` when the block has no events
/// carrying either. Mirrors `personal::dominant_project_path_for_block`'s
/// query shape.
pub fn dominant_repo_for_block(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.repo, e.project_path
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1",
    )?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map([block_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut counts: HashMap<String, u32> = HashMap::new();
    for (repo_name, project_path) in rows {
        let key = repo_name.or_else(|| project_path.as_deref().and_then(basename));
        if let Some(key) = key {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    Ok(counts.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k))
}

/// A block's wall-clock interval as epoch seconds: `[start, start +
/// duration)`. Mirrors `worklog_cli::cli::block_interval` exactly —
/// duration (not `ended_at`) is the canonical "logged time" because
/// the estimator writes `duration_seconds` independently of `ended_at`.
fn block_interval(block: &Block) -> (i64, i64) {
    let start = chrono::DateTime::parse_from_rfc3339(&block.started_at)
        .map(|d| d.timestamp())
        .unwrap_or(0);
    (start, start + block.duration_seconds.max(0))
}

/// Total length of the union of `[start, end)` intervals — stretches
/// covered by more than one interval count once. Mirrors
/// `worklog_cli::cli::union_seconds` exactly so a block that overlaps
/// another (e.g. a meeting during coding) is never double-billed.
fn union_seconds(mut intervals: Vec<(i64, i64)>) -> i64 {
    intervals.sort_by_key(|&(s, _)| s);
    let mut total = 0;
    let mut cur: Option<(i64, i64)> = None;
    for (s, e) in intervals {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

/// The grouping key within a repo: `jira_issue` if present and
/// non-empty, else the trimmed block description, else `block-{id}`.
fn task_for_block(block: &Block) -> String {
    if let Some(issue) = block.jira_issue.as_deref().filter(|s| !s.is_empty()) {
        return issue.to_string();
    }
    if let Some(desc) = block
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return desc.to_string();
    }
    format!("block-{}", block.id)
}

/// Accumulator for one `(repo, task)` group while folding a day's
/// blocks.
struct GroupAcc {
    repo: String,
    kind: Kind,
    intervals: Vec<(i64, i64)>,
    /// Distinct non-empty descriptions, in first-seen order.
    descriptions: Vec<String>,
}

/// Fold a group's accumulator into its final [`BillingRow`].
///
/// `seconds` is the union of the group's block intervals (never a
/// naive sum); `hours` is that union rounded to the nearest half hour.
/// `description` is the single shared description when every block
/// agrees, the distinct descriptions joined with `"; "` when they
/// don't, or the task string when no block in the group has one.
fn finish_group(task: &str, acc: GroupAcc) -> BillingRow {
    let seconds = union_seconds(acc.intervals);
    let hours = round_to_half_hour(seconds) as f64 / 3600.0;
    let description = if acc.descriptions.is_empty() {
        task.to_string()
    } else {
        acc.descriptions.join("; ")
    };
    BillingRow {
        repo: acc.repo,
        description,
        kind: acc.kind,
        seconds,
        hours,
    }
}

/// Compute a day's billing rows.
///
/// Slice 1 keeps only Work blocks (`!is_personal`) — Personal blocks
/// join the export in a later slice. Blocks are grouped by
/// `(dominant repo, task)`; each group's `seconds` is the union of its
/// blocks' `[started_at, started_at + duration_seconds)` intervals
/// (never a naive sum), and `hours` is that union rounded to the
/// nearest half hour. Rows are sorted by `repo`, then by descending
/// `seconds`, for deterministic output.
pub fn rows_for_day(conn: &Connection, day: &str) -> Result<Vec<BillingRow>> {
    const NO_REPO: &str = "—";

    let blocks = repo::list_blocks_for_day(conn, day)?;

    let mut groups: HashMap<(String, String), GroupAcc> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();

    for block in blocks.iter().filter(|b| !b.is_personal) {
        let repo_name =
            dominant_repo_for_block(conn, block.id)?.unwrap_or_else(|| NO_REPO.to_string());
        let task = task_for_block(block);
        let key = (repo_name.clone(), task.clone());

        if !groups.contains_key(&key) {
            order.push(key.clone());
            groups.insert(
                key.clone(),
                GroupAcc {
                    repo: repo_name,
                    kind: Kind::Work,
                    intervals: Vec::new(),
                    descriptions: Vec::new(),
                },
            );
        }
        let acc = groups.get_mut(&key).expect("group just inserted");
        acc.intervals.push(block_interval(block));
        if let Some(desc) = block
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !acc.descriptions.iter().any(|d| d == desc) {
                acc.descriptions.push(desc.to_string());
            }
        }
    }

    let mut rows: Vec<BillingRow> = order
        .into_iter()
        .map(|key| {
            let acc = groups.remove(&key).expect("group present for its own key");
            finish_group(&key.1, acc)
        })
        .collect();

    rows.sort_by(|a, b| a.repo.cmp(&b.repo).then(b.seconds.cmp(&a.seconds)));
    Ok(rows)
}

/// Render rounded seconds as a comma-decimal hour string using integer
/// half-hour math — guarantees `5,5` / `4` with no float noise.
fn format_hours(seconds: i64) -> String {
    let halves = round_to_half_hour(seconds) / HALF_HOUR_SECONDS;
    let whole = halves / 2;
    if halves % 2 == 1 {
        format!("{whole},5")
    } else {
        format!("{whole}")
    }
}

fn render_text_line(row: &BillingRow) -> String {
    format!(
        "repo: {}  description: {}  time: {} hrs  type: {}",
        row.repo,
        row.description,
        format_hours(row.seconds),
        row.kind.label()
    )
}

fn render_text(rows: &[BillingRow]) -> String {
    rows.iter()
        .map(render_text_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render `rows` in the given `format`. Slice 1 supports only
/// [`Format::Text`]: one line per row —
/// `repo: {repo}  description: {description}  time: {H} hrs  type: {Work|Personal}`.
pub fn render(rows: &[BillingRow], format: Format) -> String {
    match format {
        Format::Text => render_text(rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo as repository;
    use rusqlite::params;

    /// Insert a block row directly (mirrors the `estimate.rs` /
    /// `daemon.rs` test-fixture style) and return its id.
    #[allow(clippy::too_many_arguments)]
    fn seed_block(
        conn: &Connection,
        day: &str,
        started_at: &str,
        duration_seconds: i64,
        jira_issue: Option<&str>,
        description: Option<&str>,
        is_personal: bool,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks
                (day, jira_issue, started_at, ended_at, duration_seconds, description, is_personal)
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
            params![
                day,
                jira_issue,
                started_at,
                duration_seconds,
                description,
                is_personal as i64
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Insert an event carrying `repo`/`project_path` and link it to
    /// `block_id` via `block_events`.
    fn seed_event(
        conn: &Connection,
        block_id: i64,
        source_id: &str,
        repo: Option<&str>,
        project_path: Option<&str>,
    ) {
        let mut ev = Event::minimal("github_commit", source_id, "2026-04-18T09:00:00Z", "commit");
        ev.repo = repo.map(str::to_string);
        ev.project_path = project_path.map(str::to_string);
        let eid = repository::upsert_event(conn, &ev).unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, eid],
        )
        .unwrap();
    }

    /// Like `seed_event`, but lets the test control the event's
    /// `title` (used by the description-fallback tests, B9).
    /// `seed_event` hardcodes title to `"commit"` since most tests
    /// don't care.
    fn seed_event_titled(
        conn: &Connection,
        block_id: i64,
        source_id: &str,
        repo: Option<&str>,
        project_path: Option<&str>,
        title: &str,
    ) {
        let mut ev = Event::minimal("github_commit", source_id, "2026-04-18T09:00:00Z", title);
        ev.repo = repo.map(str::to_string);
        ev.project_path = project_path.map(str::to_string);
        let eid = repository::upsert_event(conn, &ev).unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, eid],
        )
        .unwrap();
    }

    // ─────────────────────── dominant_repo_for_block ───────────────────────

    #[test]
    fn dominant_repo_picks_most_frequent_repo() {
        // events repo A×2, B×1 → "A".
        let conn = open_memory().unwrap();
        let block_id = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            None,
            false,
        );
        seed_event(&conn, block_id, "e1", Some("A"), None);
        seed_event(&conn, block_id, "e2", Some("A"), None);
        seed_event(&conn, block_id, "e3", Some("B"), None);

        let got = dominant_repo_for_block(&conn, block_id).unwrap();
        assert_eq!(got.as_deref(), Some("A"));
    }

    #[test]
    fn dominant_repo_is_none_without_any_repo_signal() {
        let conn = open_memory().unwrap();
        let block_id = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            None,
            false,
        );
        seed_event(&conn, block_id, "e1", None, None);

        let got = dominant_repo_for_block(&conn, block_id).unwrap();
        assert_eq!(got, None);
    }

    // ─────────────────────────── rows_for_day ───────────────────────────

    #[test]
    fn b1_two_work_blocks_same_repo_different_tickets_stay_separate_rows() {
        // 2 work blocks, repo genai-infra, different tickets: 4h + 5h30m.
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            14_400, // 4h
            Some("GENAI-1"),
            Some("did the thing"),
            false,
        );
        seed_event(&conn, b1, "e1", Some("genai-infra"), None);

        let b2 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T14:00:00+00:00",
            19_800, // 5h30m
            Some("GENAI-2"),
            Some("did another thing"),
            false,
        );
        seed_event(&conn, b2, "e2", Some("genai-infra"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.repo == "genai-infra"));
        assert!(rows.iter().all(|r| r.kind == Kind::Work));

        let hours: Vec<f64> = rows.iter().map(|r| r.hours).collect();
        assert!(hours.contains(&4.0), "expected a 4.0h row, got {hours:?}");
        assert!(hours.contains(&5.5), "expected a 5.5h row, got {hours:?}");
    }

    #[test]
    fn b2_same_repo_same_ticket_blocks_collapse_to_one_row() {
        // 2 work blocks, same repo, same ticket, 1h + 1h (non-overlapping) → 2h.
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            Some("GENAI-9"),
            Some("same task"),
            false,
        );
        seed_event(&conn, b1, "e1", Some("genai-infra"), None);

        let b2 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T11:00:00+00:00",
            3600,
            Some("GENAI-9"),
            Some("same task"),
            false,
        );
        seed_event(&conn, b2, "e2", Some("genai-infra"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].hours, 2.0);
        assert_eq!(rows[0].seconds, 7200);
    }

    #[test]
    fn b3_overlapping_same_ticket_intervals_union_not_sum() {
        // Same ticket, overlapping intervals: 1h@10:00 + 1h@10:30 → union
        // 5400s = 1.5h, NOT the naive 2h sum.
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T10:00:00+00:00",
            3600,
            Some("GENAI-5"),
            Some("overlap task"),
            false,
        );
        seed_event(&conn, b1, "e1", Some("genai-infra"), None);

        let b2 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T10:30:00+00:00",
            3600,
            Some("GENAI-5"),
            Some("overlap task"),
            false,
        );
        seed_event(&conn, b2, "e2", Some("genai-infra"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seconds, 5400);
        assert_eq!(rows[0].hours, 1.5);
    }

    #[test]
    fn b4_block_with_repo_and_no_jira_uses_description_as_task() {
        // A block with events on repo "foo" and no jira_issue: repo "foo",
        // task/description derived from the block description.
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            1800,
            None,
            Some("random work on foo"),
            false,
        );
        seed_event(&conn, b1, "e1", Some("foo"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].repo, "foo");
        assert_eq!(rows[0].description, "random work on foo");
    }

    // Slice 1 excluded Personal blocks entirely (this test used to
    // assert `rows.is_empty()`); slice 2 explicitly reverses that —
    // see the "Behavioral changes" note in this module's doc-comment.
    // Personal blocks now appear in `rows_for_day`'s output, tagged
    // `Kind::Personal` (B7).
    #[test]
    fn rows_for_day_includes_personal_blocks_as_of_slice_two() {
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            Some("personal errand"),
            true,
        );
        seed_event(&conn, b1, "e1", Some("some-app"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1, "slice 2 includes personal blocks");
        assert_eq!(rows[0].kind, Kind::Personal);
        assert_eq!(rows[0].description, "personal errand");
    }

    /// B7: a ~2h personal block on repo `some-app` with a description
    /// yields one row tagged `Kind::Personal`, with that repo and
    /// hours, and the rendered text line contains `type: Personal`.
    #[test]
    fn b7_personal_block_yields_personal_row_with_repo_and_hours() {
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            7200, // 2h
            None,
            Some("dentist appointment"),
            true,
        );
        seed_event(&conn, b1, "e1", Some("some-app"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, Kind::Personal);
        assert_eq!(rows[0].repo, "some-app");
        assert_eq!(rows[0].hours, 2.0);

        let text = render(&rows, Format::Text);
        assert!(
            text.contains("type: Personal"),
            "expected a Personal type line, got: {text}"
        );
    }

    /// B8: a work block explicitly yields `Kind::Work`.
    #[test]
    fn b8_work_block_yields_work_row() {
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            Some("shipped a fix"),
            false,
        );
        seed_event(&conn, b1, "e1", Some("some-app"), None);

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, Kind::Work);
    }

    /// B9: a block with no description, whose only event is titled
    /// "Standup", falls back to that event title as its description —
    /// non-empty, deterministic.
    #[test]
    fn b9_missing_description_falls_back_to_dominant_event_title() {
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            None,
            true,
        );
        seed_event_titled(&conn, b1, "e1", Some("some-app"), None, "Standup");

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].description.is_empty());
        assert_eq!(rows[0].description, "Standup");
    }

    /// B9 (fallback branch 2): no description and no usable event
    /// title (blank title), but the block's event does carry a repo →
    /// falls back to `"Work in {repo}"`.
    #[test]
    fn b9_missing_description_and_blank_title_falls_back_to_work_in_repo() {
        let conn = open_memory().unwrap();
        let b1 = seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            None,
            true,
        );
        seed_event_titled(&conn, b1, "e1", Some("some-app"), None, "");

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].description, "Work in some-app");
    }

    /// B9 (fallback branch 3): no description, no events at all (so
    /// no repo either) → falls back to `"Untitled work"`.
    #[test]
    fn b9_missing_description_and_no_repo_falls_back_to_untitled_work() {
        let conn = open_memory().unwrap();
        seed_block(
            &conn,
            "2026-04-18",
            "2026-04-18T09:00:00+00:00",
            3600,
            None,
            None,
            true,
        );

        let rows = rows_for_day(&conn, "2026-04-18").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].description, "Untitled work");
    }

    // ───────────────────────────── render ─────────────────────────────

    #[test]
    fn render_text_matches_b1_exact_line_format() {
        let rows = vec![
            BillingRow {
                repo: "genai-infra".to_string(),
                description: "did another thing".to_string(),
                kind: Kind::Work,
                seconds: 19_800,
                hours: 5.5,
            },
            BillingRow {
                repo: "genai-infra".to_string(),
                description: "did the thing".to_string(),
                kind: Kind::Work,
                seconds: 14_400,
                hours: 4.0,
            },
        ];

        let text = render(&rows, Format::Text);
        let expected =
            "repo: genai-infra  description: did another thing  time: 5,5 hrs  type: Work\n\
             repo: genai-infra  description: did the thing  time: 4 hrs  type: Work";
        assert_eq!(text, expected);
    }
}
