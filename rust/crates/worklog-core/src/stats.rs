//! Aggregation behind GET /stats (contract: `stats_contract.rs`). One pass
//! over the range's events and one over its blocks; `raw_json` is only
//! inflated for the sources that need it, and only the handful of fields
//! used are deserialised.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike};
use rusqlite::{params, Connection};
use serde::Deserialize;

use crate::stats_contract::*;
use crate::stats_prompt::PromptAcc;
use crate::stats_records::fill_records;
use crate::{billing, raw_json, tz};

/// The few `RawRecord` fields the stats read (`kind` tag, flattened).
#[derive(Deserialize, Default)]
struct Raw {
    tool: Option<String>,
    command: Option<String>,
    text: Option<String>,
    helper_kind: Option<String>,
    message: Option<String>,
}

/// Local date of the earliest event, if any.
pub fn first_day(conn: &Connection) -> Result<Option<NaiveDate>> {
    let min: Option<String> =
        conn.query_row("SELECT MIN(started_at) FROM events", [], |r| r.get(0))?;
    Ok(min
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&tz::day_offset()).date_naive()))
}

pub(crate) fn ranked(map: HashMap<String, i64>, top: usize) -> Vec<Ranked> {
    let mut v: Vec<Ranked> = map
        .into_iter()
        .map(|(label, value)| Ranked { label, value })
        .collect();
    v.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    v.truncate(top);
    v
}

pub(crate) fn bump(map: &mut HashMap<String, i64>, key: impl Into<String>, by: i64) {
    *map.entry(key.into()).or_insert(0) += by;
}

/// Host of an http(s)-ish URL without "www."; `None` when unparseable.
fn host_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_string())
}

struct BlockRow {
    id: i64,
    day: String,
    ticket: Option<String>,
    seconds: i64,
    estimated_by: Option<String>,
    synced: bool,
    kind: &'static str,
    exported: bool,
    origin: Option<String>,
}

