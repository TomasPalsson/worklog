//! Day storage for Mirres line billing (`mirres_line_billing`). Child of
//! `mirres.rs`; callers use `mirres::store_day` / `mirres::stored_billing`.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::tempo_line_contract::{BillingClass, LineBilling, MirresDay, TempoLineKey};
use crate::tempo_lines;

fn class_str(c: BillingClass) -> &'static str {
    match c {
        BillingClass::Billable => "billable",
        BillingClass::Included => "included",
        BillingClass::NotBillable => "not_billable",
    }
}

/// Replaces the day's rows in one transaction.
pub fn store_day(conn: &Connection, day: &str, rows: &[(String, LineBilling)]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM mirres_line_billing WHERE day = ?1", [day])?;
    let pulled_at = Utc::now().to_rfc3339();
    for (issue, b) in rows {
        tx.execute(
            "INSERT OR REPLACE INTO mirres_line_billing
               (day, jira_issue, account_key, project, project_type, class, warning, pulled_at, customer, details_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                day,
                issue,
                b.account_key,
                b.project,
                b.project_type,
                class_str(b.class),
                b.warning,
                pulled_at,
                b.customer,
                b.details
                    .as_ref()
                    .and_then(|d| serde_json::to_string(d).ok())
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn stored_billing(conn: &Connection, key: &TempoLineKey) -> Result<Option<LineBilling>> {
    conn.query_row(
        "SELECT account_key, project, project_type, class, warning, customer, details_json
           FROM mirres_line_billing WHERE day = ?1 AND jira_issue = ?2",
        params![key.day, key.jira_issue],
        |r| {
            let class: String = r.get(3)?;
            Ok(LineBilling {
                account_key: r.get(0)?,
                project: r.get(1)?,
                project_type: r.get(2)?,
                class: match class.as_str() {
                    "billable" => BillingClass::Billable,
                    "included" => BillingClass::Included,
                    _ => BillingClass::NotBillable,
                },
                warning: r.get(4)?,
                customer: r.get(5)?,
                details: r
                    .get::<_, Option<String>>(6)?
                    .and_then(|j| serde_json::from_str(&j).ok()),
            })
        },
    )
    .optional()
    .context("stored_billing")
}

/// One entry per day with stored Mirres rows, newest day first. `lines`
/// holds ALL of the day's lines, so unmatched ones show `billing: None`.
pub fn overview(conn: &Connection) -> Result<Vec<MirresDay>> {
    let mut stmt = conn.prepare(
        "SELECT day, MAX(pulled_at) FROM mirres_line_billing GROUP BY day ORDER BY day DESC",
    )?;
    let days = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    days.into_iter()
        .map(|(day, fetched_at)| {
            let lines = tempo_lines::lines_for_day(conn, &day)?;
            Ok(MirresDay {
                day,
                fetched_at,
                lines,
            })
        })
        .collect()
}
