//! Block mutations — the in-process API used by both the axum server
//! (Stage 3.2) and any future direct Rust callers.
//!
//! Every mutation takes a `&Connection` so the caller owns transactions
//! across multiple operations. Returns the fresh block row so clients
//! don't need a second round-trip.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use crate::models::Block;
use crate::repo;

/// `dirty = CASE WHEN tempo_worklog_id IS ... THEN 1 ELSE dirty END` — set
/// the dirty flag only when the block has already been synced. Unsynced
/// blocks don't need the marker because the next sync picks them up via
/// `tempo_worklog_id IS NULL` anyway, and we don't want every estimator
/// pass to leave the day with a row of false-positive dirty pills.
const MARK_DIRTY_IF_SYNCED: &str =
    "CASE WHEN tempo_worklog_id IS NOT NULL AND tempo_worklog_id != '' THEN 1 ELSE dirty END";

pub fn assign_ticket(conn: &Connection, block_id: i64, key: Option<&str>) -> Result<Block> {
    // A block that's been assigned a real ticket is, by definition,
    // work — flip is_personal off so it leaves the dimmed "personal"
    // footer in the UI and starts going through estimate + tempo sync.
    // Clearing the ticket leaves is_personal alone; the user might still
    // want the personal flag managed by the path classifier on the next
    // `worklog tag reclassify`.
    if key.is_some() {
        conn.execute(
            &format!(
                "UPDATE blocks
                    SET jira_issue = ?1, is_personal = 0, dirty = {MARK_DIRTY_IF_SYNCED}
                  WHERE id = ?2"
            ),
            params![key, block_id],
        )
        .context("assign_ticket")?;
    } else {
        conn.execute(
            &format!(
                "UPDATE blocks
                    SET jira_issue = NULL, dirty = {MARK_DIRTY_IF_SYNCED}
                  WHERE id = ?1"
            ),
            params![block_id],
        )
        .context("assign_ticket")?;
    }
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

pub fn set_duration(conn: &Connection, block_id: i64, minutes: u32) -> Result<Block> {
    // Read started_at so we can derive the matching ended_at. Leaving
    // ended_at stale makes it disagree with duration_seconds, which
    // confuses both the UI and anyone who inspects the raw DB.
    let started_at: String = conn
        .query_row(
            "SELECT started_at FROM blocks WHERE id = ?1",
            params![block_id],
            |r| r.get(0),
        )
        .with_context(|| format!("block {block_id} not found"))?;
    let new_end = derive_ended_at(&started_at, minutes as i64 * 60)?;

    // Mark as manual so re-estimation doesn't clobber it.
    conn.execute(
        &format!(
            "UPDATE blocks
                SET duration_seconds = ?1,
                    ended_at = ?2,
                    estimated_by = 'manual',
                    dirty = {MARK_DIRTY_IF_SYNCED}
              WHERE id = ?3"
        ),
        params![minutes as i64 * 60, new_end, block_id],
    )
    .context("set_duration")?;
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

/// Compute the canonical ended_at string from started_at (ISO-8601) and
/// duration. Output format mirrors how the infer+repo layers emit
/// timestamps so round-trips stay byte-stable.
fn derive_ended_at(started_at: &str, duration_seconds: i64) -> Result<String> {
    let start: DateTime<Utc> = DateTime::parse_from_rfc3339(started_at)
        .with_context(|| format!("parsing started_at `{started_at}` as RFC3339"))?
        .with_timezone(&Utc);
    let end = start + Duration::seconds(duration_seconds);
    // Match the repo's emission: `2026-04-18T09:00:00+00:00` (no fractional
    // seconds when whole seconds). chrono's `%:z` yields `+00:00`.
    Ok(end.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

pub fn set_description(conn: &Connection, block_id: i64, description: &str) -> Result<Block> {
    conn.execute(
        &format!(
            "UPDATE blocks
                SET description = ?1,
                    estimated_by = 'manual',
                    dirty = {MARK_DIRTY_IF_SYNCED}
              WHERE id = ?2"
        ),
        params![description, block_id],
    )
    .context("set_description")?;
    repo::get_block(conn, block_id)?.ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))
}

pub fn delete_block(conn: &Connection, block_id: i64) -> Result<()> {
    let n = conn
        .execute("DELETE FROM blocks WHERE id = ?1", params![block_id])
        .context("delete_block")?;
    if n == 0 {
        anyhow::bail!("block {block_id} not found");
    }
    Ok(())
}

/// True when a block carries a real Tempo worklog id — i.e. it has been
/// synced. Both `NULL` and `""` count as unsynced (see CLAUDE.md invariant
/// on the `tempo_worklog_id` canary).
fn is_synced(b: &Block) -> bool {
    b.tempo_worklog_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
}

/// Outcome of a [`merge_blocks`] call: the surviving (primary) block plus
/// the ids that were absorbed into it and deleted.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeOutcome {
    pub merged: Block,
    pub absorbed: Vec<i64>,
}

/// Merge one or more blocks into `primary_id`.
///
/// The primary survives and keeps its identity — `jira_issue`,
/// `description`, `estimated_by`, `is_personal`, `flagged` and, crucially,
/// `tempo_worklog_id`. The absorbed blocks contribute their linked events
/// and their logged duration, then their rows are deleted.
///
/// The merged block starts at the earliest `started_at` across every
/// input; its `duration_seconds` is the SUM of every input's duration
/// (logged time — the quantity that ultimately reaches Tempo), and
/// `ended_at` is re-derived from start + summed duration so the row stays
/// internally consistent, the same invariant [`set_duration`] upholds.
///
/// Refuses the merge when an *absorbed* block is already synced to Tempo:
/// deleting it locally would orphan its remote worklog. The primary may be
/// synced — it survives, and is marked `dirty` so the next `worklog sync`
/// PUTs the new total instead of leaving Tempo stale.
pub fn merge_blocks(
    conn: &Connection,
    primary_id: i64,
    absorb_ids: &[i64],
) -> Result<MergeOutcome> {
    if absorb_ids.is_empty() {
        anyhow::bail!("merge needs at least one other block to absorb into block {primary_id}");
    }
    let primary = repo::get_block(conn, primary_id)?
        .ok_or_else(|| anyhow::anyhow!("block {primary_id} not found"))?;

    let mut others = Vec::with_capacity(absorb_ids.len());
    for &id in absorb_ids {
        if id == primary_id {
            anyhow::bail!("cannot merge block {primary_id} into itself");
        }
        if others.iter().any(|b: &Block| b.id == id) {
            anyhow::bail!("block {id} listed twice — each block can only be merged once");
        }
        let b =
            repo::get_block(conn, id)?.ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        if b.day != primary.day {
            anyhow::bail!(
                "block {id} is on {} but primary block {primary_id} is on {} — \
                 only blocks on the same day can be merged",
                b.day,
                primary.day
            );
        }
        if is_synced(&b) {
            anyhow::bail!(
                "block {id} is already synced to Tempo — merging it away would \
                 orphan its remote worklog. Delete it first, or re-run the merge \
                 with block {id} as the primary so it survives."
            );
        }
        others.push(b);
    }

    // Earliest start wins; duration is the sum of logged time across all
    // inputs (contiguous blocks → span == sum; blocks with gaps → the
    // user explicitly chose to merge, so summing preserves logged time).
    let started_at = std::iter::once(&primary)
        .chain(others.iter())
        .map(|b| b.started_at.clone())
        .min()
        .expect("primary is always present");
    let total_seconds: i64 = std::iter::once(&primary)
        .chain(others.iter())
        .map(|b| b.duration_seconds)
        .sum();
    let ended_at = derive_ended_at(&started_at, total_seconds)?;

    let tx = conn.unchecked_transaction()?;
    for o in &others {
        // Re-point this block's events at the primary. `OR IGNORE` drops
        // the row when the event is already linked to the primary
        // (block_events PK is `(block_id, event_id)`); the leftover row
        // is then swept by `ON DELETE CASCADE` when the block row goes.
        tx.execute(
            "UPDATE OR IGNORE block_events SET block_id = ?1 WHERE block_id = ?2",
            params![primary_id, o.id],
        )
        .context("merge_blocks: re-linking events")?;
        tx.execute("DELETE FROM blocks WHERE id = ?1", params![o.id])
            .context("merge_blocks: deleting absorbed block")?;
    }
    tx.execute(
        &format!(
            "UPDATE blocks
                SET started_at = ?1,
                    ended_at = ?2,
                    duration_seconds = ?3,
                    dirty = {MARK_DIRTY_IF_SYNCED}
              WHERE id = ?4"
        ),
        params![started_at, ended_at, total_seconds, primary_id],
    )
    .context("merge_blocks: updating primary")?;
    tx.commit().context("merge_blocks: commit")?;

    let merged = repo::get_block(conn, primary_id)?
        .ok_or_else(|| anyhow::anyhow!("block {primary_id} not found"))?;
    Ok(MergeOutcome {
        merged,
        absorbed: others.iter().map(|b| b.id).collect(),
    })
}

/// Outcome of a [`split_block`] call: the original (shortened) block and
/// the freshly created tail block.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SplitOutcome {
    pub first: Block,
    pub second: Block,
}

