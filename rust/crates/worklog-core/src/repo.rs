//! Repository layer. Thin, typed queries over `rusqlite::Connection`.
//!
//! Invariant: every collector writes through `upsert_event`, dedupe keyed on
//! `(source, source_id)` — mirrors `db.upsert_event` in the Python codebase.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Block, Event, JiraTicket};
use crate::raw_json::{decode_raw_json, encode_raw_json};
use crate::scrub;
use crate::tempo_hub_contract::StatusCategory;
use crate::tempo_line_contract::TicketOrigin;

// ───────────────────────── events ─────────────────────────

/// `upsert_event`'s statement: dedupe on `(source, source_id)`, update the
/// mutable columns in place.
const UPSERT_EVENT_SQL: &str = "INSERT INTO events
        (source, source_id, started_at, ended_at, duration_seconds,
         title, details, repo, project_path, jira_issue, session_id,
         tempo_worklog_id, raw_json)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
     ON CONFLICT(source, source_id) DO UPDATE SET
        started_at       = excluded.started_at,
        -- COALESCE: collectors don't populate these on re-collect,
        -- so a None on the new side must not wipe existing data.
        ended_at         = COALESCE(excluded.ended_at, events.ended_at),
        duration_seconds = COALESCE(excluded.duration_seconds, events.duration_seconds),
        title            = excluded.title,
        details          = excluded.details,
        repo             = excluded.repo,
        -- project_path: routing::set_label writes this via a raw
        -- UPDATE, never through this function. Collectors always
        -- pass None here, so unconditional overwrite would wipe a
        -- routing label on every re-collect of the same event.
        project_path     = COALESCE(excluded.project_path, events.project_path),
        jira_issue       = excluded.jira_issue,
        -- session_id: preserve existing when the new side doesn't
        -- carry one — matches Python and prevents e.g. a GitHub
        -- re-collect from wiping the claude session linkage.
        session_id       = COALESCE(events.session_id, excluded.session_id),
        -- tempo_worklog_id: the CLAUDE.md canary. Must NEVER be
        -- cleared. Collectors always pass None here, so unconditional
        -- overwrite would destroy the double-sync guard on every
        -- re-collect. COALESCE keeps the existing value.
        tempo_worklog_id = COALESCE(events.tempo_worklog_id, excluded.tempo_worklog_id),
        raw_json         = excluded.raw_json
     -- Skip the write when every SET above would store what is already
     -- there: the 15-min tick re-collects thousands of unchanged events,
     -- and an identical rewrite still dirties the page and grows the WAL.
     WHERE events.started_at IS NOT excluded.started_at
        OR events.ended_at IS NOT COALESCE(excluded.ended_at, events.ended_at)
        OR events.duration_seconds
               IS NOT COALESCE(excluded.duration_seconds, events.duration_seconds)
        OR events.title IS NOT excluded.title
        OR events.details IS NOT excluded.details
        OR events.repo IS NOT excluded.repo
        OR events.project_path IS NOT COALESCE(excluded.project_path, events.project_path)
        OR events.jira_issue IS NOT excluded.jira_issue
        OR events.session_id IS NOT COALESCE(events.session_id, excluded.session_id)
        OR events.tempo_worklog_id
               IS NOT COALESCE(events.tempo_worklog_id, excluded.tempo_worklog_id)
        OR events.raw_json IS NOT excluded.raw_json";

/// Insert or update an event. Returns the event id.
///
/// Dedupe is enforced by the `UNIQUE(source, source_id)` constraint in
/// `schema.sql`; on conflict we update the mutable columns in place.
///
/// `title`/`details` are scrubbed here, once, for every collector (FR-11,
/// D-03) — `raw_json` isn't: collectors already scrub it structurally
/// before building the `RawRecord`, and re-running a text-level regex
/// over serialized JSON risks corrupting its syntax.
pub fn upsert_event(conn: &Connection, e: &Event) -> Result<i64> {
    let title = scrub::scrub_secrets(&e.title);
    let details = e.details.as_deref().map(scrub::scrub_secrets);
    conn.execute(
        UPSERT_EVENT_SQL,
        params![
            e.source,
            e.source_id,
            e.started_at,
            e.ended_at,
            e.duration_seconds,
            title,
            details,
            e.repo,
            e.project_path,
            e.jira_issue,
            e.session_id,
            e.tempo_worklog_id,
            encode_raw_json(e.raw_json.as_deref()),
        ],
    )
    .context("upsert event")?;

    let id: i64 = conn
        .query_row(
            "SELECT id FROM events WHERE source = ?1 AND source_id = ?2",
            params![e.source, e.source_id],
            |r| r.get(0),
        )
        .context("fetching event id after upsert")?;
    Ok(id)
}

