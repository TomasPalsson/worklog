//! Verdict's ticket pick for a block, and the log of the Owner's ticket
//! swaps (spec 017 FR-08, FR-16..FR-18).

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde_json::json;

use crate::billing::billable_work_folder;
use crate::block_service::is_synced;
use crate::estimate::load_open_tickets;
use crate::hook_run::jira_re;
use crate::models::{Block, Event};
use crate::repo;
use crate::routing::{decide, Pending};
use crate::routing_contract::{Classifier, RouteRule, RoutedEvent};
use crate::session_tickets;
use crate::tempo_line_contract::TicketOrigin;
use crate::verdict_contract::{
    DecisionKind, DecisionRow, DecisionSource, RECENT_TICKET_DAYS, SHORTLIST_MAX,
};
use crate::verdict_decisions;

/// Event titles sent as the block's description to Verdict.
const STATE_TITLES: usize = 10;
const STATE_TITLE_CHARS: usize = 120;

/// At most `SHORTLIST_MAX` tickets, in order: the ticket of the block's coding
/// session, open ticket keys written in its events, then tickets logged on
/// blocks of the same project root in the last `RECENT_TICKET_DAYS` days,
/// newest first.
pub fn ticket_options(conn: &Connection, block: &Block) -> Result<Vec<String>> {
    options_for(conn, block, &repo::list_events_for_block(conn, block.id)?)
}

fn options_for(conn: &Connection, block: &Block, events: &[Event]) -> Result<Vec<String>> {
    let day: NaiveDate = block.day.parse().context("block day")?;
    let open: Vec<String> = load_open_tickets(conn, day)?
        .into_iter()
        .map(|c| c.key)
        .collect();

    let mut session = Vec::new();
    for sid in events.iter().filter_map(|e| e.session_id.as_deref()) {
        session.extend(session_tickets::get(conn, sid)?);
    }
    let re = jira_re();
    let written = events
        .iter()
        .flat_map(|e| [Some(e.title.as_str()), e.details.as_deref()])
        .flatten()
        .flat_map(|text| re.find_iter(text).map(|m| m.as_str().to_owned()))
        .filter(|k| open.contains(k));
    let recent = recent_tickets(conn, block, day, events)?;

    let mut out: Vec<String> = Vec::new();
    for key in session.into_iter().chain(written).chain(recent) {
        if out.len() == SHORTLIST_MAX {
            break;
        }
        if !out.contains(&key) {
            out.push(key);
        }
    }
    Ok(out)
}

/// Tickets of other work blocks in the block's project root, newest first.
// ponytail: the root is the first event folder; a block spanning two roots is matched on one.
fn recent_tickets(
    conn: &Connection,
    block: &Block,
    day: NaiveDate,
    events: &[Event],
) -> Result<Vec<String>> {
    let Some(folder) = events
        .iter()
        .find_map(|e| e.project_path.as_deref().and_then(billable_work_folder))
    else {
        return Ok(Vec::new());
    };
    let first_day = day + Duration::days(1 - RECENT_TICKET_DAYS);
    let mut stmt = conn.prepare(
        "SELECT b.jira_issue, e.project_path
           FROM blocks b
           JOIN block_events be ON be.block_id = b.id
           JOIN events e ON e.id = be.event_id
          WHERE b.id != ?1 AND b.is_personal = 0
            AND b.jira_issue IS NOT NULL AND b.jira_issue != ''
            AND b.day >= ?2 AND b.day <= ?3 AND e.project_path IS NOT NULL
          ORDER BY b.started_at DESC, b.id DESC",
    )?;
    let rows = stmt
        .query_map(params![block.id, first_day.to_string(), block.day], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, path)| billable_work_folder(path).as_deref() == Some(folder.as_str()))
        .map(|(key, _)| key)
        .collect())
}

