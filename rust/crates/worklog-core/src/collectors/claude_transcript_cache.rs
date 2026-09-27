//! Per-file fingerprint cache backing the tick skip in
//! `claude_transcripts.rs` and `claude_helpers.rs`: a `.jsonl` byte-
//! identical (same size + mtime_ns) to what a previous run already fully
//! read over the same `[since_ts, until_ts)` window is skipped outright —
//! no read, no re-parse — instead of re-upserted. Stored in its own table
//! (`transcript_file_cache`, schema.sql) rather than on `events`.
//!
//! A skip must still reproduce two things a full read would have done:
//! - seed the run's cross-file `seen` set with the uuids this file itself
//!   won the dedupe race for last time (a resumed session can copy an
//!   older file's lines, same uuid, into a new file — the first file to
//!   claim a uuid decides which file's per-minute summary it lands in);
//! - replay the events it wrote into `CollectReport.events_written`, so
//!   `worklog day`'s printed "events=N" doesn't change on a skip.
//!
//! `extra_key` covers state a file's own bytes don't capture — the
//! transcript collector's background-job session set for session files,
//! `""` for helper files (which never consult it).

use super::CollectReport;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

/// What a file contributed to a run over one window, last time it was
/// fully read (see the module doc for why both fields matter to a skip).
pub(super) struct CachedFile {
    pub(super) claimed_uuids: Vec<String>,
    pub(super) events_written: usize,
}

/// Claims `uuid` in this run's cross-file dedupe set, recording it in
/// `claimed` on first claim so a later tick can skip re-reading this same
/// file and still reseed `seen` exactly via [`store`]/[`lookup`]. Returns
/// `false` when another file/line already claimed it, exactly like
/// `HashSet::insert`.
pub(super) fn claim(seen: &mut HashSet<String>, claimed: &mut Vec<String>, uuid: &str) -> bool {
    let first = seen.insert(uuid.to_string());
    if first {
        claimed.push(uuid.to_string());
    }
    first
}

/// Session ids belonging to a background job (`<jobs_dir>/<id>/state.json`
/// -> `sessionId`): their Claude-busy minutes add no time (D-05). Also the
/// input to a session file's cache `extra_key` (module doc) — a session
/// crossing this set changes how `claude_transcripts::collect_file`
/// classifies its working minutes, so a fingerprint from before the
/// crossing must not be reused after it.
pub(super) fn background_job_sessions(jobs_dir: &Path) -> HashSet<String> {
    let mut sessions = HashSet::new();
    let Ok(entries) = std::fs::read_dir(jobs_dir) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path().join("state.json")) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            sessions.insert(id.to_string());
        }
    }
    sessions
}

/// Skips `read` when `path`'s fingerprint matches a previous full read over
/// this exact window AND none of the uuids it claimed that time have
/// already been claimed by a sibling file earlier in *this* run (a
/// brand-new resumed copy can beat an unchanged original to a shared uuid —
/// trusting the stale contribution over that would misreport it, so that
/// case falls through to a full `read` instead, exactly like the file
/// arriving unchanged never happened). Otherwise runs `read` (which claims
/// uuids via [`claim`] into its own `&mut Vec<String>`) and stores what it
/// claimed + wrote. Shared by `claude_transcripts::collect_file` (session
/// files) and `claude_helpers::collect_agent_file` (helper files) so the
/// check/store bookkeeping lives in one place.
#[allow(clippy::too_many_arguments)]
pub(super) fn skip_or_read(
    conn: &Connection,
    path: &str,
    since_ts: i64,
    until_ts: i64,
    size: u64,
    mtime_ns: i64,
    extra_key: &str,
    seen: &mut HashSet<String>,
    report: &mut CollectReport,
    read: impl FnOnce(&mut HashSet<String>, &mut Vec<String>, &mut CollectReport) -> Result<()>,
) -> Result<()> {
    let cached = lookup(conn, path, since_ts, until_ts, size, mtime_ns, extra_key)
        .filter(|cached| cached.claimed_uuids.iter().all(|u| !seen.contains(u)));
    if let Some(cached) = cached {
        seen.extend(cached.claimed_uuids);
        report.events_written += cached.events_written;
        return Ok(());
    }
    let mut claimed = Vec::new();
    let events_before = report.events_written;
    read(seen, &mut claimed, report)?;
    store(
        conn,
        path,
        since_ts,
        until_ts,
        size,
        mtime_ns,
        &claimed,
        report.events_written - events_before,
        extra_key,
    )
}

