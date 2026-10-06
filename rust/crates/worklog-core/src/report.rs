//! Monthly customer report (spec 018, FR-34..38): one customer's hours for
//! a month, grouped by deild, with the change from the month before.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::Connection;

use crate::billing::{rows_for_day, BLANK};

#[derive(Debug, Clone, PartialEq)]
pub struct ReportLine {
    /// `None` when no deild resolved; never guessed.
    pub verkefni: Option<String>,
    pub hours: f64,
    pub previous_hours: f64,
    /// Each billed line's invoice text, in day order.
    pub texts: Vec<String>,
}

impl ReportLine {
    /// `None` when the previous month has no hours for this deild.
    pub fn change(&self) -> Option<f64> {
        (self.previous_hours > 0.0).then_some(self.hours - self.previous_hours)
    }

    fn change_display(&self) -> String {
        self.change()
            .map_or("n/a".to_string(), |c| format!("{c:+.1}"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonthlyReport {
    pub customer: String,
    pub month: String,
    pub lines: Vec<ReportLine>,
}

type Totals = BTreeMap<Option<String>, (f64, Vec<String>)>;

fn month_totals(conn: &Connection, customer: &str, first: NaiveDate) -> Result<Totals> {
    let next = NaiveDate::from_ymd_opt(
        first.year() + (first.month() / 12) as i32,
        first.month() % 12 + 1,
        1,
    )
    .context("month out of range")?;
    let mut totals = Totals::new();
    let mut day = first;
    while day < next {
        for row in rows_for_day(conn, &day.format("%Y-%m-%d").to_string())? {
            if row.customer.as_deref() == Some(customer) {
                let entry = totals.entry(row.verkefni).or_default();
                entry.0 += row.hours;
                entry.1.push(row.invoice_text);
            }
        }
        day += Duration::days(1);
    }
    Ok(totals)
}

/// `month` is `YYYY-MM`.
pub fn monthly_report(conn: &Connection, customer: &str, month: &str) -> Result<MonthlyReport> {
    let first = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .with_context(|| format!("month must be YYYY-MM, got {month:?}"))?;
    let previous = (first - Duration::days(1))
        .with_day(1)
        .context("previous month")?;
    let mut previous_totals = month_totals(conn, customer, previous)?;
    let mut lines: Vec<ReportLine> = month_totals(conn, customer, first)?
        .into_iter()
        .map(|(verkefni, (hours, texts))| ReportLine {
            previous_hours: previous_totals.remove(&verkefni).map_or(0.0, |p| p.0),
            verkefni,
            hours,
            texts,
        })
        .collect();
    lines.extend(
        previous_totals
            .into_iter()
            .map(|(verkefni, (previous_hours, _))| ReportLine {
                verkefni,
                hours: 0.0,
                previous_hours,
                texts: Vec::new(),
            }),
    );
    Ok(MonthlyReport {
        customer: customer.into(),
        month: month.into(),
        lines,
    })
}

impl MonthlyReport {
    pub fn to_text(&self) -> String {
        let mut out = vec![format!("{} — {}", self.customer, self.month)];
        for l in &self.lines {
            out.push(format!(
                "{}: {:.1} h ({})",
                l.verkefni.as_deref().unwrap_or(BLANK),
                l.hours,
                l.change_display()
            ));
            out.extend(l.texts.iter().map(|t| format!("  - {t}")));
        }
        out.join("\n")
    }

    /// Unresolved deild is an empty cell, not `—`.
    pub fn to_csv(&self) -> String {
        let mut out = vec!["verkefni,timar,breyting,texti".to_string()];
        for l in &self.lines {
            out.push(
                [
                    csv_cell(l.verkefni.as_deref().unwrap_or("")),
                    format!("{:.1}", l.hours),
                    l.change_display(),
                    csv_cell(&l.texts.join("; ")),
                ]
                .join(","),
            );
        }
        out.join("\n")
    }
}

/// Formula-injection guard, then RFC-4180 quoting (same rules as the billing export).
fn csv_cell(field: &str) -> String {
    let guarded = match field.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{field}"),
        _ => field.to_string(),
    };
    if guarded.contains(['"', ',', '\r', '\n']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

#[cfg(test)]
#[path = "report_test.rs"]
mod tests;
