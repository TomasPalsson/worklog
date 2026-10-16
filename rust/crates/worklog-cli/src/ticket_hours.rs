//! Hour inputs for `worklog ticket`: `ticket hours` pins and `ticket create --estimate`.

use std::io::Write;

use anyhow::{Context, Result};
use serde_json::json;

use crate::ticket_cmd::dump;

/// `--estimate` hours ("1,5" or "1.5") → seconds, rounded to whole minutes
/// because Jira's Original Estimate is sent in minutes.
pub(crate) fn estimate_seconds(h: &str) -> Result<i64> {
    let n = h
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .with_context(|| format!("estimate must be a number, got {h:?}"))?;
    let minutes = (n * 60.0).round() as i64;
    if minutes < 1 {
        anyhow::bail!("estimate must be at least 1 minute, got {h:?}");
    }
    Ok(minutes * 60)
}

pub(crate) fn pin_hours<W: Write>(
    key: &str,
    hours: Option<String>,
    day: Option<String>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let day = match day {
        Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .with_context(|| format!("--day must be YYYY-MM-DD, got {d:?}"))?,
        None => chrono::Utc::now()
            .with_timezone(&worklog_core::tz::day_offset())
            .date_naive(),
    }
    .to_string();
    let hours = hours
        .map(|h| {
            h.replace(',', ".")
                .parse::<f64>()
                .with_context(|| format!("hours must be a number, got {h:?}"))
        })
        .transpose()?;
    let paths = worklog_core::paths::Paths::resolve()?;
    let conn = worklog_core::db::open(&paths.db)?;
    worklog_core::tempo_lines::pin_hours(
        &conn,
        &worklog_core::tempo_line_contract::SetTempoLineHours {
            day: day.clone(),
            jira_issue: key.to_string(),
            seconds: hours.map(|h| (h * 3600.0).round() as i64),
        },
    )?;
    if json {
        return dump(
            out,
            &json!({ "jira_issue": key, "day": day, "hours": hours }),
        );
    }
    match hours {
        Some(h) => writeln!(out, "{key} on {day}: {h}h (pinned)")?,
        None => writeln!(out, "{key} on {day}: pin cleared")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_hours_become_whole_minutes_and_bad_values_are_refused() {
        assert_eq!(estimate_seconds("1,5").unwrap(), 5400);
        assert_eq!(estimate_seconds("2").unwrap(), 7200);
        assert_eq!(
            estimate_seconds("1.999").unwrap(),
            7200,
            "rounds, never truncates"
        );
        for bad in ["0", "-1", "abc", "inf", "nan", "0.001"] {
            assert!(estimate_seconds(bad).is_err(), "{bad} must be refused");
        }
    }
}
