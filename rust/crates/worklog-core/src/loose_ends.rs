//! Unbilled page (`GET /loose-ends`): per day of the current billing cycle,
//! what is still open — ticket lines Tempo does not cover, ticketless work,
//! and ticketed work not yet exported to the invoice form.
//!
//! A ticket line is every ticketed, non-personal, non-ignored block of one
//! (day, jira_issue); its hours are the **union** of the blocks' intervals.
//! Tempo's side is `tempo_remote_worklogs`, matched on `jira_issue` (or, when
//! the pull could not name the issue, through `jira_tickets.issue_id`).
//!
//! "Deleted" needs proof the day was pulled *after* the line was sent.
//! Blocks carry no sync timestamp, so the signal is: the day's latest pull
//! time — `MAX(pulled_at)` over that day's `tempo_remote_worklogs` and
//! `tempo_required_days` rows (each pull rewrites every day in its range, so
//! this is when Tempo last answered for that day) — is later than the send
//! time. The send time is `tempo_line_texts.auto_sent_at` for the line when
//! the 17:00 auto-send recorded one, else the end of the local day (a line is
//! never sent before its day is over except by hand, which leaves no trace).
//! A day never pulled is never "deleted": Tempo may simply be stale.
//! That send time is a guess for hand sends, so "deleted" also needs the
//! id itself to predate the pull: Tempo worklog ids only grow, so an id
//! below the largest id held by a pull made no later than the day's latest
//! pull existed when that pull ran. A line sent after the pull has a larger
//! id and is "sent, Tempo not pulled since", not deleted.
//!
//! Blocks sent by the app but absent from the pull (and not proven deleted)
//! count as in Tempo: `in_tempo_seconds` is the pulled seconds plus their
//! union, so a partly sent line reports only what is really missing.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::billing::{interval_at, union_seconds};
use crate::purge::{configured_cycle_start_day, cycle_start_on_or_before};
use crate::time_leaks_contract::{ExportGap, TempoGap, TempoGapReason, UnbilledDay, UnbilledDays};

pub fn for_cycle(conn: &Connection, today: &str) -> Result<UnbilledDays> {
    for_cycle_with(conn, today, configured_cycle_start_day())
}

fn for_cycle_with(conn: &Connection, today: &str, start_day: u32) -> Result<UnbilledDays> {
    let today: NaiveDate = today
        .parse()
        .with_context(|| format!("parsing today {today:?}"))?;
    let start = cycle_start_on_or_before(today, start_day);
    // 32 days always lands inside the next cycle, whatever the month lengths.
    let end = cycle_start_on_or_before(start + Duration::days(32), start_day) - Duration::days(1);
    let (from, to) = (start.to_string(), end.min(today).to_string());
    let tempo = TempoSide::load(conn, &from, &to)?;
    let mut by_day: BTreeMap<String, Vec<Blk>> = BTreeMap::new();
    for b in load_blocks(conn, &from, &to)? {
        by_day.entry(b.day.clone()).or_default().push(b);
    }
    let days = by_day
        .into_iter()
        .map(|(day, blocks)| day_answer(&tempo, day, &blocks))
        .filter(|d| !d.tempo.is_empty() || d.ticketless_seconds > 0 || d.export.is_some())
        .collect();
    Ok(UnbilledDays {
        cycle_start: start.to_string(),
        cycle_end: end.to_string(),
        tempo_pulled_at: conn
            .query_row(
                "SELECT MAX(pulled_at) FROM tempo_remote_worklogs",
                [],
                |r| r.get(0),
            )
            .context("latest Tempo pull")?,
        days,
    })
}

/// One work block, reduced to what the answer needs. `jira` is "" when ticketless.
struct Blk {
    day: String,
    jira: String,
    interval: (i64, i64),
    tempo_id: Option<String>,
    exported: bool,
}