pub fn count_events(conn: &Connection) -> Result<i64> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
    Ok(n)
}

/// Events whose `started_at` falls on a given ISO-8601 day (UTC).
pub fn load_day_events(conn: &Connection, day: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, started_at, ended_at, duration_seconds,
                title, details, repo, project_path, jira_issue, session_id,
                tempo_worklog_id, raw_json
           FROM events
          WHERE substr(started_at, 1, 10) = ?1
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(Event {
            id: Some(r.get(0)?),
            source: r.get(1)?,
            source_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            title: r.get(6)?,
            details: r.get(7)?,
            repo: r.get(8)?,
            project_path: r.get(9)?,
            jira_issue: r.get(10)?,
            session_id: r.get(11)?,
            tempo_worklog_id: r.get(12)?,
            raw_json: decode_raw_json(r, 13)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ───────────────────────── blocks ─────────────────────────

pub fn list_blocks_for_day(conn: &Connection, day: &str) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(
        "SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
                description, estimated_by, flagged, tempo_worklog_id, is_personal, dirty,
                exported_at, ignored_at, ticket_origin
           FROM blocks
          WHERE day = ?1
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![day], |r| {
        Ok(Block {
            id: r.get(0)?,
            day: r.get(1)?,
            jira_issue: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            description: r.get(6)?,
            estimated_by: r.get(7)?,
            flagged: r.get::<_, i64>(8)? != 0,
            tempo_worklog_id: r.get(9)?,
            is_personal: r.get::<_, i64>(10)? != 0,
            dirty: r.get::<_, i64>(11)? != 0,
            exported_at: r.get(12)?,
            ignored_at: r.get(13)?,
            ticket_origin: r
                .get::<_, Option<String>>(14)?
                .as_deref()
                .and_then(TicketOrigin::parse),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Events linked to a specific block via `block_events`, ordered by
/// their own `started_at`. Used by the per-block events drill-down in
/// the web UI.
pub fn list_events_for_block(conn: &Connection, block_id: i64) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.source, e.source_id, e.started_at, e.ended_at,
                e.duration_seconds, e.title, e.details, e.repo,
                e.project_path, e.jira_issue, e.session_id,
                e.tempo_worklog_id, e.raw_json
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1
          ORDER BY e.started_at",
    )?;
    let rows = stmt.query_map(params![block_id], |r| {
        Ok(Event {
            id: Some(r.get(0)?),
            source: r.get(1)?,
            source_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            duration_seconds: r.get(5)?,
            title: r.get(6)?,
            details: r.get(7)?,
            repo: r.get(8)?,
            project_path: r.get(9)?,
            jira_issue: r.get(10)?,
            session_id: r.get(11)?,
            tempo_worklog_id: r.get(12)?,
            raw_json: decode_raw_json(r, 13)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_block(conn: &Connection, id: i64) -> Result<Option<Block>> {
    let block = conn
        .query_row(
            "SELECT id, day, jira_issue, started_at, ended_at, duration_seconds,
                    description, estimated_by, flagged, tempo_worklog_id, is_personal, dirty,
                    exported_at, ignored_at, ticket_origin
               FROM blocks WHERE id = ?1",
            params![id],
            |r| {
                Ok(Block {
                    id: r.get(0)?,
                    day: r.get(1)?,
                    jira_issue: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    duration_seconds: r.get(5)?,
                    description: r.get(6)?,
                    estimated_by: r.get(7)?,
                    flagged: r.get::<_, i64>(8)? != 0,
                    tempo_worklog_id: r.get(9)?,
                    is_personal: r.get::<_, i64>(10)? != 0,
                    dirty: r.get::<_, i64>(11)? != 0,
                    exported_at: r.get(12)?,
                    ignored_at: r.get(13)?,
                    ticket_origin: r
                        .get::<_, Option<String>>(14)?
                        .as_deref()
                        .and_then(TicketOrigin::parse),
                })
            },
        )
        .optional()?;
    Ok(block)
}

// ───────────────────────── jira tickets ─────────────────────────

pub fn upsert_ticket(conn: &Connection, t: &JiraTicket) -> Result<()> {
    // This path is used by the assignee=currentUser() refresh, so any
    // matching key is by definition no longer "external" — reset the
    // flag so a previously-external pick gets promoted cleanly once it
    // shows up in your assigned set.
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, project_key, updated, issue_id, external)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
         ON CONFLICT(key) DO UPDATE SET
            summary     = excluded.summary,
            status      = excluded.status,
            project_key = excluded.project_key,
            updated     = excluded.updated,
            -- Don't clobber a known issue_id with NULL from a partial
            -- refresh — only overwrite when the incoming row has one.
            issue_id    = COALESCE(excluded.issue_id, jira_tickets.issue_id),
            external    = 0,
            fetched_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            t.key,
            t.summary,
            t.status,
            t.project_key,
            t.updated,
            t.issue_id
        ],
    )
    .context("upsert jira ticket")?;
    Ok(())
}

pub fn set_ticket_status(
    conn: &Connection,
    key: &str,
    status: &str,
    category: Option<StatusCategory>,
) -> Result<()> {
    conn.execute(
        "UPDATE jira_tickets SET status = ?2, status_category = ?3 WHERE key = ?1",
        params![key, status, category.map(StatusCategory::as_str)],
    )
    .context("set jira ticket status")?;
    Ok(())
}

/// Rich Jira fields shown on My Tasks cards.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TicketDetails {
    pub issue_type: Option<String>,
    pub priority: Option<String>,
    pub due_date: Option<String>,
    pub labels: Vec<String>,
    pub parent_summary: Option<String>,
}

/// Overwrites all five detail columns; `labels` is a JSON array, NULL when empty.
pub fn set_ticket_details(conn: &Connection, key: &str, d: &TicketDetails) -> Result<()> {
    let labels = if d.labels.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&d.labels).context("encode labels")?)
    };
    conn.execute(
        "UPDATE jira_tickets
            SET issue_type = ?2, priority = ?3, due_date = ?4, labels = ?5, parent_summary = ?6
          WHERE key = ?1",
        params![
            key,
            d.issue_type,
            d.priority,
            d.due_date,
            labels,
            d.parent_summary
        ],
    )
    .context("set jira ticket details")?;
    Ok(())
}

/// Assigned tickets absent from a complete refresh are no longer open
/// for the user; externals are picked by hand and never aged out.
pub fn mark_unreturned_done(conn: &Connection, returned_keys: &[String]) -> Result<usize> {
    let keys = serde_json::to_string(returned_keys)?;
    conn.execute(
        "UPDATE jira_tickets SET status_category = 'done'
          WHERE external = 0
            AND key NOT IN (SELECT value FROM json_each(?1))",
        params![keys],
    )
    .context("mark unreturned jira tickets done")
}

/// Cache a ticket the user picked manually via the in-UI Jira search.
/// Sets `external = 1` on INSERT but DELIBERATELY does not touch the
/// flag on conflict — if the same key was previously synced as an
/// assigned ticket (`external = 0`), we don't want to downgrade it. The
/// estimator reads `WHERE external = 0`, so externals stay invisible
/// to Claude while still rendering in the picker with their summary.
pub fn upsert_external_ticket(conn: &Connection, t: &JiraTicket) -> Result<()> {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, status, project_key, updated, issue_id, external)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
         ON CONFLICT(key) DO UPDATE SET
            summary     = excluded.summary,
            status      = excluded.status,
            project_key = COALESCE(excluded.project_key, jira_tickets.project_key),
            updated     = excluded.updated,
            issue_id    = COALESCE(excluded.issue_id, jira_tickets.issue_id),
            fetched_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            t.key,
            t.summary,
            t.status,
            t.project_key,
            t.updated,
            t.issue_id
        ],
    )
    .context("upsert external jira ticket")?;
    Ok(())
}

