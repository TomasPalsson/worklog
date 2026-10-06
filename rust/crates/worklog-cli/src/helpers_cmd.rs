//! Terminal front for the spec 018 daily helpers: `undo`, `ask`, `report`.

use std::io::Write;

use anyhow::{bail, Result};
use worklog_core::{
    ask::{self, Hit, Stopped},
    daily_helpers_contract::UndoOutcome,
    db,
    paths::Paths,
    report::{monthly_report, MonthlyReport},
    tz, undo,
};

fn db_path() -> Result<std::path::PathBuf> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        bail!("db not initialized. Run `worklog db migrate` first.");
    }
    Ok(paths.db)
}

pub fn cmd_undo<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let mut conn = db::open(&db_path()?)?;
    show_undo(undo::undo_last(&mut conn)?, out, json)
}

pub fn cmd_ask<W: Write>(
    query: &[String],
    repo: Option<&str>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let conn = db::open(&db_path()?)?;
    match repo {
        Some(repo) => show_stopped(repo, &ask::where_stopped(&conn, repo)?, out, json),
        None => {
            ask::sync(&conn)?;
            show_hits(&ask::search(&conn, &query.join(" "))?, out, json)
        }
    }
}

pub fn cmd_report<W: Write>(customer: &str, month: &str, csv: bool, out: &mut W) -> Result<()> {
    let conn = db::open(&db_path()?)?;
    show_report(&monthly_report(&conn, customer, month)?, csv, out)
}

fn show_undo<W: Write>(outcome: UndoOutcome, out: &mut W, json: bool) -> Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&outcome)?)?;
    }
    match outcome {
        UndoOutcome::Restored { change, block_ids } => {
            if !json {
                let ids: Vec<String> = block_ids.iter().map(i64::to_string).collect();
                let name = serde_json::to_value(change)?;
                writeln!(
                    out,
                    "✓ undid {} (blocks {})",
                    name.as_str().unwrap_or_default(),
                    ids.join(", ")
                )?;
            }
            Ok(())
        }
        UndoOutcome::NothingToUndo => bail!("nothing left to undo"),
        UndoOutcome::RefusedSynced { block_id } => {
            bail!("block {block_id} was sent to Tempo; undo would desync it")
        }
    }
}

/// `HH:MM` in the `$WORKLOG_TZ` offset; the raw string when unparseable.
fn hhmm(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso).map_or_else(
        |_| iso.to_owned(),
        |d| {
            d.with_timezone(&tz::day_offset())
                .format("%H:%M")
                .to_string()
        },
    )
}

fn show_hits<W: Write>(hits: &[Hit], out: &mut W, json: bool) -> Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(hits)?)?;
        return Ok(());
    }
    if hits.is_empty() {
        writeln!(out, "no matches")?;
    }
    for h in hits {
        writeln!(
            out,
            "{}  {}–{}  {}  (block {})",
            h.day,
            hhmm(&h.started_at),
            hhmm(&h.ended_at),
            h.jira_issue.as_deref().unwrap_or("—"),
            h.block_id
        )?;
    }
    Ok(())
}

fn show_stopped<W: Write>(repo: &str, stopped: &Stopped, out: &mut W, json: bool) -> Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(stopped)?)?;
        return Ok(());
    }
    writeln!(out, "last prompts in {repo}:")?;
    for p in &stopped.prompts {
        writeln!(out, "  - {p}")?;
    }
    writeln!(out, "files touched:")?;
    for f in &stopped.files {
        writeln!(out, "  {f}")?;
    }
    Ok(())
}

