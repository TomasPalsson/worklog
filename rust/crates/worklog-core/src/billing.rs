//! Billing export — turn a day's blocks into per-(repo, task) line
//! items for manual copy-paste into an external invoicing system.
//!
//! Slice 1 (walking skeleton): `worklog export --day D` groups a
//! day's **Work** blocks by `(dominant repo, task)` and prints them as
//! text lines. Personal blocks, CSV/JSON rendering, and the
//! `exported_at` "billed" marker are later slices — this module only
//! grows new `Format` arms and a `Kind::Personal` inclusion, it never
//! changes the shape of an existing row.

use anyhow::Result;
use rusqlite::Connection;

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
        panic!("not implemented")
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

/// Most-frequent `COALESCE(events.repo, basename(events.project_path))`
/// across a block's events. `None` when the block has no events
/// carrying either. Mirrors `personal::dominant_project_path_for_block`'s
/// query shape.
pub fn dominant_repo_for_block(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let _ = (conn, block_id);
    panic!("not implemented")
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
    let _ = (conn, day);
    panic!("not implemented")
}

/// Render `rows` in the given `format`. Slice 1 supports only
/// [`Format::Text`]: one line per row —
/// `repo: {repo}  description: {description}  time: {H} hrs  type: {Work|Personal}`.
pub fn render(rows: &[BillingRow], format: Format) -> String {
    let _ = (rows, format);
    panic!("not implemented")
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
        let mut ev = Event::minimal(
            "github_commit",
            source_id,
            "2026-04-18T09:00:00Z",
            "commit",
        );
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
        let block_id = seed_block(&conn, "2026-04-18", "2026-04-18T09:00:00+00:00", 3600, None, None, false);
        seed_event(&conn, block_id, "e1", Some("A"), None);
        seed_event(&conn, block_id, "e2", Some("A"), None);
        seed_event(&conn, block_id, "e3", Some("B"), None);

        let got = dominant_repo_for_block(&conn, block_id).unwrap();
        assert_eq!(got.as_deref(), Some("A"));
    }

    #[test]
    fn dominant_repo_is_none_without_any_repo_signal() {
        let conn = open_memory().unwrap();
        let block_id = seed_block(&conn, "2026-04-18", "2026-04-18T09:00:00+00:00", 3600, None, None, false);
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

    #[test]
    fn rows_for_day_excludes_personal_blocks_in_slice_one() {
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
        assert!(rows.is_empty(), "slice 1 excludes personal blocks");
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
        let expected = "repo: genai-infra  description: did another thing  time: 5,5 hrs  type: Work\n\
             repo: genai-infra  description: did the thing  time: 4 hrs  type: Work";
        assert_eq!(text, expected);
    }
}
