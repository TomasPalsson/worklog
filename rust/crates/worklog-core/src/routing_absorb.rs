//! Absorb + noise: the final routing step for a day, run right after
//! `routing::commit_labels` — after owner rules, named-project Link, Slack
//! time context (`routing_context.rs`) and the Verdict model have all had
//! their turn. Called explicitly from `worklog day`, `worklog collect all`
//! and the daemon's `POST /infer`, never embedded in `routing::route_day`
//! itself (existing callers of `route_day` expect a leftover event to stay
//! unlabelled, not become noise).
//!
//! Any firefox/slack event still unlabelled at that point either sits
//! inside a stretch of other-source work activity — absorbed into that
//! project, origin `Context`, exactly like `routing_context`'s Slack
//! time-context step — or becomes `Noise`: hidden the same way `Dismissed`
//! is, but reversible by an owner rule or a manual fix.

use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::billing_registry::Registry;
use crate::routing::{context, events_in_window, narrowed_options, project_keys, set_label};
use crate::routing_contract::LabelOrigin;

/// "Other-source work activity" sources for the absorb step — wider than
/// `routing_context`'s Slack-only three: a GitHub commit/PR also anchors a
/// stretch.
const WORK_SOURCES: [&str; 5] = [
    "claude",
    "shell",
    "git_reflog",
    "github_commit",
    "github_pr",
];
/// How far from the unlabelled event a work event may lie, on each side,
/// and still bracket it inside a stretch.
const STRETCH_MINUTES: i64 = 5;

/// The day's work-activity events as `(started_at, key)` pairs — same shape
/// as `routing_context::context_events_for_day`, but over the wider absorb
/// source list.
fn work_activity_for_day(
    conn: &Connection,
    day: NaiveDate,
) -> Result<Vec<(DateTime<Utc>, String)>> {
    let Some(home) = dirs::home_dir() else {
        return Ok(Vec::new());
    };
    let home = home.to_string_lossy().into_owned();
    let (start, end) = crate::tz::utc_window_for_local_day(day);
    let mut stmt = conn.prepare(
        "SELECT started_at, project_path FROM events
          WHERE source IN (?1, ?2, ?3, ?4, ?5) AND started_at >= ?6 AND started_at < ?7
            AND project_path IS NOT NULL",
    )?;
    let rows = stmt
        .query_map(
            params![
                WORK_SOURCES[0],
                WORK_SOURCES[1],
                WORK_SOURCES[2],
                WORK_SOURCES[3],
                WORK_SOURCES[4],
                start.to_rfc3339(),
                end.to_rfc3339(),
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .filter_map(|(started_at, project_path)| {
            let time = DateTime::parse_from_rfc3339(&started_at)
                .ok()?
                .with_timezone(&Utc);
            let key = context::context_key(&project_path, &home)?;
            Some((time, key))
        })
        .collect())
}

/// Whether `activity` has a point at-or-before `event_time` AND a point
/// at-or-after it, each within `STRETCH_MINUTES` — the event sits inside a
/// stretch of work rather than at its edge (activity on one side only).
fn brackets(event_time: DateTime<Utc>, activity: &[(DateTime<Utc>, String)]) -> bool {
    let window = Duration::minutes(STRETCH_MINUTES);
    let before = activity
        .iter()
        .any(|(t, _)| *t <= event_time && event_time - *t <= window);
    let after = activity
        .iter()
        .any(|(t, _)| *t >= event_time && *t - event_time <= window);
    before && after
}

/// Set `label_origin = 'noise'`, clearing `project_path`/`label_confidence` —
/// hidden exactly like `routing_dismiss::set_dismissed`, but reversible by an
/// owner rule/fix (`routing::apply_rule_to_existing` treats it as such).
fn set_noise(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE events SET project_path = NULL, label_origin = ?1, label_confidence = NULL WHERE id = ?2",
        params![LabelOrigin::Noise.as_str(), id],
    )?;
    Ok(())
}

/// Run once, at the end of a day's routing pass. Every firefox/slack event
/// still unlabelled either joins the project a bracketing work stretch
/// dominantly names (origin `Context`, confidence `NULL`) or becomes noise.
pub fn absorb_and_noise(conn: &Connection, day: NaiveDate) -> Result<()> {
    let activity = work_activity_for_day(conn, day)?;
    let registry = Registry::load(conn)?;
    let all_options = project_keys(conn)?;

    for row in events_in_window(conn, day, true)? {
        let options = narrowed_options(&registry, &row, &all_options);
        let key = DateTime::parse_from_rfc3339(&row.started_at)
            .ok()
            .map(|t| t.with_timezone(&Utc))
            .filter(|t| brackets(*t, &activity))
            .and_then(|t| context::dominant_key_within(t, &activity, STRETCH_MINUTES))
            .filter(|k| options.iter().any(|o| o == k));

        match key {
            Some(k) => set_label(conn, row.id, &k, LabelOrigin::Context, None)?,
            None => set_noise(conn, row.id)?,
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "routing_absorb_test.rs"]
mod tests;
