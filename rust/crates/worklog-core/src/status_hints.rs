//! Read-only status hints, e.g. a merged PR on an In Progress ticket (spec 013).

use crate::clues_contract::RawRecord;
use crate::jira_assist_contract::{is_writable_key, HintReason, StatusHint};
use crate::raw_json::decode_raw_json;
use crate::tempo_hub_contract::StatusCategory;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;

/// One hint per GENAI ticket that is not Done and has a merged PR naming it;
/// the latest merge is the reason. Reads the DB only.
pub fn done_hints(conn: &Connection) -> Result<Vec<StatusHint>> {
    let mut statement = conn.prepare(
        "SELECT t.key, t.summary, e.repo, e.title, e.raw_json
           FROM jira_tickets t
           JOIN events e ON e.jira_issue = t.key AND e.source = 'github_pr'
          WHERE t.status_category IS NOT 'done'",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            decode_raw_json(r, 4)?,
        ))
    })?;
    let mut latest: BTreeMap<String, StatusHint> = BTreeMap::new();
    for row in rows {
        let (key, summary, repo, title, raw) = row.context("reading pr events")?;
        if !is_writable_key(&key) {
            continue;
        }
        let Some(RawRecord::Commit {
            merged_at: Some(merged_at),
            ..
        }) = raw.and_then(|j| serde_json::from_str(&j).ok())
        else {
            continue;
        };
        let newer = match latest.get(&key).map(|h| &h.reason) {
            Some(HintReason::PrMerged { merged_at: old, .. }) => merged_at > *old,
            None => true,
        };
        if newer {
            let number = title
                .strip_prefix("PR #")
                .and_then(|s| s.split(':').next())
                .and_then(|n| n.parse().ok())
                .unwrap_or_default();
            let reason = HintReason::PrMerged {
                repo: repo.unwrap_or_default(),
                number,
                merged_at,
            };
            let to_category = StatusCategory::Done;
            latest.insert(
                key.clone(),
                StatusHint {
                    key,
                    summary,
                    to_category,
                    reason,
                },
            );
        }
    }
    Ok(latest.into_values().collect())
}

#[path = "status_hints_test.rs"]
#[cfg(test)]
mod tests;
