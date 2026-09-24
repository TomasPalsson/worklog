//! Persistence for `overlaps::Overlap.allocation` — the owner's manual
//! split of an overlap window, in the `overlap_allocations` table. Split
//! out of `overlaps.rs` (which owns detecting overlaps) so that module
//! stays under the size guard.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection};

/// (started_at, ended_at, shares) for one saved allocation row.
pub type AllocationRow = (DateTime<Utc>, DateTime<Utc>, BTreeMap<String, f64>);

pub fn load_allocations(conn: &Connection, day: NaiveDate) -> Result<Vec<AllocationRow>> {
    let mut stmt = conn.prepare(
        "SELECT started_at, ended_at, shares FROM overlap_allocations WHERE day = ?1
          ORDER BY started_at",
    )?;
    let rows = stmt.query_map(params![day.to_string()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (s, e, shares_json) = row?;
        let started_at = parse_ts(&s).with_context(|| format!("parsing started_at {s}"))?;
        let ended_at = parse_ts(&e).with_context(|| format!("parsing ended_at {e}"))?;
        let shares: BTreeMap<String, f64> =
            serde_json::from_str(&shares_json).context("parsing allocation shares JSON")?;
        out.push((started_at, ended_at, shares));
    }
    Ok(out)
}

pub fn save_allocation(
    conn: &Connection,
    day: NaiveDate,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    shares: &BTreeMap<String, f64>,
) -> Result<()> {
    let shares_json = serde_json::to_string(shares).context("serializing allocation shares")?;
    conn.execute(
        "INSERT INTO overlap_allocations (day, started_at, ended_at, shares)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(day, started_at, ended_at) DO UPDATE SET shares = excluded.shares",
        params![
            day.to_string(),
            started_at.to_rfc3339(),
            ended_at.to_rfc3339(),
            shares_json
        ],
    )
    .context("upserting overlap_allocations")?;
    Ok(())
}

pub fn delete_allocation(
    conn: &Connection,
    day: NaiveDate,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
) -> Result<()> {
    conn.execute(
        "DELETE FROM overlap_allocations WHERE day = ?1 AND started_at = ?2 AND ended_at = ?3",
        params![
            day.to_string(),
            started_at.to_rfc3339(),
            ended_at.to_rfc3339()
        ],
    )
    .context("deleting overlap_allocations row")?;
    Ok(())
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 23, h, m, 0).unwrap()
    }

    const A: &str = "/Users/dev/Desktop/Work/vitinn-infra";
    const B: &str = "/Users/dev/Desktop/Work/lyfjastofnun";

    #[test]
    fn save_load_and_delete_allocation_round_trip() {
        let conn = open_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap();
        let mut shares = BTreeMap::new();
        shares.insert(A.to_string(), 0.7);
        shares.insert(B.to_string(), 0.3);
        save_allocation(&conn, day, at(10, 0), at(11, 0), &shares).unwrap();

        let loaded = load_allocations(&conn, day).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].2, shares);

        // Upsert overwrites rather than duplicating.
        let mut shares2 = BTreeMap::new();
        shares2.insert(A.to_string(), 1.0);
        save_allocation(&conn, day, at(10, 0), at(11, 0), &shares2).unwrap();
        let loaded = load_allocations(&conn, day).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].2, shares2);

        delete_allocation(&conn, day, at(10, 0), at(11, 0)).unwrap();
        assert!(load_allocations(&conn, day).unwrap().is_empty());
    }
}
