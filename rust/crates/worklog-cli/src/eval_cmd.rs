//! `worklog eval "<query>"` — prints `block_eval`'s report.

use std::io::Write;

use anyhow::Result;
use worklog_core::{db, paths::Paths};

use crate::{cli::human_dur, style};

pub fn cmd_eval<W: Write>(query: &str, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let report = worklog_core::block_eval::eval(&conn, query)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    let (Some(first), Some(last)) = (&report.first_day, &report.last_day) else {
        style::warn(
            out,
            &format!("no blocks about \"{query}\" ({} checked)", report.checked),
        )?;
        return Ok(());
    };
    writeln!(out)?;
    writeln!(
        out,
        "{}",
        console::style(format!(
            "\"{query}\" — {} across {} blocks, {first} → {last}",
            human_dur(report.total_seconds),
            report.matched.len()
        ))
        .bold()
    )?;
    writeln!(out)?;
    let mut table = style::table();
    table.set_header(vec!["day", "time", "ticket", "description"]);
    for b in &report.matched {
        table.add_row(vec![
            b.day.clone(),
            human_dur(b.duration_seconds),
            b.jira_issue.clone().unwrap_or_default(),
            b.description.clone().unwrap_or_default(),
        ]);
    }
    writeln!(out, "{table}")?;
    Ok(())
}
