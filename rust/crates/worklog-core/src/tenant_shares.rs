//! Hand-set per-block customer shares for multi-tenant infra folders (spec 005).

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use tracing::warn;

use crate::billing_registry::Registry;
use crate::deild_contract::{BillingSlice, BlockShares, DeildOrigin, ShareRow, SHARE_TOLERANCE};
use crate::tenant_contract::{CustomerShares, CustomerSlice, SplitOrigin};

pub fn load_shares(
    conn: &Connection,
    day: &str,
    started_at: &str,
) -> Result<Option<CustomerShares>> {
    let mut stmt = conn
        .prepare("SELECT shares FROM block_customer_shares WHERE day = ?1 AND started_at = ?2")?;
    let mut rows = stmt.query(params![day, started_at])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let shares_json: String = row.get(0)?;
    let shares = serde_json::from_str(&shares_json).context("parsing customer shares JSON")?;
    Ok(Some(CustomerShares {
        day: day.to_string(),
        started_at: started_at.to_string(),
        shares,
    }))
}

pub fn save_shares(conn: &Connection, shares: &CustomerShares, registry: &Registry) -> Result<()> {
    let mut sum = 0.0;
    for (customer, fraction) in &shares.shares {
        if *fraction <= 0.0 {
            anyhow::bail!("Share must be above 0");
        }
        if !registry.customers.iter().any(|c| &c.name == customer) {
            anyhow::bail!("Customer no longer exists");
        }
        sum += fraction;
    }
    if (sum - 1.0).abs() > 0.001 {
        anyhow::bail!("Shares must add up to 100%");
    }

    let shares_json =
        serde_json::to_string(&shares.shares).context("serializing customer shares")?;
    conn.execute(
        "INSERT INTO block_customer_shares (day, started_at, shares)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(day, started_at) DO UPDATE SET shares = excluded.shares",
        params![shares.day, shares.started_at, shares_json],
    )
    .context("upserting block_customer_shares")?;
    Ok(())
}

pub fn clear_shares(conn: &Connection, day: &str, started_at: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM block_customer_shares WHERE day = ?1 AND started_at = ?2",
        params![day, started_at],
    )
    .context("deleting block_customer_shares row")?;
    Ok(())
}

/// Consecutive slices of `[start, end)`, one per customer in `shares` (name
/// order — `CustomerShares.shares` is a `BTreeMap`); the last slice absorbs
/// whatever second the earlier ones' rounding leaves over.
pub fn slices_from_shares(start: i64, end: i64, shares: &CustomerShares) -> Vec<CustomerSlice> {
    let total = end - start;
    let n = shares.shares.len();
    let mut out = Vec::with_capacity(n);
    let (mut from, mut cum) = (start, 0.0);
    for (i, (customer, fraction)) in shares.shares.iter().enumerate() {
        cum += fraction;
        let to = if i + 1 == n {
            end
        } else {
            (start + (cum * total as f64).round() as i64).clamp(from, end)
        };
        out.push(CustomerSlice {
            customer: Some(customer.clone()),
            intervals: vec![(from, to)],
            origin: SplitOrigin::Manual,
        });
        from = to;
    }
    out
}

// ───────── v2: (customer, deild, %) rows, stored in `rows_json` ─────────

/// Parses a `block_customer_shares` row: `rows_json` wins when present and
/// valid; otherwise the v1 `shares` map is converted, each entry landing
/// with `deild: None`. A malformed value is logged and treated as no rows,
/// per the trust-boundary table (design.md §2) — never a hard failure.
pub fn parse_rows(rows_json: Option<&str>, shares_json: &str) -> Vec<ShareRow> {
    if let Some(json) = rows_json {
        match serde_json::from_str::<Vec<ShareRow>>(json) {
            Ok(rows) => return rows,
            Err(e) => warn!("parsing block_customer_shares.rows_json failed: {e}"),
        }
    }
    match serde_json::from_str::<std::collections::BTreeMap<String, f64>>(shares_json) {
        Ok(shares) => shares
            .into_iter()
            .map(|(customer, fraction)| ShareRow {
                customer,
                deild: None,
                fraction,
            })
            .collect(),
        Err(e) => {
            warn!("parsing block_customer_shares.shares failed: {e}");
            Vec::new()
        }
    }
}

pub fn load_rows(conn: &Connection, day: &str, started_at: &str) -> Result<Option<BlockShares>> {
    let mut stmt = conn.prepare(
        "SELECT rows_json, shares FROM block_customer_shares WHERE day = ?1 AND started_at = ?2",
    )?;
    let mut result = stmt.query(params![day, started_at])?;
    let Some(row) = result.next()? else {
        return Ok(None);
    };
    let rows_json: Option<String> = row.get(0)?;
    let shares_json: String = row.get(1)?;
    Ok(Some(BlockShares {
        day: day.to_string(),
        started_at: started_at.to_string(),
        rows: parse_rows(rows_json.as_deref(), &shares_json),
    }))
}

pub fn validate_rows(rows: &[ShareRow], registry: &Registry) -> Result<()> {
    let mut sum = 0.0;
    for row in rows {
        if row.fraction <= 0.0 {
            anyhow::bail!("Share must be above 0");
        }
        if !registry.customers.iter().any(|c| c.name == row.customer) {
            anyhow::bail!("Customer no longer exists");
        }
        sum += row.fraction;
    }
    if (sum - 1.0).abs() > SHARE_TOLERANCE {
        anyhow::bail!("Shares must add up to 100%");
    }
    Ok(())
}

/// Writes `rows_json`; leaves v1 `shares` as `'{}'` (design.md §7 — v1 rows
/// are never rewritten, only read in preference order).
pub fn save_rows(conn: &Connection, s: &BlockShares, registry: &Registry) -> Result<()> {
    validate_rows(&s.rows, registry)?;
    let rows_json = serde_json::to_string(&s.rows).context("serializing share rows")?;
    conn.execute(
        "INSERT INTO block_customer_shares (day, started_at, shares, rows_json)
         VALUES (?1, ?2, '{}', ?3)
         ON CONFLICT(day, started_at) DO UPDATE SET shares = '{}', rows_json = excluded.rows_json",
        params![s.day, s.started_at, rows_json],
    )
    .context("upserting block_customer_shares rows_json")?;
    Ok(())
}

/// Consecutive slices of `[start, end)`, one per row, ordered by
/// `(customer, deild)`; the last absorbs whatever second the earlier ones'
/// rounding leaves over.
pub fn slices_from_rows(start: i64, end: i64, rows: &[ShareRow]) -> Vec<BillingSlice> {
    let mut ordered: Vec<&ShareRow> = rows.iter().collect();
    ordered.sort_by(|a, b| (&a.customer, &a.deild).cmp(&(&b.customer, &b.deild)));

    let total = end - start;
    let n = ordered.len();
    let mut out = Vec::with_capacity(n);
    let (mut from, mut cum) = (start, 0.0);
    for (i, row) in ordered.iter().enumerate() {
        cum += row.fraction;
        let to = if i + 1 == n {
            end
        } else {
            (start + (cum * total as f64).round() as i64).clamp(from, end)
        };
        out.push(BillingSlice {
            customer: Some(row.customer.clone()),
            deild: row.deild.clone(),
            intervals: vec![(from, to)],
            origin: SplitOrigin::Manual,
            deild_origin: DeildOrigin::Manual,
        });
        from = to;
    }
    out
}

#[cfg(test)]
#[path = "tenant_shares_test.rs"]
mod tests;
