use crate::collectors::jira::JiraAuth;
use crate::collectors::jira_time::{
    fetch_estimates_with, fetch_tempo_worklogs_with, fetch_worklogs_with,
};
use crate::collectors::tempo::TempoAuth;
use crate::estimate_progress_contract::{DayProgress, ProgressError};
use crate::ticket_progress;
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::blocking::Client;
use rusqlite::Connection;
use std::collections::HashMap;
use tracing::warn;

/// Refetch the day's due tickets from Jira, then answer from the cache.
/// A failed fetch leaves that ticket's cached rows (and `pulled_at`) alone
/// and flags it. `refresh` forces that one ticket and no other.
#[allow(clippy::too_many_arguments)]
pub(crate) fn progress_with(
    conn: &Connection,
    day: NaiveDate,
    refresh: Option<&str>,
    auth: Option<&JiraAuth>,
    tempo: Option<&TempoAuth>,
    me: Option<&str>,
    client: &Client,
    now: DateTime<Utc>,
) -> Result<DayProgress> {
    let keys = ticket_progress::day_ticket_keys(conn, day)?;
    let due = match refresh {
        Some(k) => keys.iter().filter(|c| *c == k).cloned().collect(),
        None => ticket_progress::stale_keys(conn, &keys, now)?,
    };
    let mut errors: HashMap<String, ProgressError> = HashMap::new();
    let mut fail = |keys: &[String], e| errors.extend(keys.iter().map(|k| (k.clone(), e)));
    if !due.is_empty() {
        match auth {
            None => fail(&due, ProgressError::NotConfigured),
            Some(auth) => match fetch_estimates_with(auth, &due, client) {
                Err(e) => {
                    warn!("jira estimates failed: {e:#}");
                    fail(&due, ProgressError::JiraUnavailable);
                }
                Ok(estimates) => {
                    for key in &due {
                        let logs = match tempo {
                            Some(t) => fetch_tempo_worklogs_with(t, auth, key, client),
                            None => fetch_worklogs_with(auth, key, client),
                        };
                        match logs {
                            Ok(logs) => ticket_progress::store_ticket(
                                conn,
                                key,
                                estimates[key],
                                &logs,
                                now,
                            )?,
                            Err(e) => {
                                warn!("jira worklogs for {key} failed: {e:#}");
                                fail(std::slice::from_ref(key), ProgressError::JiraUnavailable);
                            }
                        }
                    }
                }
            },
        }
    }
    let mut tickets = ticket_progress::view(conn, &keys, me)?;
    for t in &mut tickets {
        t.error = errors.get(&t.key).copied();
    }
    Ok(DayProgress { day, tickets })
}

/// After a Tempo sync the Jira-side numbers have moved: refetch next time.
pub(crate) fn mark_day_stale(
    conn: &Connection,
    day: NaiveDate,
    only_issue: Option<&str>,
) -> Result<()> {
    let keys = match only_issue {
        Some(k) => vec![k.to_string()],
        None => ticket_progress::day_ticket_keys(conn, day)?,
    };
    ticket_progress::mark_stale(conn, &keys)
}

#[cfg(test)]
#[path = "daemon_progress_test.rs"]
mod tests;
