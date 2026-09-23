//! Time-context step for Slack events: when the day's `claude`/`shell`/
//! `git_reflog` activity around a Slack message clearly names one project,
//! `routing::load_pending` files the message under it without asking the
//! model. Split out of routing.rs, which is at the 400-line size limit.
//! Never applied to firefox events (routing.rs gates the call by source).

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection};

/// How far from the Slack event a claude/shell/git_reflog event may lie and
/// still count as context.
const WINDOW_MINUTES: i64 = 10;
/// The dominant key must outnumber the runner-up by at least this ratio.
const DOMINANCE_RATIO: u32 = 3;

const CONTEXT_SOURCES: [&str; 3] = ["claude", "shell", "git_reflog"];

/// Collapse a raw `project_path` to its billable root key: the last path
/// segment under `~/Desktop/Work` or `~/Desktop/Projects`, stripping
/// worktree scaffolding (`/.claude/worktrees/...`) first — mirrors
/// `collectors::fish::repo_root_for`. `None` outside both roots.
fn context_key(path: &str, home: &str) -> Option<String> {
    let base = match path.find("/.claude/") {
        Some(i) => &path[..i],
        None => path,
    };
    for root in ["Desktop/Work", "Desktop/Projects"] {
        let prefix = format!("{home}/{root}");
        let Some(rest) = base.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            continue;
        }
        let first = rest.split('/').next().unwrap_or(rest);
        if !first.is_empty() {
            return Some(first.to_owned());
        }
    }
    None
}

/// The day's `claude`/`shell`/`git_reflog` events as `(started_at, key)`
/// pairs, dropping events with no `project_path` usable as a key.
pub(crate) fn context_events_for_day(
    conn: &Connection,
    day: NaiveDate,
) -> Result<Vec<(DateTime<Utc>, String)>> {
    let Some(home) = dirs::home_dir() else {
        return Ok(Vec::new());
    };
    let home = home.to_string_lossy().into_owned();
    let (start, end) = crate::tz::utc_window_for_local_day(day);
    let mut stmt = conn
        .prepare(
            "SELECT started_at, project_path FROM events
              WHERE source IN (?1, ?2, ?3) AND started_at >= ?4 AND started_at < ?5
                AND project_path IS NOT NULL",
        )
        .context("preparing context events query")?;
    let rows = stmt
        .query_map(
            params![
                CONTEXT_SOURCES[0],
                CONTEXT_SOURCES[1],
                CONTEXT_SOURCES[2],
                start.to_rfc3339(),
                end.to_rfc3339(),
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .context("querying context events")?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .filter_map(|(started_at, project_path)| {
            let time = DateTime::parse_from_rfc3339(&started_at)
                .ok()?
                .with_timezone(&Utc);
            let key = context_key(&project_path, &home)?;
            Some((time, key))
        })
        .collect())
}

/// Pure decision: the project key the day's context events point to at
/// `event_time`, within ±10 minutes — `None` when there's no clear winner
/// (nothing in range, or the top key doesn't beat the runner-up 3-to-1).
pub(crate) fn dominant_key(
    event_time: DateTime<Utc>,
    context: &[(DateTime<Utc>, String)],
) -> Option<String> {
    let window_secs = WINDOW_MINUTES * 60;
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for (t, key) in context {
        if (*t - event_time).num_seconds().abs() <= window_secs {
            *counts.entry(key.as_str()).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(&str, u32)> = counts.into_iter().collect();
    sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    match sorted.as_slice() {
        [] => None,
        [(key, _)] => Some((*key).to_owned()),
        [(top_key, top_n), (_, second_n), ..] => {
            (*top_n >= DOMINANCE_RATIO * second_n).then(|| (*top_key).to_owned())
        }
    }
}

/// A Slack event's time-context hit: the dominant key at `started_at`
/// (RFC-3339), only when it's one of the event's own `options`. Callers
/// gate this to Slack events only — firefox never reaches it.
pub(crate) fn context_hit(
    started_at: &str,
    options: &[String],
    context: &[(DateTime<Utc>, String)],
) -> Option<String> {
    let event_time = DateTime::parse_from_rfc3339(started_at)
        .ok()?
        .with_timezone(&Utc);
    let key = dominant_key(event_time, context)?;
    options.iter().any(|o| o == &key).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap() + Duration::minutes(min)
    }

    fn key(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn dominant_project_wins() {
        let context = vec![
            (t(-5), key("sjukra")),
            (t(-3), key("sjukra")),
            (t(2), key("sjukra")),
            (t(4), key("other")),
        ];
        assert_eq!(dominant_key(t(0), &context), Some("sjukra".to_string()));
    }

    #[test]
    fn tie_is_no_decision() {
        let mut context = Vec::new();
        for i in 0..7 {
            context.push((t(i), key("a")));
            context.push((t(-i), key("b")));
        }
        assert_eq!(dominant_key(t(0), &context), None);
    }

    #[test]
    fn three_times_the_runner_up_is_dominant() {
        let mut context = Vec::new();
        for _ in 0..6 {
            context.push((t(1), key("a")));
        }
        for _ in 0..2 {
            context.push((t(1), key("b")));
        }
        assert_eq!(dominant_key(t(0), &context), Some("a".to_string()));
    }

    #[test]
    fn just_under_three_times_is_no_decision() {
        let mut context = Vec::new();
        for _ in 0..5 {
            context.push((t(1), key("a")));
        }
        for _ in 0..2 {
            context.push((t(1), key("b")));
        }
        assert_eq!(dominant_key(t(0), &context), None);
    }

    #[test]
    fn outside_the_window_is_ignored() {
        let context = vec![(t(11), key("a"))];
        assert_eq!(dominant_key(t(0), &context), None);
    }

    #[test]
    fn exactly_ten_minutes_is_still_in_window() {
        let context = vec![(t(10), key("a"))];
        assert_eq!(dominant_key(t(0), &context), Some("a".to_string()));
    }

    #[test]
    fn dominant_key_not_in_options_is_no_hit() {
        let context = vec![(t(0), key("sjukra"))];
        let started_at = "2026-04-20T09:00:00+00:00";
        let options = vec!["other-project".to_string()];
        assert_eq!(context_hit(started_at, &options, &context), None);
    }

    #[test]
    fn context_key_collapses_worktrees_and_subdirs_under_either_root() {
        let home = "/Users/x";
        assert_eq!(
            context_key("/Users/x/Desktop/Work/sjukra/.claude/worktrees/foo", home),
            Some("sjukra".to_string())
        );
        assert_eq!(
            context_key("/Users/x/Desktop/Projects/worklog/rust", home),
            Some("worklog".to_string())
        );
        assert_eq!(context_key("/Users/x/dotfiles", home), None);
    }
}
