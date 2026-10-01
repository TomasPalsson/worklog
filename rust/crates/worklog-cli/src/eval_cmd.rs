//! `worklog eval "<query>"` — prints `block_eval`'s report.

use std::io::Write;

use anyhow::Result;
use worklog_core::{
    block_digest::{build_digest, digest_for_block},
    block_eval, db,
    digest_contract::BlockDigest,
    models::Block,
    paths::Paths,
};

use crate::{cli::human_dur, style};

pub fn cmd_eval<W: Write>(query: &str, out: &mut W, json: bool, details: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
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
}
