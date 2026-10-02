//! Which tickets are still live: shared by the estimator's candidate list and the My Tasks board.

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use std::collections::HashSet;

/// A ticket with no Jira update and no hand/event-set block in this many days is stale.
pub const STALE_DAYS: i64 = 30;

/// A finished (status category done) ticket stays an AI candidate this long after its last Jira update.
pub const DONE_GRACE_DAYS: i64 = 7;

/// Statuses nobody bills against: Backlog, and cancelled-like closes.
pub fn is_dead_status(status: Option<&str>) -> bool {
    let Some(status) = status else { return false };
    let s = status.trim().to_lowercase();
    s == "backlog"
        || [
            "cancel",
            "won't",
            "wont",
            "reject",
            "declin",
            "duplicate",
            "abandon",
        ]
        .iter()
        .any(|needle| s.contains(needle))
}

/// Keys with activity in `[day - STALE_DAYS, ..]`: Jira `updated` date >= cutoff, OR a non-personal,
/// non-ignored block on the key with day in `[cutoff, day]` whose `ticket_origin` is manual/event.
/// AI picks (`auto`/NULL) don't count, or a wrong pick would keep a dead ticket alive forever.
pub fn active_keys(conn: &Connection, day: NaiveDate) -> Result<HashSet<String>> {
    // ponytail: no upper bound on the Jira side (updated after `day` still counts); `created` isn't cached.
    let cutoff = (day - Duration::days(STALE_DAYS)).to_string();
    let mut stmt = conn.prepare(
        "SELECT key FROM jira_tickets WHERE substr(COALESCE(updated,''),1,10) >= ?1
         UNION
         SELECT jira_issue FROM blocks
          WHERE jira_issue IS NOT NULL AND is_personal = 0 AND ignored_at IS NULL
            AND ticket_origin IN ('manual','event') AND day >= ?1 AND day <= ?2",
    )?;
    let keys = stmt
        .query_map([cutoff, day.to_string()], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(keys)
}

#[path = "ticket_activity_test.rs"]
#[cfg(test)]
mod tests;