fn load_blocks(conn: &Connection, from: &str, to: &str) -> Result<Vec<Blk>> {
    let mut stmt = conn.prepare(
        "SELECT day, TRIM(COALESCE(jira_issue, '')), started_at, duration_seconds,
                NULLIF(TRIM(COALESCE(tempo_worklog_id, '')), ''), COALESCE(exported_at, '') <> ''
         FROM blocks
         WHERE day BETWEEN ?1 AND ?2 AND is_personal = 0 AND ignored_at IS NULL",
    )?;
    let rows = stmt.query_map([from, to], |r| {
        Ok(Blk {
            day: r.get(0)?,
            jira: r.get(1)?,
            interval: interval_at(&r.get::<_, String>(2)?, r.get(3)?),
            tempo_id: r.get(4)?,
            exported: r.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<_>>()
        .context("listing unbilled blocks")
}

fn union<'a>(blocks: impl Iterator<Item = &'a Blk>) -> i64 {
    union_seconds(blocks.map(|b| b.interval).collect())
}

fn day_answer(tempo: &TempoSide, day: String, blocks: &[Blk]) -> UnbilledDay {
    let mut lines: BTreeMap<&str, Vec<&Blk>> = BTreeMap::new();
    for b in blocks.iter().filter(|b| !b.jira.is_empty()) {
        lines.entry(&b.jira).or_default().push(b);
    }
    let dismissed = tempo.dismissed.get(&day).cloned();
    let gaps = if dismissed.is_some() {
        Vec::new()
    } else {
        lines
            .iter()
            .filter_map(|(jira, line)| tempo.gap(&day, jira, line))
            .collect()
    };
    let unexported = union(blocks.iter().filter(|b| !b.jira.is_empty() && !b.exported));
    UnbilledDay {
        ticketless_seconds: union(blocks.iter().filter(|b| b.jira.is_empty())),
        export: (unexported > 0).then_some(ExportGap {
            seconds: unexported,
        }),
        tempo: gaps,
        dismissed,
        day,
    }
}

/// Tempo's side of the cycle, loaded once (no per-line queries).
struct TempoSide {
    /// (day, jira_issue) → summed remote seconds.
    remote: HashMap<(String, String), i64>,
    /// Every stored remote worklog id (a worklog moved to another day is not deleted).
    ids: HashSet<String>,
    /// day → that day's latest pull.
    pulled: HashMap<String, DateTime<Utc>>,
    /// day → largest numeric remote id held by a pull no later than the day's latest pull.
    known_max: HashMap<String, i64>,
    /// (day, jira_issue) → recorded auto-send time.
    sent: HashMap<(String, String), DateTime<Utc>>,
    dismissed: HashMap<String, String>,
}

impl TempoSide {
    fn load(conn: &Connection, from: &str, to: &str) -> Result<TempoSide> {
        let pairs = |sql: &str| -> Result<Vec<(String, String)>> {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        let keyed_time = |rows: Vec<(String, String, String)>| {
            rows.into_iter()
                .filter_map(|(d, j, t)| Some(((d, j), parse_utc(&t)?)))
                .collect()
        };
        let pulled: HashMap<String, DateTime<Utc>> = pairs(
            "SELECT day, MAX(pulled_at) FROM (
                SELECT day, pulled_at FROM tempo_remote_worklogs WHERE day BETWEEN ?1 AND ?2
                UNION ALL
                SELECT day, pulled_at FROM tempo_required_days WHERE day BETWEEN ?1 AND ?2)
             GROUP BY day",
        )?
        .into_iter()
        .filter_map(|(d, t)| Some((d, parse_utc(&t)?)))
        .collect();
        let (ids, numbered) = remote_ids(conn)?;
        let known_max = pulled
            .iter()
            .filter_map(|(day, at)| {
                let max = numbered
                    .iter()
                    .filter(|(t, _)| t <= at)
                    .map(|(_, id)| *id)
                    .max()?;
                Some((day.clone(), max))
            })
            .collect();
        Ok(TempoSide {
            remote: remote_sums(conn, from, to)?,
            ids,
            pulled,
            known_max,
            sent: keyed_time(triples(
                conn,
                "SELECT day, jira_issue, auto_sent_at FROM tempo_line_texts
                 WHERE day BETWEEN ?1 AND ?2 AND auto_sent_at IS NOT NULL",
                from,
                to,
            )?),
            dismissed: pairs(
                "SELECT day, reason FROM tempo_day_dismissals WHERE day BETWEEN ?1 AND ?2",
            )?
            .into_iter()
            .collect(),
        })
    }

    fn gap(&self, day: &str, jira: &str, line: &[&Blk]) -> Option<TempoGap> {
        let seconds = union(line.iter().copied());
        let key = (day.to_string(), jira.to_string());
        let remote = self.remote.get(&key).copied().unwrap_or(0);
        if remote >= seconds {
            return None;
        }
        // Sent by the app but not in the stored pull: deleted, or sent after the pull.
        let unseen: Vec<&Blk> = line
            .iter()
            .copied()
            .filter(|b| b.tempo_id.as_ref().is_some_and(|id| !self.ids.contains(id)))
            .collect();
        let gap = |in_tempo_seconds, reason| TempoGap {
            jira_issue: jira.to_string(),
            seconds,
            in_tempo_seconds,
            reason,
        };
        let mut unseen_ids = unseen.iter().filter_map(|b| b.tempo_id.as_deref());
        if self.pulled_after_send(&key) && unseen_ids.any(|id| self.predates_pull(day, id)) {
            return Some(gap(remote, TempoGapReason::Deleted));
        }
        let in_tempo = remote + union(unseen.into_iter());
        if line.iter().all(|b| b.tempo_id.is_some()) || in_tempo >= seconds {
            return None;
        }
        Some(gap(
            in_tempo,
            if in_tempo > 0 {
                TempoGapReason::Short
            } else {
                TempoGapReason::NotSent
            },
        ))
    }

    /// See the module doc: the id is below one a pull covering the day already held.
    fn predates_pull(&self, day: &str, id: &str) -> bool {
        let max = self.known_max.get(day);
        id.parse::<i64>()
            .ok()
            .zip(max)
            .is_some_and(|(id, max)| id < *max)
    }

    /// See the module doc: the day's latest pull is later than the line's send.
    fn pulled_after_send(&self, key: &(String, String)) -> bool {
        let Some(pulled) = self.pulled.get(&key.0) else {
            return false;
        };
        let sent = self.sent.get(key).copied().or_else(|| {
            let day: NaiveDate = key.0.parse().ok()?;
            Some(crate::tz::utc_window_for_local_day(day).1)
        });
        sent.is_some_and(|sent| *pulled > sent)
    }
}

/// (pull time, numeric remote worklog id).
type Numbered = Vec<(DateTime<Utc>, i64)>;

/// Every stored remote id, plus the numeric ones with the pull that stored them.
fn remote_ids(conn: &Connection) -> Result<(HashSet<String>, Numbered)> {
    let mut stmt = conn.prepare("SELECT tempo_worklog_id, pulled_at FROM tempo_remote_worklogs")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let (mut ids, mut numbered) = (HashSet::new(), Vec::new());
    for row in rows {
        let (id, at) = row.context("listing Tempo worklog ids")?;
        if let (Ok(n), Some(at)) = (id.parse::<i64>(), parse_utc(&at)) {
            numbered.push((at, n));
        }
        ids.insert(id);
    }
    Ok((ids, numbered))
}

fn remote_sums(conn: &Connection, from: &str, to: &str) -> Result<HashMap<(String, String), i64>> {
    let mut stmt = conn.prepare(
        "SELECT day, key, SUM(seconds) FROM (
            SELECT day, seconds, COALESCE(NULLIF(jira_issue, ''),
                (SELECT key FROM jira_tickets WHERE issue_id = CAST(r.issue_id AS TEXT) LIMIT 1)) AS key
            FROM tempo_remote_worklogs r WHERE day BETWEEN ?1 AND ?2)
         WHERE key IS NOT NULL GROUP BY day, key",
    )?;
    let rows = stmt.query_map([from, to], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?;
    rows.collect::<rusqlite::Result<_>>()
        .context("summing Tempo worklogs")
}

fn triples(
    conn: &Connection,
    sql: &str,
    from: &str,
    to: &str,
) -> Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn parse_utc(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
#[path = "loose_ends_test.rs"]
mod tests;