pub fn list_tickets(conn: &Connection) -> Result<Vec<JiraTicket>> {
    let mut stmt = conn.prepare(
        "SELECT key, summary, status, project_key, updated, issue_id
           FROM jira_tickets
          ORDER BY updated DESC NULLS LAST, key",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(JiraTicket {
            key: r.get(0)?,
            summary: r.get(1)?,
            status: r.get(2)?,
            project_key: r.get(3)?,
            updated: r.get(4)?,
            issue_id: r.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Cached numeric issue_id for a key, if known. Tempo sync calls this
/// before paying the cost of a Jira REST roundtrip.
pub fn get_ticket_issue_id(conn: &Connection, key: &str) -> Result<Option<String>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT issue_id FROM jira_tickets WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(id)
}

/// Write back an issue_id resolved on the fly by the tempo sync.
/// Inserts the row if the ticket isn't cached yet — partial data is
/// better than no data, and the next `worklog collect jira` will fill
/// in the rest.
pub fn set_ticket_issue_id(conn: &Connection, key: &str, issue_id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO jira_tickets (key, summary, issue_id)
         VALUES (?1, '', ?2)
         ON CONFLICT(key) DO UPDATE SET issue_id = excluded.issue_id",
        params![key, issue_id],
    )
    .context("set_ticket_issue_id")?;
    Ok(())
}

// Tests live in repo_test.rs (same module, split file for line budget).
#[cfg(test)]
#[path = "repo_test.rs"]
mod tests;
