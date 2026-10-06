//! Which projects Verdict is offered for one loose event, and which past
//! fixes it is shown as examples of each (spec 017 FR-10..FR-12).

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::{params, Connection};

use crate::billing::billable_work_folder;
use crate::billing_registry::{alias_matches, Registry};
use crate::routing::{narrowed_options, project_keys, EventRow};
use crate::tz::utc_window_for_local_day;
use crate::verdict_contract::{
    EXAMPLES_MAX, EXAMPLE_CHARS_EACH, EXAMPLE_CHARS_TOTAL, RECENT_PROJECT_DAYS, SHORTLIST_MAX,
};
use crate::verdict_decisions::examples_for;

/// At most `SHORTLIST_MAX` projects, in order: the customer's pinned folders
/// when the container names one, folders named in the event's own text, then
/// folders with the most worked minutes in the 14 days ending on `day`.
pub(crate) fn shortlist(conn: &Connection, row: &EventRow, day: NaiveDate) -> Result<Vec<String>> {
    let registry = Registry::load(conn)?;
    let universe = narrowed_options(&registry, row, &project_keys(conn)?);
    let pinned = row
        .container
        .as_deref()
        .is_some_and(|c| registry.customer_in_text(c).is_some());
    let text = [
        Some(row.title.as_str()),
        row.details.as_deref(),
        row.container.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");

    let first = if pinned { universe.clone() } else { Vec::new() };
    let hits = universe.iter().filter(|f| alias_matches(&text, f)).cloned();
    let recents = recent_folders(conn, day)?
        .into_iter()
        .filter(|f| universe.contains(f));

    let mut out: Vec<String> = Vec::new();
    for folder in first.into_iter().chain(hits).chain(recents) {
        if out.len() == SHORTLIST_MAX {
            break;
        }
        if !out.contains(&folder) {
            out.push(folder);
        }
    }
    Ok(out)
}

/// Work folders by distinct minutes with an event, most first, ties by name.
fn recent_folders(conn: &Connection, day: NaiveDate) -> Result<Vec<String>> {
    let first_day = day + chrono::Duration::days(1 - RECENT_PROJECT_DAYS);
    let start = utc_window_for_local_day(first_day).0;
    let end = utc_window_for_local_day(day).1;
    let mut stmt = conn.prepare(
        "SELECT project_path, COUNT(DISTINCT substr(started_at, 1, 16)) FROM events
          WHERE project_path IS NOT NULL AND started_at >= ?1 AND started_at < ?2
          GROUP BY project_path",
    )?;
    let paths = stmt
        .query_map(params![start.to_rfc3339(), end.to_rfc3339()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut minutes: HashMap<String, i64> = HashMap::new();
    for (path, n) in paths {
        if let Some(folder) = billable_work_folder(&path) {
            *minutes.entry(folder).or_insert(0) += n;
        }
    }
    let mut ranked: Vec<(String, i64)> = minutes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(ranked.into_iter().map(|(f, _)| f).collect())
}

/// Past Owner fixes per option, each cut to `EXAMPLE_CHARS_EACH`; the first
/// example that would push the request past `EXAMPLE_CHARS_TOTAL` ends the list.
pub(crate) fn examples_for_options(
    conn: &Connection,
    options: &[String],
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut room = EXAMPLE_CHARS_TOTAL;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for option in options {
        for title in examples_for(conn, option, EXAMPLES_MAX)? {
            let cut: String = title.chars().take(EXAMPLE_CHARS_EACH).collect();
            let len = cut.chars().count();
            if len > room {
                return Ok(out);
            }
            room -= len;
            out.entry(option.clone()).or_default().push(cut);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "routing_shortlist_test.rs"]
mod tests;
