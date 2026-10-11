//! Per-source collect health (spec 021 FR-16/FR-17): each collector run
//! leaves a success time or an error time + text in `meta`.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use crate::purge::{meta_get, meta_set};
use crate::time_leaks_contract::{collect_err_key, collect_ok_key, SourceHealth};

pub fn record(conn: &Connection, source: &str, outcome: Result<(), &str>) -> Result<()> {
    let stamp = Utc::now().to_rfc3339();
    match outcome {
        Ok(()) => meta_set(conn, &collect_ok_key(source), &stamp),
        Err(msg) => meta_set(conn, &collect_err_key(source), &format!("{stamp} {msg}")),
    }
}

/// Every source with a recorded run or an event in the last 24 h, by name.
pub fn all(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<SourceHealth>> {
    let cutoff = (now - Duration::hours(24)).to_rfc3339();
    let mut sources = BTreeSet::new();
    let mut stmt = conn.prepare("SELECT key FROM meta WHERE key LIKE 'collect.%'")?;
    for key in stmt.query_map([], |r| r.get::<_, String>(0))? {
        let key = key?;
        if let Some((source, "last_ok" | "last_err")) = key
            .strip_prefix("collect.")
            .and_then(|k| k.rsplit_once('.'))
        {
            sources.insert(source.to_string());
        }
    }
    let mut stmt = conn.prepare(
        "SELECT source, COUNT(*) FROM events
         WHERE datetime(started_at) >= datetime(?1) GROUP BY source",
    )?;
    let counts = stmt
        .query_map(params![cutoff], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("counting events")?;
    sources.extend(counts.iter().map(|(s, _)| s.clone()));
    sources
        .into_iter()
        .map(|source| {
            Ok(SourceHealth {
                last_ok: meta_get(conn, &collect_ok_key(&source))?,
                last_err: meta_get(conn, &collect_err_key(&source))?,
                events_24h: counts.iter().find(|(s, _)| *s == source).map_or(0, |c| c.1),
                source,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "collect_health_test.rs"]
mod tests;
