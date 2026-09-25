//! Hand-set per-block customer shares for multi-tenant infra folders (spec 005).

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::billing_registry::Registry;
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

#[cfg(test)]
#[path = "tenant_shares_test.rs"]
mod tests;
