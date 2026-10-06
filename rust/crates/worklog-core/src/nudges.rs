//! Footer nudges: review requests (cached), merged-not-done, stale (spec 018 FR-40).

use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::collectors::github::{self, GitHubAuth};
use crate::daily_helpers_contract::{Nudge, NudgeKind, NUDGE_CACHE_SECONDS, STALE_TICKET_DAYS};
use crate::purge::{meta_get, meta_set};
use crate::status_hints;

const REVIEWS_KEY: &str = "nudge_reviews";
const IN_PROGRESS: &str = "In Progress";

#[derive(Serialize, Deserialize)]
struct ReviewCache {
    fetched_at: DateTime<Utc>,
    items: Vec<Nudge>,
}

#[derive(Deserialize)]
struct ReviewSearch {
    items: Vec<ReviewItem>,
}

#[derive(Deserialize)]
struct ReviewItem {
    number: i64,
    title: String,
    html_url: String,
    repository_url: String,
}

/// A bad body means zero review nudges, never an error.
pub fn parse_review_search(body: &str) -> Vec<Nudge> {
    let search: ReviewSearch = match serde_json::from_str(body) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("worklog: review search unreadable: {e}");
            return Vec::new();
        }
    };
    search
        .items
        .into_iter()
        .map(|i| {
            let repo = i.repository_url.rsplit('/').take(2).collect::<Vec<_>>();
            Nudge {
                kind: NudgeKind::ReviewRequested,
                text: format!(
                    "Review requested: {}/{}#{} {}",
                    repo[1], repo[0], i.number, i.title
                ),
                url: Some(i.html_url),
            }
        })
        .collect()
}

pub fn store_reviews(conn: &Connection, now: DateTime<Utc>, items: &[Nudge]) -> Result<()> {
    let cache = ReviewCache {
        fetched_at: now,
        items: items.to_vec(),
    };
    meta_set(conn, REVIEWS_KEY, &serde_json::to_string(&cache)?)
}

fn fresh_reviews(conn: &Connection, now: DateTime<Utc>) -> Result<Option<Vec<Nudge>>> {
    let cache: Option<ReviewCache> =
        meta_get(conn, REVIEWS_KEY)?.and_then(|raw| serde_json::from_str(&raw).ok());
    Ok(cache
        .filter(|c| now - c.fetched_at <= Duration::seconds(NUDGE_CACHE_SECONDS))
        .map(|c| c.items))
}

pub fn reviews_stale(conn: &Connection, now: DateTime<Utc>) -> Result<bool> {
    Ok(fresh_reviews(conn, now)?.is_none())
}

/// Refetch review requests into the cache; a failure is logged and leaves it as it was.
pub fn refresh_reviews(conn: &Connection, now: DateTime<Utc>, client: &Client, auth: &GitHubAuth) {
    let result = github::review_requests(client, auth)
        .and_then(|body| store_reviews(conn, now, &parse_review_search(&body)));
    if let Err(e) = result {
        eprintln!("worklog: review nudges not refreshed: {e:#}");
    }
}

/// One refresh tick. `fetch` runs before `lock` is called, so the connection is
/// never held across the network or a Keychain read. Any failure stores an empty
/// cache stamped `now`, so nothing retries before the next tick.
pub fn refresh_once<C: std::ops::Deref<Target = Connection>>(
    now: DateTime<Utc>,
    fetch: impl FnOnce() -> Result<String>,
    lock: impl FnOnce() -> C,
) {
    let items = fetch()
        .map(|body| parse_review_search(&body))
        .unwrap_or_else(|e| {
            tracing::warn!("review nudges not refreshed: {e:#}");
            Vec::new()
        });
    if let Err(e) = store_reviews(&lock(), now, &items) {
        tracing::warn!("review nudge cache not stored: {e:#}");
    }
}

fn merged_not_done(conn: &Connection) -> Result<Vec<Nudge>> {
    let mut out = Vec::new();
    for hint in status_hints::done_hints(conn)? {
        let status: Option<String> = conn.query_row(
            "SELECT status FROM jira_tickets WHERE key = ?1",
            params![hint.key],
            |r| r.get(0),
        )?;
        if status.is_some_and(|s| s.eq_ignore_ascii_case(IN_PROGRESS)) {
            out.push(Nudge {
                kind: NudgeKind::MergedNotDone,
                text: format!("{} has a merged PR but is still In Progress", hint.key),
                url: None,
            });
        }
    }
    Ok(out)
}

fn latest_activity(conn: &Connection, key: &str) -> Result<Option<DateTime<Utc>>> {
    let mut stmt = conn.prepare(
        "SELECT started_at FROM events WHERE jira_issue = ?1
         UNION ALL SELECT ended_at FROM blocks WHERE jira_issue = ?1",
    )?;
    let stamps = stmt.query_map([key], |r| r.get::<_, String>(0))?;
    let mut latest = None;
    for stamp in stamps {
        if let Ok(t) = DateTime::parse_from_rfc3339(&stamp?) {
            latest = latest.max(Some(t.with_timezone(&Utc)));
        }
    }
    Ok(latest)
}

fn stale(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<Nudge>> {
    let mut stmt =
        conn.prepare("SELECT key FROM jira_tickets WHERE status = ?1 COLLATE NOCASE ORDER BY key")?;
    let keys = stmt
        .query_map([IN_PROGRESS], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for key in keys {
        let Some(last) = latest_activity(conn, &key)? else {
            continue;
        };
        if now - last > Duration::days(STALE_TICKET_DAYS) {
            out.push(Nudge {
                kind: NudgeKind::Stale,
                text: format!(
                    "{key} has had no activity since {}",
                    &last.to_rfc3339_opts(SecondsFormat::Secs, true)[..10]
                ),
                url: None,
            });
        }
    }
    Ok(out)
}

/// Every nudge that applies now: review requests, merged-not-done, stale.
pub fn current(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<Nudge>> {
    let mut out = fresh_reviews(conn, now)?.unwrap_or_default();
    out.extend(merged_not_done(conn)?);
    out.extend(stale(conn, now)?);
    Ok(out)
}

#[path = "nudges_test.rs"]
#[cfg(test)]
mod tests;
