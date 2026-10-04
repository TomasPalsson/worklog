//! Account clue log: learn, suggest and record billing-account picks (spec 013).

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};

use crate::billing_registry::alias_matches;
use crate::jira_assist_contract::{
    AccountSuggestion, AllowedAccount, RelearnReport, CLUE_DROP_AFTER_WRONG, SUGGEST_LIMIT,
};

const STOPWORDS: &[&str] = &[
    "with", "from", "that", "this", "have", "into", "over", "work", "ticket",
];
const WORD_MIN_LEN: usize = 4;
const WORDS_PER_TICKET: usize = 5;

fn extract_clues(summary: &str) -> Vec<String> {
    let mut clues = Vec::new();
    if let Some((prefix, _)) = summary.split_once(" - ") {
        let prefix = prefix.trim().to_lowercase();
        if !prefix.is_empty() {
            clues.push(prefix);
        }
    }
    let lower = summary.to_lowercase();
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for word in lower.split(|ch: char| !ch.is_alphanumeric()) {
        if word.chars().count() < WORD_MIN_LEN || STOPWORDS.contains(&word) {
            continue;
        }
        match counts.iter_mut().find(|(w, _)| *w == word) {
            Some((_, n)) => *n += 1,
            None => counts.push((word, 1)),
        }
    }
    counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (word, _) in counts.into_iter().take(WORDS_PER_TICKET) {
        if !clues.iter().any(|c| c == word) {
            clues.push(word.to_string());
        }
    }
    clues
}

const BUMP_HITS: &str = "INSERT INTO account_clues (account_id, account_name, clue, hits)
     VALUES (?1, ?2, ?3, 1)
     ON CONFLICT(account_id, clue) DO UPDATE SET hits = hits + 1, account_name = excluded.account_name";
const BUMP_TICKETS: &str = "INSERT INTO account_ticket_counts (account_id, account_name, tickets)
     VALUES (?1, ?2, 1)
     ON CONFLICT(account_id) DO UPDATE SET tickets = tickets + 1, account_name = excluded.account_name";

pub fn relearn(conn: &Connection, tickets: &[(String, AllowedAccount)]) -> Result<RelearnReport> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE account_clues SET hits = 0", [])?;
    tx.execute("DELETE FROM account_ticket_counts", [])?;
    for (summary, account) in tickets {
        tx.execute(BUMP_TICKETS, params![account.id, account.name])?;
        for clue in extract_clues(summary) {
            tx.execute(BUMP_HITS, params![account.id, account.name, clue])?;
        }
    }
    tx.execute("DELETE FROM account_clues WHERE hits = 0", [])?;
    let accounts = tx.query_row("SELECT COUNT(*) FROM account_ticket_counts", [], |r| {
        r.get::<_, i64>(0)
    })? as usize;
    let clues = tx.query_row("SELECT COUNT(*) FROM account_clues", [], |r| {
        r.get::<_, i64>(0)
    })? as usize;
    tx.commit()?;
    Ok(RelearnReport {
        tickets_read: tickets.len(),
        accounts,
        clues,
    })
}

pub fn suggest(
    conn: &Connection,
    text: &str,
    allowed: &[AllowedAccount],
) -> Result<Vec<AccountSuggestion>> {
    let mut stmt = conn.prepare(
        "SELECT clue, hits FROM account_clues
         WHERE account_id = ?1 AND wrong < ?2 ORDER BY hits DESC, clue",
    )?;
    let mut out = Vec::new();
    for account in allowed {
        let rows = stmt
            .query_map(params![account.id, CLUE_DROP_AFTER_WRONG], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let matched: Vec<(String, i64)> = rows
            .into_iter()
            .filter(|(clue, _)| alias_matches(text, clue))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let past_tickets = conn
            .query_row(
                "SELECT tickets FROM account_ticket_counts WHERE account_id = ?1",
                [&account.id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        out.push(AccountSuggestion {
            account: account.clone(),
            score: matched.iter().map(|(_, hits)| *hits as f64).sum(),
            matched_clues: matched.into_iter().map(|(clue, _)| clue).collect(),
            past_tickets,
        });
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.truncate(SUGGEST_LIMIT);
    Ok(out)
}

pub fn record_decision(
    conn: &Connection,
    summary: &str,
    picked: &AllowedAccount,
    guessed_id: Option<&str>,
    clues: &[String],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let correct = guessed_id == Some(picked.id.as_str());
    tx.execute(
        "INSERT INTO account_decisions (decided_at, summary, picked_id, guessed_id, correct, clues)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            summary,
            picked.id,
            guessed_id,
            correct,
            serde_json::to_string(clues)?
        ],
    )?;
    tx.execute(BUMP_TICKETS, params![picked.id, picked.name])?;
    for clue in clues {
        tx.execute(BUMP_HITS, params![picked.id, picked.name, clue])?;
        if let Some(wrong_id) = guessed_id.filter(|_| !correct) {
            tx.execute(
                "UPDATE account_clues SET wrong = wrong + 1 WHERE account_id = ?1 AND clue = ?2",
                params![wrong_id, clue],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[path = "account_clues_test.rs"]
mod tests;
