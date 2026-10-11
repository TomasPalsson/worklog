//! GET /stats?from&to — the stats page's one payload. Child module of
//! `daemon.rs` (via `#[path]`) so it reuses its private `with_conn`/`ApiError`.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

const STATS_MAX_RANGE_DAYS: i64 = 366 * 30;
const TOPICS_HISTORY_DAYS: i64 = 60;

use crate::embed::{self, EmbedClient};
use crate::stats_contract::StatsReport;
use crate::stats_topics::{self, BlockPoint, TopicsInput};
use crate::topics_contract::{TopicsReport, TopicsStatus};
use crate::verdict_contract::VERDICT_ENABLED_KEY;
use crate::{stats, tz};

use super::{with_conn, ApiError, Shared};

#[derive(Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    to: Option<String>,
}

fn parse_day(raw: &str) -> Result<NaiveDate, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::bad_request(anyhow!("`{raw}` is not a date (YYYY-MM-DD)")))
}

/// `from` stays optional: each route picks its own default start.
fn resolve_range(q: &RangeQuery) -> Result<(Option<NaiveDate>, NaiveDate, NaiveDate), ApiError> {
    let from = q.from.as_deref().map(parse_day).transpose()?;
    let today = tz::local_date(Utc::now());
    let to = q.to.as_deref().map(parse_day).transpose()?.unwrap_or(today);
    if from.is_some_and(|f| f > to) {
        return Err(ApiError::bad_request(anyhow!("`from` is after `to`")));
    }
    // Daily rows are allocated up front while the shared connection is
    // locked, so bound the range (and the years, for tz window arithmetic).
    let in_years = |d: NaiveDate| (1970..=9999).contains(&d.year());
    if !in_years(to) || from.is_some_and(|f| !in_years(f)) {
        return Err(ApiError::bad_request(anyhow!(
            "dates must fall in 1970..=9999"
        )));
    }
    if from.is_some_and(|f| (to - f).num_days() + 1 > STATS_MAX_RANGE_DAYS) {
        return Err(ApiError::bad_request(anyhow!(
            "range is longer than {STATS_MAX_RANGE_DAYS} days"
        )));
    }
    Ok((from, to, today))
}