fn show_report<W: Write>(report: &MonthlyReport, csv: bool, out: &mut W) -> Result<()> {
    let text = if csv {
        report.to_csv()
    } else {
        report.to_text()
    };
    writeln!(out, "{text}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use worklog_core::daily_helpers_contract::BlockChange;
    use worklog_core::report::ReportLine;

    fn run<F: FnOnce(&mut Vec<u8>) -> Result<()>>(f: F) -> (Result<()>, String) {
        let mut buf = Vec::new();
        let res = f(&mut buf);
        (res, String::from_utf8(buf).unwrap())
    }

    fn hit(id: i64, day: &str, hour: u32, ticket: Option<&str>) -> Hit {
        Hit {
            block_id: id,
            day: day.into(),
            started_at: format!("{day}T{hour:02}:00:00+00:00"),
            ended_at: format!("{day}T{hour:02}:30:00+00:00"),
            jira_issue: ticket.map(str::to_owned),
        }
    }

    #[test]
    fn undo_restored_names_change_and_blocks() {
        // catches printing success without the change kind or block ids
        let o = UndoOutcome::Restored {
            change: BlockChange::Merge,
            block_ids: vec![3, 4],
        };
        let (res, text) = run(|w| show_undo(o, w, false));
        res.unwrap();
        assert_eq!(text, "✓ undid merge (blocks 3, 4)\n");
    }

    #[test]
    fn undo_nothing_left_is_an_error() {
        // catches swallowing NothingToUndo as exit 0
        let (res, _) = run(|w| show_undo(UndoOutcome::NothingToUndo, w, false));
        assert_eq!(res.unwrap_err().to_string(), "nothing left to undo");
    }

    #[test]
    fn undo_refused_is_an_error_naming_the_block() {
        // catches swallowing RefusedSynced as exit 0, or dropping the block id
        let (res, _) = run(|w| show_undo(UndoOutcome::RefusedSynced { block_id: 7 }, w, false));
        assert_eq!(
            res.unwrap_err().to_string(),
            "block 7 was sent to Tempo; undo would desync it"
        );
    }

    #[test]
    fn undo_json_prints_the_outcome() {
        // catches --json being ignored
        let o = UndoOutcome::Restored {
            change: BlockChange::Delete,
            block_ids: vec![1],
        };
        let (res, text) = run(|w| show_undo(o, w, true));
        res.unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["outcome"], "restored");
        assert_eq!(v["block_ids"][0], 1);
    }

    #[test]
    fn undo_json_refusal_still_fails() {
        // catches --json turning a refusal into exit 0
        let (res, text) = run(|w| show_undo(UndoOutcome::RefusedSynced { block_id: 7 }, w, true));
        assert!(res.is_err());
        assert!(text.contains("refused_synced"), "{text}");
    }

    #[test]
    fn hits_print_date_block_time_and_ticket_in_given_order() {
        // catches dropping the ticket, date or block time; re-sorting the newest-first order
        std::env::set_var("WORKLOG_TZ", "UTC");
        let hits = [
            hit(2, "2026-04-18", 14, Some("NEW-2")),
            hit(1, "2026-04-17", 8, None),
        ];
        let (res, text) = run(|w| show_hits(&hits, w, false));
        res.unwrap();
        assert_eq!(
            text,
            "2026-04-18  14:00–14:30  NEW-2  (block 2)\n2026-04-17  08:00–08:30  —  (block 1)\n"
        );
    }

    #[test]
    fn no_hits_says_so_and_succeeds() {
        // catches an error or blank output on zero hits
        let (res, text) = run(|w| show_hits(&[], w, false));
        res.unwrap();
        assert_eq!(text, "no matches\n");
    }

    #[test]
    fn hits_json_is_the_hit_array() {
        // catches --json being ignored
        let (res, text) = run(|w| show_hits(&[hit(9, "2026-04-18", 9, Some("AAA-1"))], w, true));
        res.unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v[0]["block_id"], 9);
        assert_eq!(v[0]["jira_issue"], "AAA-1");
    }

    #[test]
    fn stopped_prints_prompts_and_files() {
        // catches dropping either list
        let s = Stopped {
            prompts: vec!["fix the parser".into()],
            files: vec!["src/parse.rs".into()],
        };
        let (res, text) = run(|w| show_stopped("worklog", &s, w, false));
        res.unwrap();
        assert_eq!(
            text,
            "last prompts in worklog:\n  - fix the parser\nfiles touched:\n  src/parse.rs\n"
        );
    }

    fn report() -> MonthlyReport {
        MonthlyReport {
            customer: "APRÓ".into(),
            month: "2026-07".into(),
            lines: vec![ReportLine {
                verkefni: Some("Vefsíður".into()),
                hours: 3.0,
                previous_hours: 2.0,
                texts: vec!["Síða eitt".into()],
            }],
        }
    }

    #[test]
    fn report_prints_text_by_default_and_csv_on_flag() {
        // catches --csv being ignored, or csv printed by default
        let (res, text) = run(|w| show_report(&report(), false, w));
        res.unwrap();
        assert_eq!(
            text,
            "APRÓ — 2026-07\nVefsíður: 3.0 h (+1.0)\n  - Síða eitt\n"
        );
        let (res, csv) = run(|w| show_report(&report(), true, w));
        res.unwrap();
        assert_eq!(
            csv,
            "verkefni,timar,breyting,texti\nVefsíður,3.0,+1.0,Síða eitt\n"
        );
    }

    #[test]
    fn verbs_parse_with_their_arguments() {
        // catches a missing variant, a lost --repo/--csv flag, or an unrequired query/month
        let parse = |a: &[&str]| {
            let mut argv = vec!["worklog"];
            argv.extend(a);
            crate::cli::Cli::try_parse_from(argv)
        };
        assert!(parse(&["undo"]).is_ok());
        assert!(parse(&["ask", "kafka", "lag"]).is_ok());
        assert!(parse(&["ask", "--repo", "worklog"]).is_ok());
        assert!(parse(&["ask"]).is_err(), "needs words or --repo");
        assert!(parse(&["ask", "--repo", "worklog", "kafka"]).is_err());
        assert!(parse(&["report", "APRÓ", "2026-07", "--csv"]).is_ok());
        assert!(parse(&["report", "APRÓ"]).is_err(), "month is required");
    }
}
