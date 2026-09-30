//! `worklog eval "<query>"`: ask the Verdict helper which work blocks are
//! about `<query>` and total their time. Unlike routing, an unreachable
//! helper is an error here — the user asked for this answer explicitly.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    billing,
    block_digest::{digest_for_block, eval_evidence},
    models::Block,
    repo,
    routing_contract::CLASSIFIER_ADDR,
};

#[derive(Debug, Serialize)]
pub struct EvalReport {
    pub query: String,
    /// Work blocks Verdict was asked about.
    pub checked: usize,
    /// Matching blocks, oldest first.
    pub matched: Vec<Block>,
    /// Union of the matching blocks' intervals — overlaps count once.
    pub total_seconds: i64,
    pub first_day: Option<String>,
    pub last_day: Option<String>,
}

#[derive(Deserialize)]
struct MatchResponse {
    matches: Vec<bool>,
}

/// Production entry point: talks to the helper on `CLASSIFIER_ADDR`.
pub fn eval(conn: &Connection, query: &str) -> Result<EvalReport> {
    let client = Client::builder()
        // One model call per block; a few hundred blocks take seconds,
        // a long history can take minutes.
        .timeout(Duration::from_secs(600))
        .build()?;
    eval_with(conn, query, &client, &format!("http://{CLASSIFIER_ADDR}"))
}

pub fn eval_with(
    conn: &Connection,
    query: &str,
    client: &Client,
    base_url: &str,
) -> Result<EvalReport> {
    let blocks = work_blocks(conn)?;
    let states = blocks
        .iter()
        .map(|b| block_state(conn, b))
        .collect::<Result<Vec<_>>>()?;
    let resp = client
        .post(format!("{base_url}/match"))
        .json(&json!({ "query": query, "states": states }))
        .send()
        .context("verdict not reachable — run `worklog verdict serve`")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("verdict helper is outdated — restart `worklog verdict serve`");
    }
    let matches = resp.error_for_status()?.json::<MatchResponse>()?.matches;
    if matches.len() != blocks.len() {
        bail!(
            "verdict answered {} of {} blocks",
            matches.len(),
            blocks.len()
        );
    }
    let matched: Vec<Block> = blocks
        .into_iter()
        .zip(matches)
        .filter_map(|(b, m)| m.then_some(b))
        .collect();
    Ok(EvalReport {
        query: query.to_owned(),
        checked: states.len(),
        total_seconds: billing::union_seconds(
            matched.iter().map(billing::block_interval).collect(),
        ),
        first_day: matched.first().map(|b| b.day.clone()),
        last_day: matched.last().map(|b| b.day.clone()),
        matched,
    })
}

/// Every non-personal, non-ignored block, oldest first.
fn work_blocks(conn: &Connection) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare("SELECT DISTINCT day FROM blocks ORDER BY day")?;
    let days = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for day in days {
        out.extend(
            repo::list_blocks_for_day(conn, &day)?
                .into_iter()
                .filter(|b| !b.is_personal && b.ignored_at.is_none()),
        );
    }
    Ok(out)
}

/// What Verdict reads for one block: its description and ticket plus the
/// repos and first few distinct titles of its events, so undescribed
/// blocks still carry evidence.
fn block_state(conn: &Connection, block: &Block) -> Result<Value> {
    let (repos, titles) = match digest_for_block(conn, block.id)? {
        Some(card) => (card.eval_repos, card.eval_titles),
        None => eval_evidence(&repo::list_events_for_block(conn, block.id)?),
    };
    // Arrays + nulls, not flat " | "-joined strings: hand-scored on 137
    // real blocks, flat strings caught one more right block but six
    // wrong ones (~2h45m of false time); this shape had none wrong.
    Ok(json!({
        "description": block.description,
        "jira_issue": block.jira_issue,
        "repos": repos,
        "titles": titles,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;
    use httpmock::prelude::*;

    fn seed(conn: &Connection, day: &str, start: &str, secs: i64, desc: &str) {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
             VALUES (?1, ?2, ?2, ?3, ?4)",
            rusqlite::params![day, start, secs, desc],
        )
        .unwrap();
    }

    #[test]
    fn totals_matching_blocks_once_and_reports_the_window() {
        let conn = open_memory().unwrap();
        seed(
            &conn,
            "2026-09-01",
            "2026-09-01T09:00:00Z",
            3600,
            "code interpreter",
        );
        // Overlaps the first by 30 min — counted once.
        seed(
            &conn,
            "2026-09-01",
            "2026-09-01T09:30:00Z",
            3600,
            "code interpreter egress",
        );
        seed(
            &conn,
            "2026-09-02",
            "2026-09-02T09:00:00Z",
            1800,
            "design system",
        );
        seed(
            &conn,
            "2026-09-03",
            "2026-09-03T09:00:00Z",
            900,
            "code interpreter image",
        );

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/match")
                .json_body_partial(r#"{"query": "code interpreter"}"#);
            then.status(200)
                .json_body(json!({ "matches": [true, true, false, true] }));
        });

        let report = eval_with(
            &conn,
            "code interpreter",
            &Client::new(),
            &server.base_url(),
        )
        .unwrap();
        mock.assert();
        assert_eq!(report.checked, 4);
        assert_eq!(report.matched.len(), 3);
        assert_eq!(report.total_seconds, 5400 + 900);
        assert_eq!(report.first_day.as_deref(), Some("2026-09-01"));
        assert_eq!(report.last_day.as_deref(), Some("2026-09-03"));
    }

    #[test]
    fn block_state_survive_compression_reads_the_card() {
        let conn = open_memory().unwrap();
        seed(&conn, "2026-01-01", "2026-01-01T09:00:00Z", 600, "x");
        let id = conn.last_insert_rowid();
        let card = crate::digest_contract::BlockDigest {
            eval_repos: vec!["aproorg/worklog".into()],
            eval_titles: vec!["Fix the export".into()],
            ..Default::default()
        };
        crate::block_digest::write_digest(&conn, id, &card).unwrap();
        let block = repo::get_block(&conn, id).unwrap().unwrap();

        let state = block_state(&conn, &block).unwrap();
        assert_eq!(state["repos"], json!(["aproorg/worklog"]));
        assert_eq!(state["titles"], json!(["Fix the export"]));
    }

    #[test]
    fn length_mismatch_is_an_error() {
        let conn = open_memory().unwrap();
        seed(&conn, "2026-09-01", "2026-09-01T09:00:00Z", 3600, "x");
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/match");
            then.status(200).json_body(json!({ "matches": [] }));
        });
        assert!(eval_with(&conn, "x", &Client::new(), &server.base_url()).is_err());
    }
}
