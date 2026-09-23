//! Dismissing noise events (a random DM, a news site) so they never need
//! sorting and never count as work time. Split out of routing.rs to keep
//! that file at its line budget; shares its event-row/rule plumbing.

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::routing::{apply_rule_to_existing, fetch_event, rule_pattern, to_routed, upsert_rule};
use crate::routing_contract::{
    LabelOrigin, RoutedEvent, RuleKind, IGNORE_FOLDER, SOURCE_FIREFOX, SOURCE_SLACK,
};

/// Set `label_origin = 'dismissed'`, `project_path = NULL`, `label_confidence = NULL` for
/// one event. Shared by the manual dismiss endpoint and by rule application (an
/// `IGNORE_FOLDER` rule hit dismisses instead of filing).
pub(crate) fn set_dismissed(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE events SET project_path = NULL, label_origin = ?1, label_confidence = NULL WHERE id = ?2",
        params![LabelOrigin::Dismissed.as_str(), id],
    )?;
    Ok(())
}

/// Dismiss one event by hand; `rule_kind` also creates and retroactively applies an
/// `__ignore__` rule so future (and today's other unlabelled/guess/link/context) matches
/// are dismissed too — hand-`Fix`ed events are never touched, mirroring `label_event`.
pub fn dismiss_event(
    conn: &Connection,
    id: i64,
    rule_kind: Option<RuleKind>,
) -> Result<RoutedEvent> {
    let row = fetch_event(conn, id)?.ok_or_else(|| anyhow::anyhow!("event {id} not found"))?;
    if row.source != SOURCE_FIREFOX && row.source != SOURCE_SLACK {
        anyhow::bail!("event {id} is not a firefox/slack event");
    }
    if let Some(kind) = rule_kind {
        if !matches!(kind, RuleKind::Domain | RuleKind::SlackChannel) {
            anyhow::bail!("dismiss rule_kind must be domain or slack_channel");
        }
    }
    // Resolve the rule pattern before writing anything — a failure here must
    // leave the event untouched, not the dismissal committed with the rule missing.
    let rule = rule_kind
        .map(|kind| rule_pattern(&row, kind).map(|pattern| (kind, pattern)))
        .transpose()?;

    set_dismissed(conn, id)?;

    if let Some((kind, pattern)) = rule {
        upsert_rule(conn, kind, &pattern, IGNORE_FOLDER)?;
        apply_rule_to_existing(conn, kind, &pattern, id)?;
    }

    fetch_event(conn, id)?
        .map(to_routed)
        .ok_or_else(|| anyhow::anyhow!("event {id} vanished after dismissing"))
}

#[cfg(test)]
#[path = "routing_dismiss_test.rs"]
mod tests;
