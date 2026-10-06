//! Read-only "when did I last touch X" search over saved blocks and the
//! Owner's Claude prompts (spec 018, FR-30..33).

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::block_digest;
use crate::digest_contract::BlockDigest;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub block_id: i64,
    pub day: String,
    pub started_at: String,
    pub ended_at: String,
    pub jira_issue: Option<String>,
}

#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Stopped {
    pub prompts: Vec<String>,
    pub files: Vec<String>,
}

const MAX_HITS: usize = 5;
const MAX_STOPPED_PROMPTS: usize = 3;
/// Newest blocks of a repo examined for "where did I stop".
const STOPPED_BLOCK_WINDOW: i64 = 20;

fn digest_of(conn: &Connection, block_id: i64) -> BlockDigest {
    if let Ok(Some(card)) = block_digest::digest_for_block(conn, block_id) {
        return card;
    }
    block_digest::build_digest(conn, block_id).unwrap_or_else(|err| {
        tracing::warn!(block_id, error = %err, "ask: no digest for block");
        BlockDigest::default()
    })
}

/// Replaces the block's index row with its current text and digest clues.
pub fn index_block(conn: &Connection, block_id: i64) -> Result<()> {
    let (description, ticket): (Option<String>, Option<String>) = conn.query_row(
        "SELECT description, jira_issue FROM blocks WHERE id = ?1",
        [block_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let card = digest_of(conn, block_id);
    let text = description
        .into_iter()
        .chain(ticket)
        .chain(card.prompts)
        .chain(card.files)
        .chain(card.branches)
        .chain(card.change_titles)
        .collect::<Vec<_>>()
        .join("\n");
    conn.execute("DELETE FROM ask_index WHERE block_id = ?1", [block_id])?;
    conn.execute(
        "INSERT INTO ask_index (block_id, text) VALUES (?1, ?2)",
        params![block_id, text],
    )?;
    Ok(())
}

/// Indexes blocks that have no row yet and drops rows of blocks that are gone.
pub fn sync(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM ask_index WHERE block_id NOT IN (SELECT id FROM blocks)",
        [],
    )?;
    let missing: Vec<i64> = conn
        .prepare(
            "SELECT id FROM blocks WHERE id NOT IN (SELECT block_id FROM ask_index) ORDER BY id",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in missing {
        index_block(conn, id)?;
    }
    Ok(())
}

/// Each word becomes a quoted FTS5 string, so quotes and operators are text.
/// `None` when the query holds no words.
pub fn to_match_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

pub fn search(conn: &Connection, query: &str) -> Result<Vec<Hit>> {
    let Some(q) = to_match_query(query) else {
        return Ok(Vec::new());
    };
    let mut st = conn.prepare(
        "SELECT b.id, b.day, b.started_at, b.ended_at, b.jira_issue
           FROM ask_index JOIN blocks b ON b.id = ask_index.block_id
          WHERE ask_index MATCH ?1
          ORDER BY b.started_at DESC, b.id DESC
          LIMIT ?2",
    )?;
    let hits = st
        .query_map(params![q, MAX_HITS as i64], |r| {
            Ok(Hit {
                block_id: r.get(0)?,
                day: r.get(1)?,
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
                jira_issue: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(hits)
}

/// The repo's last 3 prompts (newest first) and the files touched in the
/// blocks they came from.
pub fn where_stopped(conn: &Connection, repo: &str) -> Result<Stopped> {
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT b.id FROM blocks b
              WHERE EXISTS (SELECT 1 FROM block_events be JOIN events e ON e.id = be.event_id
                             WHERE be.block_id = b.id AND e.repo = ?1)
              ORDER BY b.started_at DESC, b.id DESC
              LIMIT ?2",
        )?
        .query_map(params![repo, STOPPED_BLOCK_WINDOW], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Stopped::default();
    for id in ids {
        if out.prompts.len() >= MAX_STOPPED_PROMPTS {
            break;
        }
        let card = digest_of(conn, id);
        let room = MAX_STOPPED_PROMPTS - out.prompts.len();
        out.prompts
            .extend(card.prompts.into_iter().rev().take(room));
        for f in card.files {
            if !out.files.contains(&f) {
                out.files.push(f);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "ask_test.rs"]
mod tests;
