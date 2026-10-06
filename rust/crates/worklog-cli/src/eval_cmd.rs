//! `worklog eval "<query>"` — prints `block_eval`'s report; `--replay` prints the scorecard.

use std::io::Write;

use anyhow::Result;
use worklog_core::{
    block_digest::{build_digest, digest_for_block},
    block_eval, db,
    digest_contract::BlockDigest,
    models::Block,
    paths::Paths,
    scorecard::{self, Scorecard},
    verdict::VerdictClassifier,
};

use crate::{cli::human_dur, style};

pub fn cmd_eval<W: Write>(
    query: Option<&str>,
    out: &mut W,
    json: bool,
    details: bool,
) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let Some(query) = query else {
        // The helper owns a fixed port; the override lets tests run without it.
        let classifier = match std::env::var("WORKLOG_VERDICT_URL") {
            Ok(url) => VerdictClassifier::with_client(reqwest::blocking::Client::new(), url),
            Err(_) => VerdictClassifier::new(),
        };
        return write_scorecard(&scorecard::run(&conn, &classifier, false)?, out, json);
    };
    // The helper owns a fixed port; the override lets tests run without it.
    let report = match std::env::var("WORKLOG_VERDICT_URL") {
        Ok(url) => block_eval::eval_with(&conn, query, &reqwest::blocking::Client::new(), &url)?,
        Err(_) => block_eval::eval(&conn, query)?,
    };
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
    if details {
        let cards = report
            .matched
            .iter()
            .map(|b| match digest_for_block(&conn, b.id)? {
                Some(card) => Ok((b, card)),
                None => Ok((b, build_digest(&conn, b.id)?)),
            })
            .collect::<Result<Vec<_>>>()?;
        write_details(&cards, out)?;
    }
    Ok(())
}

fn write_scorecard<W: Write>(card: &Scorecard, out: &mut W, json: bool) -> Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(card)?)?;
    } else {
        writeln!(out, "{}", card.summary())?;
    }
    Ok(())
}

fn write_details<W: Write>(cards: &[(&Block, BlockDigest)], out: &mut W) -> Result<()> {
    for (b, card) in cards {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            console::style(format!("{}  {}", b.day, human_dur(b.duration_seconds))).bold()
        )?;
        let lists = [
            ("change_titles", &card.change_titles),
            ("prompts", &card.prompts),
            ("branches", &card.branches),
        ];
        for (name, items) in lists {
            if !items.is_empty() {
                writeln!(out, "  {name}: {}", items.join(" | "))?;
            }
        }
        if card.active_minutes > 0 {
            writeln!(out, "  active_minutes: {}", card.active_minutes)?;
        }
        if let Some(folder) = card.folder.as_deref().filter(|f| !f.is_empty()) {
            writeln!(out, "  folder: {folder}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use worklog_core::scorecard::{Scorecard, Tally};

    fn parse(args: &[&str]) -> Result<crate::cli::Cli, clap::Error> {
        crate::cli::Cli::try_parse_from(["worklog", "eval"].iter().chain(args))
    }

    fn card() -> Scorecard {
        Scorecard {
            project: Tally {
                right: 3,
                wrong: 1,
                unsure: 2,
            },
            ticket: Tally {
                right: 0,
                wrong: 0,
                unsure: 4,
            },
            skipped: 0,
            p95_ms: 12,
            tuned: Some((1.35, 1.1)),
            applied: false,
        }
    }

    fn block(day: &str, secs: i64) -> Block {
        Block {
            id: 1,
            day: day.into(),
            jira_issue: None,
            started_at: format!("{day}T09:00:00Z"),
            ended_at: format!("{day}T10:00:00Z"),
            duration_seconds: secs,
            description: None,
            estimated_by: None,
            flagged: false,
            tempo_worklog_id: None,
            is_personal: false,
            dirty: false,
            exported_at: None,
            ignored_at: None,
            ticket_origin: None,
        }
    }

    fn render(cards: &[(&Block, BlockDigest)]) -> String {
        let mut out = Vec::new();
        write_details(cards, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn eval_details_prints_cards() {
        let card = BlockDigest {
            change_titles: vec!["Sandbox the interpreter".into()],
            prompts: vec!["why does the sandbox leak".into()],
            branches: vec!["feat/sandbox".into()],
            active_minutes: 42,
            folder: Some("acme".into()),
            ..Default::default()
        };
        let text = render(&[(&block("2026-01-05", 3600), card)]);

        assert!(text.contains("2026-01-05"));
        assert!(text.contains("1h 00m"));
        assert!(text.contains("change_titles: Sandbox the interpreter"));
        assert!(text.contains("prompts: why does the sandbox leak"));
        assert!(text.contains("branches: feat/sandbox"));
        assert!(text.contains("active_minutes: 42"));
        assert!(text.contains("folder: acme"));
    }

    #[test]
    fn eval_details_skips_empty_fields() {
        let text = render(&[(&block("2026-01-05", 3600), BlockDigest::default())]);

        assert!(text.contains("2026-01-05"));
        assert!(!text.contains("prompts"));
        assert!(!text.contains("folder"));
        assert!(!text.contains("active"));
    }

    #[test]
    fn eval_replay_needs_no_query_and_a_query_needs_no_replay() {
        // catches: a required query that blocks `worklog eval --replay`
        assert!(parse(&["--replay"]).is_ok());
        // catches: dropping the query requirement for the plain eval
        assert!(parse(&[]).is_err());
        // catches: a replay that silently ignores a query
        assert!(parse(&["--replay", "login"]).is_err());
        assert!(parse(&["login"]).is_ok());
    }

    #[test]
    fn eval_replay_prints_each_count_and_the_thresholds() {
        let mut out = Vec::new();
        write_scorecard(&card(), &mut out, false).unwrap();
        let text = String::from_utf8(out).unwrap();
        // catches: swapped or dropped counts, and a missing tickets tally
        assert!(
            text.contains("project 3 right, 1 wrong, 2 unsure"),
            "{text}"
        );
        assert!(text.contains("ticket 0 right, 0 wrong, 4 unsure"), "{text}");
        assert!(text.contains("p95 12 ms"), "{text}");
        assert!(text.contains("best 1.35/1.10 (not saved)"), "{text}");
    }

    #[test]
    fn eval_replay_json_carries_the_tallies() {
        let mut out = Vec::new();
        write_scorecard(&card(), &mut out, true).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        // catches: printing the text line under --json
        assert_eq!(v["project"]["wrong"], 1);
        assert_eq!(v["ticket"]["unsure"], 4);
    }
}