pub fn stats_report(
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
) -> Result<StatsReport> {
    let ndays = ((to - from).num_days() + 1).max(0) as usize;
    let idx = |d: NaiveDate| {
        let i = (d - from).num_days();
        (0..ndays as i64).contains(&i).then_some(i as usize)
    };
    let mut daily: Vec<DailyStat> = (0..ndays)
        .map(|i| DailyStat {
            day: (from + Duration::days(i as i64)).to_string(),
            work_seconds: 0,
            personal_seconds: 0,
            ignored_seconds: 0,
            prompts: 0,
            tool_calls: 0,
            shell: 0,
            slack: 0,
            browser_minutes: 0,
            claude_busy_minutes: 0,
            commits: 0,
            meeting_seconds: 0,
            first_at: None,
            last_at: None,
            folders: 0,
            tickets: 0,
        })
        .collect();
    let mut t = StatsTotals::default();
    let mut punch = vec![vec![0i64; 24]; 7];
    let (mut tools, mut shell, mut slack, mut domains, mut helpers) = (
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    );
    let mut prompt = PromptAcc::default();

    // ---- events: one pass ----
    let off = tz::day_offset();
    let start = tz::utc_window_for_local_day(from).0.to_rfc3339();
    let end = tz::utc_window_for_local_day(to).1.to_rfc3339();
    let mut stmt = conn.prepare(
        "SELECT source, started_at, title, details,
                CASE WHEN source IN ('claude_tool','shell','claude_turn','claude_helper','git_reflog')
                     THEN raw_json END, duration_seconds
           FROM events WHERE started_at >= ?1 AND started_at < ?2",
    )?;
    let mut rows = stmt.query(params![start, end])?;
    while let Some(r) = rows.next()? {
        let source: String = r.get(0)?;
        let Ok(ts) = DateTime::parse_from_rfc3339(&r.get::<_, String>(1)?) else {
            continue;
        };
        let local = ts.with_timezone(&off);
        let Some(i) = idx(local.date_naive()) else {
            continue;
        };
        let raw: Raw = raw_json::decode_raw_json(r, 4)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let d = &mut daily[i];
        match source.as_str() {
            "claude_turn" => {
                t.prompts += 1;
                d.prompts += 1;
                if let Some(text) = &raw.text {
                    prompt.add(text);
                }
            }
            "claude_tool" => {
                t.tool_calls += 1;
                d.tool_calls += 1;
                if let Some(tool) = raw.tool {
                    bump(&mut tools, tool, 1);
                }
            }
            "claude_helper" => {
                t.helpers += 1;
                if let Some(k) = raw.helper_kind {
                    bump(&mut helpers, k, 1);
                }
            }
            "claude_message" => t.claude_messages += 1,
            "claude_work" => {
                t.claude_busy_minutes += 1;
                d.claude_busy_minutes += 1;
            }
            "shell" => {
                t.shell_commands += 1;
                d.shell += 1;
                if let Some(w) = raw
                    .command
                    .as_deref()
                    .and_then(|c| c.split_whitespace().next())
                {
                    bump(&mut shell, w, 1);
                }
            }
            "git_reflog" => {
                t.reflog_entries += 1;
                if raw.message.is_some_and(|m| m.starts_with("commit")) {
                    t.commits += 1;
                    d.commits += 1;
                }
            }
            "github_commit" => {
                t.commits += 1;
                d.commits += 1;
            }
            "github_pr" => t.prs += 1,
            "slack" => {
                t.slack_messages += 1;
                d.slack += 1;
                bump(&mut slack, r.get::<_, String>(2)?, 1);
            }
            "firefox" => {
                t.browser_minutes += 1;
                d.browser_minutes += 1;
                if let Some(h) = r.get::<_, Option<String>>(3)?.as_deref().and_then(host_of) {
                    bump(&mut domains, h, 1);
                }
            }
            "gcal" => {
                let secs = r.get::<_, Option<i64>>(5)?.unwrap_or(0);
                t.meetings += 1;
                t.meeting_seconds += secs;
                d.meeting_seconds += secs;
            }
            _ => {}
        }
        if crate::infer_lanes::is_human(&source) {
            punch[local.weekday().num_days_from_monday() as usize][local.hour() as usize] += 1;
            let hm = local.format("%H:%M").to_string();
            if d.first_at.as_ref().is_none_or(|f| hm < *f) {
                d.first_at = Some(hm.clone());
            }
            if d.last_at.as_ref().is_none_or(|l| hm > *l) {
                d.last_at = Some(hm);
            }
        }
    }
    drop(rows);
    drop(stmt);

    // ---- blocks ----
    let (from_s, to_s) = (from.to_string(), to.to_string());
    let blocks: Vec<BlockRow> = conn
        .prepare(
            "SELECT id, day, jira_issue, duration_seconds, estimated_by, tempo_worklog_id,
                    is_personal, ignored_at, exported_at, ticket_origin
               FROM blocks WHERE day BETWEEN ?1 AND ?2 ORDER BY started_at, id",
        )?
        .query_map(params![from_s, to_s], |r| {
            let tempo: Option<String> = r.get(5)?;
            let ignored: Option<String> = r.get(7)?;
            let personal: i64 = r.get(6)?;
            Ok(BlockRow {
                id: r.get(0)?,
                day: r.get(1)?,
                ticket: r.get(2)?,
                seconds: r.get(3)?,
                estimated_by: r.get(4)?,
                synced: tempo.is_some_and(|s| !s.trim().is_empty()),
                kind: if ignored.is_some() {
                    "ignored"
                } else if personal == 1 {
                    "personal"
                } else {
                    "work"
                },
                exported: r.get::<_, Option<String>>(8)?.is_some(),
                origin: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut sources: HashMap<i64, Vec<FlowSource>> = HashMap::new();
    let mut src_stmt = conn.prepare(
        "SELECT be.block_id, e.source, COUNT(*)
           FROM block_events be
           JOIN events e ON e.id = be.event_id
           JOIN blocks b ON b.id = be.block_id
          WHERE b.day BETWEEN ?1 AND ?2
          GROUP BY be.block_id, e.source
          ORDER BY COUNT(*) DESC, e.source",
    )?;
    for row in src_stmt.query_map(params![from_s, to_s], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            FlowSource {
                source: r.get(1)?,
                n: r.get(2)?,
            },
        ))
    })? {
        let (id, s) = row?;
        sources.entry(id).or_default().push(s);
    }
    type JiraInfo = (Option<String>, Option<String>, Option<String>);
    let jira: HashMap<String, JiraInfo> = conn
        .prepare("SELECT key, summary, status, status_category FROM jira_tickets")?
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
        .collect::<rusqlite::Result<_>>()?;

    let mut flow_blocks = Vec::with_capacity(blocks.len());
    let mut folders: HashMap<String, i64> = HashMap::new();
    let mut day_folders: Vec<BTreeSet<String>> = vec![BTreeSet::new(); ndays];
    let mut day_tickets: Vec<BTreeSet<String>> = vec![BTreeSet::new(); ndays];
    let mut ticket_acc: HashMap<String, (i64, i64, BTreeMap<String, i64>)> = HashMap::new();
    let (mut estimates, mut origins) = (HashMap::new(), HashMap::new());
    let mut rec = StatsRecords::default();
    for b in &blocks {
        t.blocks += 1;
        bump(
            &mut estimates,
            b.estimated_by.as_deref().unwrap_or("none"),
            1,
        );
        bump(&mut origins, b.origin.as_deref().unwrap_or("none"), 1);
        flow_blocks.push(FlowBlockStat {
            seconds: b.seconds,
            kind: b.kind,
            ticket: b.ticket.clone(),
            sources: sources.remove(&b.id).unwrap_or_default(),
        });
        let Some(i) = b.day.parse().ok().and_then(idx) else {
            continue;
        };
        match b.kind {
            "ignored" => {
                t.ignored_seconds += b.seconds;
                daily[i].ignored_seconds += b.seconds;
            }
            "personal" => {
                t.personal_seconds += b.seconds;
                daily[i].personal_seconds += b.seconds;
            }
            _ => {
                t.work_seconds += b.seconds;
                daily[i].work_seconds += b.seconds;
                let folder = billing::work_folder_for_block(conn, b.id)?;
                let label = folder.unwrap_or_else(|| "(no folder)".to_string());
                bump(&mut folders, label.clone(), b.seconds);
                day_folders[i].insert(label);
                if rec
                    .longest_block
                    .as_ref()
                    .is_none_or(|l| b.seconds > l.seconds)
                {
                    rec.longest_block = Some(LongestBlock {
                        day: b.day.clone(),
                        seconds: b.seconds,
                        ticket: b.ticket.clone(),
                    });
                }
                if let Some(k) = &b.ticket {
                    day_tickets[i].insert(k.clone());
                    let e = ticket_acc.entry(k.clone()).or_default();
                    e.0 += b.seconds;
                    e.1 += 1;
                    *e.2.entry(b.day.clone()).or_default() += b.seconds;
                }
            }
        }
    }
    for (i, d) in daily.iter_mut().enumerate() {
        d.folders = day_folders[i].len() as i64;
        d.tickets = day_tickets[i].len() as i64;
    }
    t.days_worked = daily.iter().filter(|d| d.work_seconds > 0).count() as i64;

    let mut tickets: Vec<TicketStat> = ticket_acc
        .into_iter()
        .map(|(key, (seconds, n, days))| {
            let (summary, status, status_category) = jira.get(&key).cloned().unwrap_or_default();
            TicketStat {
                summary,
                status,
                status_category,
                key,
                seconds,
                blocks: n,
                first_day: days.keys().next().cloned().unwrap_or_default(),
                last_day: days.keys().next_back().cloned().unwrap_or_default(),
                days_active: days.len() as i64,
                days: days.keys().cloned().collect(),
                day_seconds: days.values().copied().collect(),
            }
        })
        .collect();
    tickets.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.key.cmp(&b.key)));
    tickets.truncate(40);

    let sync = SyncStats {
        synced_seconds: blocks
            .iter()
            .filter(|b| b.kind == "work" && b.synced)
            .map(|b| b.seconds)
            .sum(),
        unsynced_seconds: blocks
            .iter()
            .filter(|b| b.kind == "work" && !b.synced)
            .map(|b| b.seconds)
            .sum(),
        exported_blocks: blocks.iter().filter(|b| b.exported).count() as i64,
    };

    fill_records(&mut rec, &daily, to.min(today), from);

    Ok(StatsReport {
        from: from_s,
        to: to_s,
        today: today.to_string(),
        first_day: first_day(conn)?.map(|d| d.to_string()),
        totals: t,
        daily,
        punchcard: punch,
        flow_blocks,
        tools: ranked(tools, 12),
        shell: ranked(shell, 12),
        slack_channels: ranked(slack, 10),
        domains: ranked(domains, 10),
        folders: ranked(folders, 10),
        helpers: ranked(helpers, 8),
        tickets,
        prompt: prompt.finish(),
        records: rec,
        estimates: ranked(estimates, usize::MAX),
        ticket_origin: ranked(origins, usize::MAX),
        sync,
    })
}

#[cfg(test)]
#[path = "stats_test.rs"]
mod tests;
