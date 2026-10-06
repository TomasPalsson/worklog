//! Row-level plumbing for `routing`: reading the `events` columns routing
//! cares about and turning them into a `RoutedEvent`. Split out of
//! routing.rs so that file stays at the contract-facing layer.

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::billing_registry::Registry;
use crate::routing_contract::{
    Guess, LabelOrigin, RouteRule, RoutedEvent, SOURCE_FIREFOX, SOURCE_SLACK,
};
use crate::verdict_contract::{DecisionKind, DecisionRow, DecisionSource, Ranking};
use crate::verdict_decisions;

pub(crate) const EVENT_COLUMNS: &str =
    "id, source, started_at, title, details, container, project_path, label_origin, label_confidence, verdict_ranking";

/// A raw `events` row, as read for routing purposes.
pub(crate) struct EventRow {
    pub(crate) id: i64,
    pub(crate) source: String,
    pub(crate) started_at: String,
    pub(crate) title: String,
    pub(crate) details: Option<String>,
    pub(crate) container: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) label_origin: Option<String>,
    pub(crate) label_confidence: Option<f64>,
    pub(crate) verdict_ranking: Option<String>,
}

pub(crate) fn row_from(r: &rusqlite::Row) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        source: r.get(1)?,
        started_at: r.get(2)?,
        title: r.get(3)?,
        details: r.get(4)?,
        container: r.get(5)?,
        project_path: r.get(6)?,
        label_origin: r.get(7)?,
        label_confidence: r.get(8)?,
        verdict_ranking: r.get(9)?,
    })
}

pub(crate) fn to_routed(row: EventRow) -> RoutedEvent {
    RoutedEvent {
        id: row.id,
        source: row.source,
        started_at: row.started_at,
        title: row.title,
        details: row.details,
        container: row.container,
        folder: row
            .project_path
            .as_deref()
            .and_then(crate::billing::work_folder_for_path),
        label_origin: row.label_origin.as_deref().and_then(LabelOrigin::parse),
        label_confidence: row.label_confidence,
        ranking: row
            .verdict_ranking
            .and_then(|json| serde_json::from_str::<Ranking>(&json).ok())
            .map(|r| r.ranking),
    }
}

/// Verdict's answer for one pending event; `guess` is set only when it cleared the filing rule.
#[derive(Debug, Clone)]
pub struct Answer {
    pub id: i64,
    pub state: Value,
    pub options: Vec<String>,
    pub ranking: Ranking,
    pub guess: Option<Guess>,
}

/// FR-09a: the top choice is an offered option, the order check agreed, and the top score clears both ratios.
pub(crate) fn accept(ranking: &Ranking, options: &[String], rule: RouteRule) -> Option<Guess> {
    let top = ranking.ranking.first()?;
    let runner_up = ranking.ranking.get(1).map_or(0.0, |o| o.probability);
    let accepted = ranking.agreed
        && options.contains(&top.id)
        && top.probability >= ranking.abstain * rule.abstain_margin
        && top.probability >= runner_up * rule.runner_up_ratio;
    accepted.then(|| Guess {
        folder: top.id.clone(),
        confidence: top.probability,
        runner_up,
        abstain: ranking.abstain,
    })
}

fn log_decision(conn: &Connection, row: DecisionRow) -> Result<()> {
    verdict_decisions::record(conn, &row).map(|_| ())
}

/// Keep the ranking on the event and in the decision log, filed or not (FR-07).
pub(crate) fn record_answer(conn: &Connection, answer: &Answer) -> Result<()> {
    conn.execute(
        "UPDATE events SET verdict_ranking = ?1 WHERE id = ?2",
        params![serde_json::to_string(&answer.ranking)?, answer.id],
    )
    .context("storing event ranking")?;
    // A re-run re-asks an event Verdict left unfiled; keep one row for it, not one per run.
    conn.execute(
        "DELETE FROM verdict_decisions
          WHERE kind = 'project' AND source = 'verdict' AND subject = ?1 AND chosen IS NULL",
        params![answer.id.to_string()],
    )
    .context("replacing earlier unfiled decision")?;
    log_decision(
        conn,
        DecisionRow {
            kind: DecisionKind::Project,
            source: DecisionSource::Verdict,
            subject: answer.id.to_string(),
            state_json: answer.state.to_string(),
            options: answer.options.clone(),
            ranking: Some(answer.ranking.clone()),
            chosen: answer.guess.as_ref().map(|g| g.folder.clone()),
            previous: None,
            decided_at: Utc::now().to_rfc3339(),
        },
    )
}

/// Log an Owner correction next to the folder it replaced (FR-08).
pub(crate) fn record_fix(
    conn: &Connection,
    id: i64,
    previous: Option<String>,
    folder: &str,
) -> Result<()> {
    if previous.as_deref() == Some(folder) {
        return Ok(()); // re-confirming the same label is not a correction
    }
    log_decision(
        conn,
        DecisionRow {
            kind: DecisionKind::Project,
            source: DecisionSource::Owner,
            subject: id.to_string(),
            state_json: "{}".into(),
            options: Vec::new(),
            ranking: None,
            chosen: Some(folder.to_owned()),
            previous,
            decided_at: Utc::now().to_rfc3339(),
        },
    )
}

/// Whether `origin` marks an event hidden from the default routed/inferred
/// view — shared by `routing::routed_for_day` and anything that mirrors it.
pub(crate) fn is_hidden(origin: Option<LabelOrigin>) -> bool {
    matches!(origin, Some(LabelOrigin::Dismissed | LabelOrigin::Noise))
}

pub(crate) fn fetch_event(conn: &Connection, id: i64) -> Result<Option<EventRow>> {
    let sql = format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = ?1");
    conn.query_row(&sql, params![id], row_from)
        .optional()
        .context("fetching event")
}

/// Firefox/Slack events for the local day `day`; `unlabelled_only` narrows
/// to events still needing a label.
pub(crate) fn events_in_window(
    conn: &Connection,
    day: NaiveDate,
    unlabelled_only: bool,
) -> Result<Vec<EventRow>> {
    let (start, end) = crate::tz::utc_window_for_local_day(day);
    let extra = if unlabelled_only {
        " AND label_origin IS NULL"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {EVENT_COLUMNS} FROM events
          WHERE source IN (?1, ?2) AND started_at >= ?3 AND started_at < ?4{extra}
          ORDER BY started_at"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            SOURCE_FIREFOX,
            SOURCE_SLACK,
            start.to_rfc3339(),
            end.to_rfc3339()
        ],
        row_from,
    )?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

/// A container naming exactly one customer narrows to that customer's pinned folders (B10);
/// otherwise every project key is a candidate. Shared by `routing::load_pending` and
/// `routing_absorb`'s absorb step, both of which need an event's own option list.
pub(crate) fn narrowed_options(registry: &Registry, row: &EventRow, all: &[String]) -> Vec<String> {
    if let Some(container) = row.container.as_deref() {
        if let Some(customer) = registry.customer_in_text(container) {
            return registry
                .folders
                .iter()
                .filter(|f| f.customer.as_deref() == Some(customer.as_str()))
                .map(|f| f.folder.clone())
                .collect();
        }
    }
    all.to_vec()
}