/// Split block `block_id` into two consecutive blocks.
///
/// `first_minutes` is the duration the *original* block keeps; the
/// remainder becomes a new tail block. The boundary timestamp is
/// `started_at + first_minutes`, and each linked event re-buckets to
/// whichever side its own `started_at` falls on.
///
/// The first block keeps its identity — including `tempo_worklog_id` — and
/// is marked `dirty` if it was synced (its duration shrank). The second is
/// always a fresh, unsynced block; it inherits the ticket, description,
/// `is_personal` and `flagged` of the original. Both halves are stamped
/// `estimated_by = 'manual'` since the user has taken manual control of
/// the durations — re-estimation must not clobber them.
pub fn split_block(conn: &Connection, block_id: i64, first_minutes: u32) -> Result<SplitOutcome> {
    let block = repo::get_block(conn, block_id)?
        .ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))?;
    let first_secs = first_minutes as i64 * 60;
    if first_secs <= 0 || first_secs >= block.duration_seconds {
        anyhow::bail!(
            "split point {first_minutes}m must be between 0 and the block's {}m duration",
            block.duration_seconds / 60
        );
    }
    let boundary = derive_ended_at(&block.started_at, first_secs)?;
    let second_secs = block.duration_seconds - first_secs;

    let tx = conn.unchecked_transaction()?;
    // The original block keeps its id; shrink it to the first slice.
    tx.execute(
        &format!(
            "UPDATE blocks
                SET duration_seconds = ?1,
                    ended_at = ?2,
                    estimated_by = 'manual',
                    dirty = {MARK_DIRTY_IF_SYNCED}
              WHERE id = ?3"
        ),
        params![first_secs, boundary, block_id],
    )
    .context("split_block: shrinking first block")?;
    // The tail is a brand-new, unsynced block — no tempo_worklog_id.
    tx.execute(
        "INSERT INTO blocks
            (day, jira_issue, started_at, ended_at, duration_seconds,
             description, estimated_by, flagged, tempo_worklog_id, is_personal, dirty)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', ?7, NULL, ?8, 0)",
        params![
            block.day,
            block.jira_issue,
            boundary,
            block.ended_at,
            second_secs,
            block.description,
            block.flagged as i64,
            block.is_personal as i64,
        ],
    )
    .context("split_block: inserting tail block")?;
    let second_id = tx.last_insert_rowid();
    // Re-bucket events: anything starting at/after the boundary moves to
    // the tail. (block_events PK won't collide — `second_id` is new.)
    tx.execute(
        "UPDATE block_events
            SET block_id = ?1
          WHERE block_id = ?2
            AND event_id IN (SELECT id FROM events WHERE started_at >= ?3)",
        params![second_id, block_id, boundary],
    )
    .context("split_block: re-bucketing events")?;
    tx.commit().context("split_block: commit")?;

    Ok(SplitOutcome {
        first: repo::get_block(conn, block_id)?
            .ok_or_else(|| anyhow::anyhow!("block {block_id} not found"))?,
        second: repo::get_block(conn, second_id)?
            .ok_or_else(|| anyhow::anyhow!("block {second_id} not found"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn seed(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18','2026-04-18T09:00:00+00:00','2026-04-18T09:30:00+00:00',1800)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn assign_ticket_updates_and_clears() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = assign_ticket(&conn, id, Some("PROJ-1")).unwrap();
        assert_eq!(got.jira_issue.as_deref(), Some("PROJ-1"));
        let got = assign_ticket(&conn, id, None).unwrap();
        assert!(got.jira_issue.is_none());
    }

    #[test]
    fn assign_ticket_flips_is_personal_off() {
        // Regression: a block that the path-classifier flagged personal
        // shouldn't STAY personal once the user manually picks a ticket
        // in the UI. assign_ticket is the boss of work/personal status.
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        conn.execute("UPDATE blocks SET is_personal = 1 WHERE id = ?1", [id])
            .unwrap();
        let got = assign_ticket(&conn, id, Some("PROJ-1")).unwrap();
        assert_eq!(got.jira_issue.as_deref(), Some("PROJ-1"));
        assert!(!got.is_personal, "ticket assignment must clear is_personal");
    }

    #[test]
    fn clearing_ticket_does_not_touch_is_personal() {
        // Inverse: clearing the ticket shouldn't auto-flip is_personal
        // either way — let the path classifier / reclassify decide.
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        conn.execute(
            "UPDATE blocks SET jira_issue = 'PROJ-1', is_personal = 0 WHERE id = ?1",
            [id],
        )
        .unwrap();
        let got = assign_ticket(&conn, id, None).unwrap();
        assert!(got.jira_issue.is_none());
        assert!(!got.is_personal, "clearing must not toggle is_personal");
    }

    #[test]
    fn set_duration_marks_manual_and_stores_seconds() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = set_duration(&conn, id, 45).unwrap();
        assert_eq!(got.duration_seconds, 45 * 60);
        assert_eq!(got.estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn set_duration_returns_error_on_naive_started_at() {
        // started_at must be RFC3339 (with offset). If a row somehow got
        // stored without one — shouldn't happen post-stage-1, but
        // defensive — set_duration must error cleanly rather than panic.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00', '2026-04-18T09:30:00', 1800)",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let err = set_duration(&conn, id, 60).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("RFC3339") || msg.contains("parsing started_at"),
            "expected parse error context, got: {msg}"
        );
        // DB must be untouched — no half-update.
        let dur: i64 = conn
            .query_row(
                "SELECT duration_seconds FROM blocks WHERE id = ?",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dur, 1800, "duration must not change when parse fails");
    }

    #[test]
    fn set_duration_updates_ended_at_to_match() {
        // Regression: set_duration used to leave ended_at stale, so
        // ended_at - started_at disagreed with duration_seconds. Anyone
        // reading the raw DB (or the UI time range) saw the old end.
        let conn = open_memory().unwrap();
        let id = seed(&conn); // started 09:00, ended 09:30, 1800s
        let got = set_duration(&conn, id, 120).unwrap();
        assert_eq!(got.duration_seconds, 120 * 60);
        // 09:00 + 120m = 11:00 — match the same ISO format we store.
        assert_eq!(
            got.ended_at, "2026-04-18T11:00:00+00:00",
            "ended_at must track duration_seconds"
        );
    }

    #[test]
    fn set_description_marks_manual() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        let got = set_description(&conn, id, "hello").unwrap();
        assert_eq!(got.description.as_deref(), Some("hello"));
        assert_eq!(got.estimated_by.as_deref(), Some("manual"));
    }

    #[test]
    fn delete_block_removes_row() {
        let conn = open_memory().unwrap();
        let id = seed(&conn);
        delete_block(&conn, id).unwrap();
        assert!(repo::get_block(&conn, id).unwrap().is_none());
    }

    #[test]
    fn errors_on_missing_block() {
        let conn = open_memory().unwrap();
        assert!(assign_ticket(&conn, 9999, Some("PROJ-1")).is_err());
        assert!(set_duration(&conn, 9999, 10).is_err());
        assert!(set_description(&conn, 9999, "x").is_err());
        assert!(delete_block(&conn, 9999).is_err());
    }

    // ───────────────────────── merge ─────────────────────────

    /// Insert a block with explicit start/end/duration and return its id.
    fn seed_at(conn: &Connection, start: &str, end: &str, secs: i64) -> i64 {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', ?1, ?2, ?3)",
            params![start, end, secs],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Link an event to a block; returns the event id.
    fn link_event(conn: &Connection, block_id: i64, source_id: &str) -> i64 {
        let eid = repo::upsert_event(
            conn,
            &crate::models::Event::minimal("claude", source_id, "2026-04-18T09:00:00+00:00", "x"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, eid],
        )
        .unwrap();
        eid
    }

    #[test]
    fn merge_sums_duration_and_takes_earliest_start() {
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T11:00:00+00:00",
            "2026-04-18T11:30:00+00:00",
            1800,
        );
        let b = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:45:00+00:00",
            2700,
        );
        let out = merge_blocks(&conn, a, &[b]).unwrap();
        assert_eq!(out.merged.id, a);
        assert_eq!(out.absorbed, vec![b]);
        // earliest start across both inputs
        assert_eq!(out.merged.started_at, "2026-04-18T09:00:00+00:00");
        // 1800 + 2700 = 4500s of logged time
        assert_eq!(out.merged.duration_seconds, 4500);
        // ended_at re-derived from start + summed duration
        assert_eq!(out.merged.ended_at, "2026-04-18T10:15:00+00:00");
        // the absorbed block is gone
        assert!(repo::get_block(&conn, b).unwrap().is_none());
    }

    #[test]
    fn merge_relinks_events_onto_the_primary() {
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
        );
        let b = seed_at(
            &conn,
            "2026-04-18T10:00:00+00:00",
            "2026-04-18T10:30:00+00:00",
            1800,
        );
        link_event(&conn, a, "ev-a");
        link_event(&conn, b, "ev-b");
        merge_blocks(&conn, a, &[b]).unwrap();
        // both events now hang off the primary
        let events = repo::list_events_for_block(&conn, a).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn merge_keeps_primary_ticket_and_description() {
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
        );
        let b = seed_at(
            &conn,
            "2026-04-18T10:00:00+00:00",
            "2026-04-18T10:30:00+00:00",
            1800,
        );
        assign_ticket(&conn, a, Some("PROJ-1")).unwrap();
        set_description(&conn, a, "primary work").unwrap();
        assign_ticket(&conn, b, Some("PROJ-9")).unwrap();
        let out = merge_blocks(&conn, a, &[b]).unwrap();
        assert_eq!(out.merged.jira_issue.as_deref(), Some("PROJ-1"));
        assert_eq!(out.merged.description.as_deref(), Some("primary work"));
    }

    #[test]
    fn merge_refuses_when_an_absorbed_block_is_synced() {
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
        );
        let b = seed_at(
            &conn,
            "2026-04-18T10:00:00+00:00",
            "2026-04-18T10:30:00+00:00",
            1800,
        );
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = 'tmp-42' WHERE id = ?1",
            [b],
        )
        .unwrap();
        let err = merge_blocks(&conn, a, &[b]).unwrap_err();
        assert!(format!("{err}").contains("synced to Tempo"), "got: {err}");
        // nothing was deleted — the merge aborted before the transaction
        assert!(repo::get_block(&conn, b).unwrap().is_some());
    }

    #[test]
    fn merge_marks_a_synced_primary_dirty() {
        // The primary may legitimately be synced — it survives. Since its
        // duration changes, it must come out dirty so the next sync PUTs
        // the new total to Tempo.
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
        );
        let b = seed_at(
            &conn,
            "2026-04-18T10:00:00+00:00",
            "2026-04-18T10:30:00+00:00",
            1800,
        );
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = 'tmp-7' WHERE id = ?1",
            [a],
        )
        .unwrap();
        let out = merge_blocks(&conn, a, &[b]).unwrap();
        assert!(out.merged.dirty, "synced primary must be dirty after merge");
    }

    // ───────────────────────── split ─────────────────────────

    #[test]
    fn split_divides_duration_and_keeps_id() {
        let conn = open_memory().unwrap();
        // 09:00–10:00, 60 min.
        let id = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T10:00:00+00:00",
            3600,
        );
        let out = split_block(&conn, id, 20).unwrap();
        // first keeps the id, shrinks to 20m, ends at the boundary
        assert_eq!(out.first.id, id);
        assert_eq!(out.first.duration_seconds, 1200);
        assert_eq!(out.first.ended_at, "2026-04-18T09:20:00+00:00");
        // second is new, gets the remaining 40m, runs to the old end
        assert_ne!(out.second.id, id);
        assert_eq!(out.second.duration_seconds, 2400);
        assert_eq!(out.second.started_at, "2026-04-18T09:20:00+00:00");
        assert_eq!(out.second.ended_at, "2026-04-18T10:00:00+00:00");
    }

    #[test]
    fn split_second_block_inherits_ticket_and_is_unsynced() {
        let conn = open_memory().unwrap();
        let id = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T10:00:00+00:00",
            3600,
        );
        assign_ticket(&conn, id, Some("PROJ-1")).unwrap();
        set_description(&conn, id, "shared work").unwrap();
        let out = split_block(&conn, id, 30).unwrap();
        assert_eq!(out.second.jira_issue.as_deref(), Some("PROJ-1"));
        assert_eq!(out.second.description.as_deref(), Some("shared work"));
        // the tail is brand new — never synced, not dirty
        assert!(out.second.tempo_worklog_id.is_none());
        assert!(!out.second.dirty);
    }

    #[test]
    fn split_rebuckets_events_by_timestamp() {
        let conn = open_memory().unwrap();
        let id = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T10:00:00+00:00",
            3600,
        );
        // one event before the boundary, one after
        let early = repo::upsert_event(
            &conn,
            &crate::models::Event::minimal("claude", "e1", "2026-04-18T09:10:00+00:00", "x"),
        )
        .unwrap();
        let late = repo::upsert_event(
            &conn,
            &crate::models::Event::minimal("claude", "e2", "2026-04-18T09:50:00+00:00", "y"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2), (?1, ?3)",
            params![id, early, late],
        )
        .unwrap();
        let out = split_block(&conn, id, 30).unwrap(); // boundary 09:30
        let first_events = repo::list_events_for_block(&conn, out.first.id).unwrap();
        let second_events = repo::list_events_for_block(&conn, out.second.id).unwrap();
        assert_eq!(first_events.len(), 1);
        assert_eq!(first_events[0].source_id, "e1");
        assert_eq!(second_events.len(), 1);
        assert_eq!(second_events[0].source_id, "e2");
    }

    #[test]
    fn split_marks_synced_first_block_dirty() {
        let conn = open_memory().unwrap();
        let id = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T10:00:00+00:00",
            3600,
        );
        conn.execute(
            "UPDATE blocks SET tempo_worklog_id = 'tmp-3' WHERE id = ?1",
            [id],
        )
        .unwrap();
        let out = split_block(&conn, id, 25).unwrap();
        // first block shrank — synced, so it must come out dirty
        assert!(out.first.dirty);
        assert_eq!(out.first.tempo_worklog_id.as_deref(), Some("tmp-3"));
    }

    #[test]
    fn split_rejects_out_of_range() {
        let conn = open_memory().unwrap();
        let id = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T10:00:00+00:00",
            3600,
        );
        assert!(split_block(&conn, id, 0).is_err(), "zero split");
        assert!(split_block(&conn, id, 60).is_err(), "split == duration");
        assert!(split_block(&conn, id, 90).is_err(), "split > duration");
        assert!(split_block(&conn, 9999, 10).is_err(), "missing block");
    }

    #[test]
    fn merge_rejects_cross_day_self_and_empty() {
        let conn = open_memory().unwrap();
        let a = seed_at(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
        );
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-19', '2026-04-19T09:00:00+00:00', '2026-04-19T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let other_day = conn.last_insert_rowid();
        assert!(merge_blocks(&conn, a, &[]).is_err(), "empty absorb list");
        assert!(merge_blocks(&conn, a, &[a]).is_err(), "self-merge");
        assert!(
            merge_blocks(&conn, a, &[other_day]).is_err(),
            "cross-day merge"
        );
        assert!(merge_blocks(&conn, a, &[9999]).is_err(), "missing block");
    }
}