/// The ticket Verdict is clearly sure of for `block`, logged either way.
/// `None` when the Owner or the block's events set the ticket, there is
/// nothing to choose from, or Verdict is unsure or unreachable.
pub fn pick(
    conn: &Connection,
    classifier: &dyn Classifier,
    block: &Block,
    rule: RouteRule,
) -> Result<Option<String>> {
    if matches!(
        block.ticket_origin,
        Some(TicketOrigin::Manual | TicketOrigin::Event)
    ) {
        return Ok(None);
    }
    let events = repo::list_events_for_block(conn, block.id)?;
    let options = options_for(conn, block, &events)?;
    if options.is_empty() {
        return Ok(None);
    }
    let titles: Vec<String> = events
        .iter()
        .take(STATE_TITLES)
        .map(|e| e.title.chars().take(STATE_TITLE_CHARS).collect())
        .collect();
    let pending = Pending {
        event: RoutedEvent {
            id: block.id,
            source: "ticket".into(),
            started_at: block.started_at.clone(),
            title: block.description.clone().unwrap_or_default(),
            details: None,
            container: None,
            folder: None,
            label_origin: None,
            label_confidence: None,
        },
        options,
        state: json!({ "events": titles }),
        examples: BTreeMap::new(),
    };
    let Some(answer) = decide(&[pending], classifier, rule).pop() else {
        return Ok(None);
    };
    let chosen = answer.guess.map(|g| g.folder);
    // A re-run re-asks a block Verdict left unticketed; keep one row for it.
    conn.execute(
        "DELETE FROM verdict_decisions
          WHERE kind = 'ticket' AND source = 'verdict' AND subject = ?1 AND chosen IS NULL",
        params![block.id.to_string()],
    )
    .context("replacing earlier unpicked decision")?;
    verdict_decisions::record(
        conn,
        &DecisionRow {
            kind: DecisionKind::Ticket,
            source: DecisionSource::Verdict,
            subject: block.id.to_string(),
            state_json: answer.state.to_string(),
            options: answer.options,
            ranking: Some(answer.ranking),
            chosen: chosen.clone(),
            previous: None,
            decided_at: Utc::now().to_rfc3339(),
        },
    )?;
    Ok(chosen)
}

/// Run [`pick`] over the day's unsynced work blocks and set each clear pick
/// as an automatic ticket. Returns how many blocks changed.
pub fn apply(
    conn: &Connection,
    classifier: &dyn Classifier,
    day: NaiveDate,
    rule: RouteRule,
) -> Result<usize> {
    let mut changed = 0;
    for block in repo::list_blocks_for_day(conn, &day.to_string())? {
        if block.is_personal || is_synced(&block) {
            continue;
        }
        let Some(key) = pick(conn, classifier, &block, rule)? else {
            continue;
        };
        changed += conn
            .execute(
                "UPDATE blocks SET jira_issue = ?1, ticket_origin = 'auto'
                  WHERE id = ?2 AND (ticket_origin IS NULL OR ticket_origin = 'auto')
                    AND (jira_issue IS NOT ?1 OR ticket_origin IS NOT 'auto')",
                params![key, block.id],
            )
            .context("setting Verdict ticket")?;
    }
    Ok(changed)
}

/// Log an Owner ticket change next to the ticket it replaced (FR-08).
pub(crate) fn record_swap(
    conn: &Connection,
    block_id: i64,
    previous: Option<String>,
    chosen: Option<&str>,
) -> Result<()> {
    if previous.as_deref() == chosen {
        return Ok(()); // re-saving the same ticket is not a correction
    }
    verdict_decisions::record(
        conn,
        &DecisionRow {
            kind: DecisionKind::Ticket,
            source: DecisionSource::Owner,
            subject: block_id.to_string(),
            state_json: "{}".into(),
            options: Vec::new(),
            ranking: None,
            chosen: chosen.map(str::to_owned),
            previous,
            decided_at: Utc::now().to_rfc3339(),
        },
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "ticket_verdict_test.rs"]
mod tests;
