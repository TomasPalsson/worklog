//! Row-level plumbing for `routing`: reading the `events` columns routing
//! cares about and turning them into a `RoutedEvent`. Split out of
//! routing.rs so that file stays at the contract-facing layer.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::routing_contract::{LabelOrigin, RoutedEvent, SOURCE_FIREFOX, SOURCE_SLACK};

pub(crate) const EVENT_COLUMNS: &str =
    "id, source, started_at, title, details, container, project_path, label_origin, label_confidence";

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
    }
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