/// `Some` when `path` was already fully read for this exact
/// `(since_ts, until_ts, extra_key)` and its size/mtime haven't changed.
pub(super) fn lookup(
    conn: &Connection,
    path: &str,
    since_ts: i64,
    until_ts: i64,
    size: u64,
    mtime_ns: i64,
    extra_key: &str,
) -> Option<CachedFile> {
    let row: (String, i64) = conn
        .query_row(
            "SELECT claimed_uuids_json, events_written FROM transcript_file_cache
              WHERE path = ?1 AND since_ts = ?2 AND until_ts = ?3
                AND size = ?4 AND mtime_ns = ?5 AND extra_key = ?6",
            params![path, since_ts, until_ts, size as i64, mtime_ns, extra_key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()??;
    let claimed_uuids = serde_json::from_str(&row.0).ok()?;
    Some(CachedFile {
        claimed_uuids,
        events_written: row.1 as usize,
    })
}

/// Record a full read's fingerprint + contribution, replacing any stale
/// row for the same `path` + window (a changed `extra_key` invalidates the
/// old row on the next lookup instead of leaving two rows behind).
#[allow(clippy::too_many_arguments)]
pub(super) fn store(
    conn: &Connection,
    path: &str,
    since_ts: i64,
    until_ts: i64,
    size: u64,
    mtime_ns: i64,
    claimed_uuids: &[String],
    events_written: usize,
    extra_key: &str,
) -> Result<()> {
    let json = serde_json::to_string(claimed_uuids).context("serializing claimed uuids")?;
    conn.execute(
        "INSERT INTO transcript_file_cache
            (path, since_ts, until_ts, size, mtime_ns, claimed_uuids_json, events_written, extra_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(path, since_ts, until_ts) DO UPDATE SET
            size                = excluded.size,
            mtime_ns            = excluded.mtime_ns,
            claimed_uuids_json  = excluded.claimed_uuids_json,
            events_written      = excluded.events_written,
            extra_key           = excluded.extra_key",
        params![
            path,
            since_ts,
            until_ts,
            size as i64,
            mtime_ns,
            json,
            events_written as i64,
            extra_key,
        ],
    )
    .context("upserting transcript_file_cache")?;
    Ok(())
}

/// Any code path that deletes rows from `events` must call this in the same
/// transaction (currently `purge::purge_rows`): a cached fingerprint doesn't
/// know its file's rows were deleted out from under it, so an unmodified
/// file would keep being skipped forever and its deleted rows would never
/// come back on a later tick. Clearing the whole table is cheap and always
/// safe — every file it drops just gets a normal full read next time.
pub fn clear_after_events_delete(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM transcript_file_cache", [])
        .context("clearing transcript_file_cache after an events delete")?;
    Ok(())
}

/// Called once per collect run, before any file in `dir`/`jobs_dir` is
/// looked up. Two invalidations a per-file size/mtime fingerprint alone
/// can't catch:
/// - retention: rows for any window other than `(since_ts, until_ts)` are
///   dropped, so the table holds at most one window's worth of state
///   instead of growing forever, one row per file per day, across ticks;
/// - a cached path for THIS window that no longer exists on disk means a
///   file was removed since it was last read (e.g. a resumed session's
///   original file). Cross-file uuid ownership for the window could differ
///   from a fresh run in that case, so the window's cache is dropped
///   entirely and this run re-reads every file in it from scratch.
pub(super) fn prepare_window(conn: &Connection, since_ts: i64, until_ts: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM transcript_file_cache WHERE NOT (since_ts = ?1 AND until_ts = ?2)",
        params![since_ts, until_ts],
    )
    .context("clearing stale transcript_file_cache windows")?;

    let mut stmt = conn
        .prepare("SELECT path FROM transcript_file_cache WHERE since_ts = ?1 AND until_ts = ?2")
        .context("preparing transcript_file_cache path lookup")?;
    let paths: Vec<String> = stmt
        .query_map(params![since_ts, until_ts], |r| r.get(0))
        .context("querying transcript_file_cache paths")?
        .collect::<rusqlite::Result<_>>()
        .context("reading transcript_file_cache paths")?;
    drop(stmt);
    if paths.iter().any(|p| !Path::new(p).exists()) {
        conn.execute(
            "DELETE FROM transcript_file_cache WHERE since_ts = ?1 AND until_ts = ?2",
            params![since_ts, until_ts],
        )
        .context("clearing transcript_file_cache after a missing cached file")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "claude_transcript_cache_test.rs"]
mod tests;
