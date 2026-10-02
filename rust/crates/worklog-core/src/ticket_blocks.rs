//! "Work logged" on a ticket (blocks in the My Tasks panel): the ticket's
//! blocks per local day over a window, with the day's Tempo line.

use crate::repo;
use crate::tempo_hub_contract::{TicketBlocks, TicketDay};
use crate::tempo_lines;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;

pub fn ticket_blocks(
    conn: &Connection,
    key: &str,
    today: NaiveDate,
    days: u32,
) -> Result<TicketBlocks> {
    let mut out = Vec::new();
    for offset in 0..days {
        let day = (today - Duration::days(offset.into())).to_string();
        let blocks: Vec<_> = repo::list_blocks_for_day(conn, &day)?
            .into_iter()
            .filter(|b| {
                b.jira_issue.as_deref() == Some(key) && !b.is_personal && b.ignored_at.is_none()
            })
            .collect();
        if blocks.is_empty() {
            continue;
        }
        let line = tempo_lines::lines_for_day(conn, &day)?
            .into_iter()
            .find(|line| line.jira_issue == key);
        let (line_seconds, line_text) = line.map_or((0, String::new()), |line| {
            let text = line
                .text
                .filter(|t| !t.is_empty())
                .unwrap_or(line.fallback_text);
            (line.effective_seconds, text)
        });
        out.push(TicketDay {
            day,
            line_seconds,
            line_text,
            blocks,
        });
    }
    Ok(TicketBlocks {
        key: key.to_string(),
        from: (today - Duration::days(days.saturating_sub(1).into())).to_string(),
        to: today.to_string(),
        days: out,
    })
}

#[path = "ticket_blocks_test.rs"]
#[cfg(test)]
mod tests;