pub async fn get_stats(
    State(state): State<Shared>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<StatsReport>, ApiError> {
    let (from, to, today) = resolve_range(&q)?;
    Ok(Json(
        with_conn(state, move |c| {
            // Default start = earliest event day, clamped so an explicit
            // past `to` never yields an inverted range.
            let from = match from {
                Some(f) => f,
                None => stats::first_day(c)?.unwrap_or(today).min(to),
            };
            stats::stats_report(c, from, to, today)
        })
        .await?,
    ))
}

pub async fn get_topics(
    State(state): State<Shared>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<TopicsReport>, ApiError> {
    let (from, to, today) = resolve_range(&q)?;
    // Three phases so the shared connection is never held across the
    // blocking /embed calls (same split as daemon_note_block).
    let prep = with_conn(state.clone(), move |c| {
        let from = match from {
            Some(f) => f,
            None => stats::first_day(c)?.unwrap_or(today).min(to),
        };
        if crate::envfile::read(VERDICT_ENABLED_KEY).as_deref() == Some("off") {
            return Ok(Err(TopicsReport::empty(
                TopicsStatus::HelperDown,
                &from.to_string(),
                &to.to_string(),
            )));
        }
        topics_prepare(c, from, to).map(Ok)
    })
    .await?;
    let prep = match prep {
        Ok(p) => p,
        Err(report) => return Ok(Json(report)),
    };
    let (prep, embedded) = tokio::task::spawn_blocking(move || {
        let embedded = embed_pending(&EmbedClient::from_env(), &prep.pending);
        (prep, embedded)
    })
    .await
    .map_err(|e| anyhow!("embed task: {e}"))?;
    Ok(Json(
        with_conn(state, move |c| topics_finish(c, prep, embedded)).await?,
    ))
}

fn load_points(conn: &Connection, lo: NaiveDate, hi: NaiveDate) -> Result<Vec<BlockPoint>> {
    conn.prepare(
        "SELECT id, day, started_at, duration_seconds, jira_issue, description
           FROM blocks
          WHERE day BETWEEN ?1 AND ?2 AND is_personal = 0 AND ignored_at IS NULL
          ORDER BY id",
    )?
    .query_map(params![lo.to_string(), hi.to_string()], |r| {
        Ok(BlockPoint {
            id: r.get(0)?,
            day: r.get(1)?,
            started_at: r.get(2)?,
            seconds: r.get(3)?,
            ticket: r.get::<_, Option<String>>(4)?.filter(|t| !t.is_empty()),
            description: r.get(5)?,
            vec: vec![],
        })
    })?
    .collect::<rusqlite::Result<_>>()
    .map_err(Into::into)
}

struct TopicsPrep {
    from: NaiveDate,
    to: NaiveDate,
    all: Vec<BlockPoint>,
    pending: Vec<embed::PendingText>,
}

/// `None` = the helper failed; nothing may be stored (FR-19).
fn embed_pending(client: &EmbedClient, pending: &[embed::PendingText]) -> Option<Vec<Vec<f32>>> {
    let texts: Vec<String> = pending.iter().map(|p| p.text.clone()).collect();
    client.embed_all(&texts).ok()
}

fn topics_prepare(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Result<TopicsPrep> {
    let lo = from - Duration::days(TOPICS_HISTORY_DAYS);
    let all = load_points(conn, lo, to)?;
    let ids: Vec<i64> = all.iter().map(|p| p.id).collect();
    let pending = embed::pending(conn, &ids)?;
    Ok(TopicsPrep {
        from,
        to,
        all,
        pending,
    })
}

#[cfg(test)]
pub(super) fn topics_report(
    conn: &Connection,
    client: &EmbedClient,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<TopicsReport> {
    let prep = topics_prepare(conn, from, to)?;
    let embedded = embed_pending(client, &prep.pending);
    topics_finish(conn, prep, embedded)
}

fn topics_finish(
    conn: &Connection,
    prep: TopicsPrep,
    embedded: Option<Vec<Vec<f32>>>,
) -> Result<TopicsReport> {
    let TopicsPrep {
        from,
        to,
        all,
        pending,
    } = prep;
    let (from_s, to_s) = (from.to_string(), to.to_string());
    let Some(new_vectors) = embedded else {
        return Ok(TopicsReport::empty(
            TopicsStatus::HelperDown,
            &from_s,
            &to_s,
        ));
    };
    embed::store(conn, &pending, &new_vectors)?;
    let ids: Vec<i64> = all.iter().map(|p| p.id).collect();
    let mut vectors = embed::load(conn, &ids)?;

    let mut input = TopicsInput {
        from: from_s.clone(),
        to: to_s.clone(),
        now: vec![],
        history: vec![],
        ticket_summaries: HashMap::new(),
    };
    let mut skipped = 0;
    for mut p in all {
        let in_now = p.day >= from_s && p.day <= to_s;
        let Some(v) = vectors.remove(&p.id) else {
            skipped += i64::from(in_now);
            continue;
        };
        p.vec = v;
        if in_now {
            input.now.push(p.clone());
        } else {
            if p.day >= (from - Duration::days(TOPICS_HISTORY_DAYS)).to_string() {
                input.history.push(p);
            }
        }
    }
    let tickets: HashSet<&str> = input
        .now
        .iter()
        .filter_map(|p| p.ticket.as_deref())
        .collect();
    for t in tickets {
        if let Some(summary) = conn
            .query_row(
                "SELECT summary FROM jira_tickets WHERE key = ?1",
                [t],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            input.ticket_summaries.insert(t.to_string(), summary);
        }
    }
    let mut report = stats_topics::build(&input);
    report.skipped_blocks = skipped;
    Ok(report)
}
