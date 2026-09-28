//! Session customer pins — "session S is for customer C from time T on"
//! (spec 008, design.md §1). Stored in `session_pins`; a pin is set by
//! the `worklog pin` command (`source = "claude"`) or copied into a new
//! session on the same branch at start (`source = "inherited"`).

use std::path::Path;

use anyhow::{Context, Result};
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::billing;
use crate::billing_registry::Registry;
use crate::git;

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

/// How close a session's earliest pin must be to that session's first
/// event for [`pin_covering`]'s setup-race reach-back to cover events
/// recorded before it. Real sessions record a SessionStart hook row (and
/// often a first prompt) before Claude gets around to running `worklog
/// pin` — or before the session-hint's inherited pin lands — so without
/// this grace window those setup-minute events are never covered by any
/// pin and FR-07's Pinned origin can't fire on real data.
pub const SETUP_GRACE: Duration = Duration::minutes(10);

/// The pin covering `session_id`'s event at `at`, or `None`.
///
/// The covering pin is the latest of that session's pins with `from_at
/// <= at`. When no pin qualifies (every pin for the session starts
/// after `at`), the session's EARLIEST pin still covers it as a
/// setup-race exception — but only when that earliest pin's `from_at` is
/// within [`SETUP_GRACE`] of `session_start` (the session's first event
/// timestamp, as known to the caller). Otherwise `None`: the caller falls
/// back to its own uncovered-event handling (a text guess, or "not
/// pinned").
pub fn pin_covering<'a>(
    pins: &'a [SessionPin],
    session_id: &str,
    at: DateTime<Utc>,
    session_start: DateTime<Utc>,
) -> Option<&'a SessionPin> {
    let mut latest: Option<&SessionPin> = None;
    let mut earliest: Option<&SessionPin> = None;
    for pin in pins.iter().filter(|p| p.session_id == session_id) {
        if pin.from_at <= at && latest.is_none_or(|l| pin.from_at > l.from_at) {
            latest = Some(pin);
        }
        if earliest.is_none_or(|e| pin.from_at < e.from_at) {
            earliest = Some(pin);
        }
    }
    if latest.is_some() {
        return latest;
    }
    let earliest = earliest?;
    (earliest.from_at - session_start <= SETUP_GRACE).then_some(earliest)
}

/// The `worklog session-hint` start-of-session text (design.md §4,
/// contract T004), or `None` when nothing should be printed.
///
/// Gated on `cwd` actually living under `~/Desktop/Work` (FR-01, §2.2) —
/// `billing::billable_work_folder` is the strict form of the lookup;
/// unlike `work_folder_for_path` it refuses to fall back to a bare
/// basename for paths outside the work prefix, so a folder name outside
/// `~/Desktop/Work` can never coincidentally match a registered
/// multi-tenant folder — and on top of the multi_tenant check.
pub fn start_text(
    conn: &Connection,
    registry: &Registry,
    session_id: &str,
    cwd: &Path,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    let Some(folder) = billing::billable_work_folder(&cwd.to_string_lossy()) else {
        return Ok(None);
    };
    let multi_tenant = registry
        .folders
        .iter()
        .any(|f| f.folder == folder && f.multi_tenant);
    if !multi_tenant {
        return Ok(None);
    }

    if let Some(branch) = git::current_branch(cwd) {
        if let Some(branch_pin) = pin_for_branch(conn, &folder, &branch)? {
            let inherited = store_pin(
                conn,
                session_id,
                &branch_pin.customer,
                now,
                &folder,
                Some(&branch),
                "inherited",
            )?;
            return Ok(Some(format!(
                "This session is for {} (pinned from branch {branch}). If you switch \
                 customer, run: worklog pin <name> --session {session_id}",
                inherited.customer
            )));
        }
    }

    let names = registry
        .customers
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    let render = |shown: &[&str]| -> String {
        let more = names.len() - shown.len();
        let list = shown.join(", ");
        let list = match (list.is_empty(), more) {
            (_, 0) => list,
            (true, more) => format!("… (+{more} more; a wrong name lists them all)"),
            (false, more) => format!("{list}, … (+{more} more; a wrong name lists them all)"),
        };
        format!(
            "Shared repo — work out this session's customer from the repo, the prompt, and \
             the files you touch. When it's clear, pin it without asking: worklog pin <name> \
             --session {session_id}. When it's unclear and the Owner is present, ask once. \
             When nobody is present (a background or unattended run), pin nothing. Known \
             customers: {list}."
        )
    };
    // ponytail: shrink-by-one is O(n^2) in customer count, fine for the
    // tens of customers a real registry holds; revisit with a binary
    // search if that ever grows into the hundreds.
    let mut shown = names.len();
    let mut text = render(&names[..shown]);
    while text.chars().count() > 600 && shown > 0 {
        shown -= 1;
        text = render(&names[..shown]);
    }
    Ok(Some(text))
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

    Ok(store_pin(
        conn, session_id, &customer, at, &folder, branch, "claude",
    )?)
}

/// Shared INSERT behind [`pin`] (`source = "claude"`) and the branch
/// inheritance in [`start_text`] (`source = "inherited"`).
fn store_pin(
    conn: &Connection,
    session_id: &str,
    customer: &str,
    at: DateTime<Utc>,
    folder: &str,
    branch: Option<&str>,
    source: &str,
) -> Result<SessionPin> {
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
        customer: customer.to_owned(),
        from_at: at,
        folder: folder.to_owned(),
        branch: branch.map(str::to_owned),
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
