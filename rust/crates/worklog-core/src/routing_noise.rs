//! Browser hosts the Owner always dismisses (GET /days/:day/noise-hosts).
//! Contract: `topics_contract::{NoiseHost, NoiseHosts, NOISE_MIN_DISMISSED}`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::routing;
use crate::routing_contract::SOURCE_FIREFOX;
use crate::topics_contract::{NoiseHost, NOISE_MIN_DISMISSED};

pub fn noise_hosts(conn: &Connection, day: &str) -> Result<Vec<NoiseHost>> {
    let mut stmt = conn
        .prepare(
            "SELECT details, label_origin = 'dismissed' FROM events
             WHERE source = ?1 AND substr(started_at, 1, 10) < ?2
               AND label_origin IN ('dismissed', 'fix', 'context', 'link', 'rule')",
        )
        .context("preparing noise-hosts query")?;
    let rows = stmt
        .query_map([SOURCE_FIREFOX, day], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, bool>(1)?))
        })
        .context("querying noise hosts")?;
    // host -> (rows seen, every row dismissed)
    let mut hosts: HashMap<String, (i64, bool)> = HashMap::new();
    for row in rows {
        let (details, dismissed) = row.context("reading noise-host row")?;
        let Some(host) = details.as_deref().and_then(routing::domain_of) else {
            continue;
        };
        let entry = hosts.entry(host).or_insert((0, true));
        entry.0 += 1;
        entry.1 &= dismissed;
    }
    let mut out: Vec<NoiseHost> = hosts
        .into_iter()
        .filter(|(_, (n, all))| *all && *n >= NOISE_MIN_DISMISSED)
        .map(|(host, (n, _))| NoiseHost {
            host,
            dismissed_before: n,
        })
        .collect();
    out.sort_by(|a, b| {
        b.dismissed_before
            .cmp(&a.dismissed_before)
            .then_with(|| a.host.cmp(&b.host))
    });
    Ok(out)
}

#[cfg(test)]
#[path = "routing_noise_test.rs"]
mod tests;
