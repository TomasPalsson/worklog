//! Session customer pins — "session S is for customer C from time T on"
//! (spec 008, design.md §1). Stored in `session_pins`; a pin is set by
//! the `worklog pin` command (`source = "claude"`) or copied into a new
//! session on the same branch at start (`source = "inherited"`).

use std::path::Path;

use anyhow::{Context, Result};
use chrono::DateTime;
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::billing;
use crate::billing_registry::Registry;

/// One pin row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPin {
    pub session_id: String,
    pub customer: String,
    pub from_at: DateTime<Utc>,
    pub folder: String,
    pub branch: Option<String>,
    pub source: String,
}

/// Why `pin` refused to store a pin.
#[derive(Debug)]
pub enum PinError {
    /// `name` didn't resolve to a known customer or alias — nothing was
    /// stored. `known` is every customer name, for the caller to print.
    UnknownCustomer {
        known: Vec<String>,
    },
    Other(anyhow::Error),
}

impl From<anyhow::Error> for PinError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

/// `true` for the branches a pin never applies to (§1, `is_default_branch`).
pub fn is_default_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
}

/// The `worklog session-hint` start-of-session text (design.md §4,
/// contract T004), or `None` when nothing should be printed.
pub fn start_text(
    _conn: &Connection,
    _registry: &Registry,
    _session_id: &str,
    _cwd: &Path,
    _now: DateTime<Utc>,
) -> Result<Option<String>> {
    unimplemented!("T004")
}

/// Pin `session_id` to `name` from `at` on.
///
/// `name` must resolve to a known customer name or alias
/// (case-insensitively) via [`Registry::customer_named`] — anything else
/// is refused and nothing is stored. Re-pinning the same session at the
/// same `at` overwrites that row; a different `at` adds a new one, so a
/// session can carry several pins across time.
pub fn pin(
    conn: &Connection,
    registry: &Registry,
    session_id: &str,
    cwd: &Path,
    name: &str,
    at: DateTime<Utc>,
    branch: Option<&str>,
) -> Result<SessionPin, PinError> {
    let customer = registry
        .customer_named(name)
        .ok_or_else(|| PinError::UnknownCustomer {
            known: registry.customers.iter().map(|c| c.name.clone()).collect(),
        })?;
    let folder = billing::work_folder_for_path(&cwd.to_string_lossy()).ok_or_else(|| {
        anyhow::anyhow!("cwd {} is not under a usable work folder", cwd.display())
    })?;
    let branch = branch.map(str::to_owned);
    let source = "claude";

    conn.execute(
        "INSERT INTO session_pins (session_id, customer, from_at, folder, branch, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(session_id, from_at) DO UPDATE SET
             customer = excluded.customer,
             folder = excluded.folder,
             branch = excluded.branch,
             source = excluded.source",
        params![
            session_id,
            customer,
            at.to_rfc3339(),
            folder,
            branch,
            source
        ],
    )
    .context("inserting session pin")?;

    Ok(SessionPin {
        session_id: session_id.to_owned(),
        customer,
        from_at: at,
        folder,
        branch,
        source: source.to_owned(),
    })
}

/// The latest pin for `branch` in `folder`, or `None` on a default
/// branch (a pin is never inherited there) or when nothing is pinned.
pub fn pin_for_branch(conn: &Connection, folder: &str, branch: &str) -> Result<Option<SessionPin>> {
    if is_default_branch(branch) {
        return Ok(None);
    }
    let row = conn
        .query_row(
            "SELECT session_id, customer, from_at, folder, branch, source
               FROM session_pins
              WHERE folder = ?1 AND branch = ?2
              ORDER BY from_at DESC
              LIMIT 1",
            params![folder, branch],
            row_to_raw,
        )
        .optional()
        .context("loading branch pin")?;
    row.map(raw_to_pin).transpose()
}

/// Every pin row for the given session ids, ordered by session then time.
pub fn pins_for_sessions(conn: &Connection, session_ids: &[String]) -> Result<Vec<SessionPin>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; session_ids.len()].join(",");
    let sql = format!(
        "SELECT session_id, customer, from_at, folder, branch, source
           FROM session_pins
          WHERE session_id IN ({placeholders})
          ORDER BY session_id, from_at"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(session_ids.iter()), row_to_raw)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("reading session pins")?;
    rows.into_iter().map(raw_to_pin).collect()
}

type RawPin = (String, String, String, String, Option<String>, String);

fn row_to_raw(row: &rusqlite::Row) -> rusqlite::Result<RawPin> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn raw_to_pin(raw: RawPin) -> Result<SessionPin> {
    let (session_id, customer, from_at, folder, branch, source) = raw;
    let from_at = DateTime::parse_from_rfc3339(&from_at)
        .with_context(|| format!("parsing session_pins.from_at {from_at}"))?
        .with_timezone(&Utc);
    Ok(SessionPin {
        session_id,
        customer,
        from_at,
        folder,
        branch,
        source,
    })
}

#[cfg(test)]
#[path = "session_pins_test.rs"]
mod tests;
