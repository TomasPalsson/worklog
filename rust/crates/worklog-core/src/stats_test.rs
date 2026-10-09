use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection};

use super::*;
use crate::db::open_memory;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn ev(c: &Connection, source: &str, id: &str, ts: &str, details: Option<&str>, raw: Option<&str>) {
    let title = if source == "slack" {
        details
    } else {
        Some("t")
    };
    let details = if source == "slack" { None } else { details };
    c.execute(
        "INSERT INTO events (source, source_id, started_at, title, details, raw_json, duration_seconds)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            source,
            id,
            ts,
            title,
            details,
            raw_json::encode_raw_json(raw),
            if source == "gcal" { Some(1800) } else { None::<i64> }
        ],
    )
    .unwrap();
}

struct B<'a> {
    day: &'a str,
    secs: i64,
    ticket: Option<&'a str>,
    personal: i64,
    ignored: bool,
    tempo: Option<&'a str>,
    exported: bool,
}

fn block(c: &Connection, b: B) {
    c.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, estimated_by,
                             tempo_worklog_id, is_personal, ignored_at, exported_at)
         VALUES (?1, ?2, ?3, ?3, ?4, 'auto', ?5, ?6, ?7, ?8)",
        params![
            b.day,
            b.ticket,
            format!("{}T12:00:00Z", b.day),
            b.secs,
            b.tempo,
            b.personal,
            b.ignored.then_some("2026-09-04T00:00:00Z"),
            b.exported.then_some("2026-09-04T00:00:00Z"),
        ],
    )
    .unwrap();
}

fn tool(name: &str) -> String {
    format!(
        r#"{{"kind":"claude_tool","session_id":"s","tool":"{name}","input":{{}},"output":null,"output_cut_bytes":0,"files":[]}}"#
    )
}

fn prompt(text: &str) -> String {
    serde_json::json!({"kind":"claude_prompt","session_id":"s","text":text}).to_string()
}

fn seed(c: &Connection) {
    let long_cmd = format!(
        r#"{{"kind":"shell","command":"cargo test {}","cwd":null}}"#,
        "x ".repeat(300)
    );
    let git = r#"{"kind":"shell","command":"git status","cwd":null}"#;
    ev(
        c,
        "shell",
        "s1",
        "2026-09-02T03:30:00Z",
        None,
        Some(&long_cmd),
    );
    ev(c, "shell", "s2", "2026-09-02T16:00:00Z", None, Some(git));
    for (id, ts, text) in [
        ("p1", "2026-09-02T14:00:00Z", "Please fix this?"),
        ("p2", "2026-09-02T18:15:00Z", "thanks! Thank you so much"),
        (
            "p3",
            "2026-09-03T15:00:00Z",
            "wtf, sorry about the shitty bug",
        ),
        ("p4", "2026-09-01T15:00:00Z", "hello"),
    ] {
        ev(c, "claude_turn", id, ts, None, Some(&prompt(text)));
    }
    for (i, name) in ["Bash", "Bash", "Read"].iter().enumerate() {
        let id = format!("t{i}");
        ev(
            c,
            "claude_tool",
            &id,
            "2026-09-02T15:00:00Z",
            None,
            Some(&tool(name)),
        );
    }
    let ff = |c: &Connection, id, ts, url| ev(c, "firefox", id, ts, Some(url), None);
    ff(
        c,
        "f1",
        "2026-09-02T20:00:00Z",
        "https://www.Example.com:8080/path?q=1",
    );
    ff(c, "f2", "2026-09-02T20:01:00Z", "http://example.com/");
    ff(c, "f3", "2026-09-02T20:02:00Z", "not a url");
    ev(
        c,
        "slack",
        "sl1",
        "2026-09-02T17:00:00Z",
        Some("#dev"),
        None,
    );
    ev(
        c,
        "slack",
        "sl2",
        "2026-09-02T17:01:00Z",
        Some("#dev"),
        None,
    );
    ev(
        c,
        "slack",
        "sl3",
        "2026-09-02T17:02:00Z",
        Some("dm: Bob"),
        None,
    );
    let rl = |m: &str| format!(r#"{{"kind":"reflog","message":"{m}"}}"#);
    ev(
        c,
        "git_reflog",
        "r1",
        "2026-09-02T17:00:00Z",
        None,
        Some(&rl("commit: x")),
    );
    ev(
        c,
        "git_reflog",
        "r2",
        "2026-09-02T17:00:00Z",
        None,
        Some(&rl("checkout: y")),
    );
    ev(c, "github_commit", "gc", "2026-09-02T17:00:00Z", None, None);
    ev(c, "gcal", "g1", "2026-09-02T17:00:00Z", None, None);
    ev(c, "claude_work", "w1", "2026-09-02T17:00:00Z", None, None);
    let helper =
        r#"{"kind":"helper","parent_session_id":"s","helper_kind":"subagent","summary":""}"#;
    ev(
        c,
        "claude_helper",
        "h1",
        "2026-09-02T17:00:00Z",
        None,
        Some(helper),
    );
    let b = |day, secs, ticket, personal, ignored, tempo, exported| B {
        day,
        secs,
        ticket,
        personal,
        ignored,
        tempo,
        exported,
    };
    block(
        c,
        b("2026-09-01", 3600, Some("ABC-1"), 0, false, Some(""), false),
    );
    block(
        c,
        b(
            "2026-09-02",
            7200,
            Some("ABC-1"),
            0,
            false,
            Some("w1"),
            true,
        ),
    );
    block(c, b("2026-09-03", 600, None, 1, false, None, false));
    block(c, b("2026-09-03", 300, None, 1, true, None, false));
}

fn report(c: &Connection, today: &str) -> StatsReport {
    stats_report(c, d("2026-09-01"), d("2026-09-03"), d(today)).unwrap()
}

fn find(v: &[Ranked], label: &str) -> i64 {
    v.iter().find(|r| r.label == label).map_or(-1, |r| r.value)
}

#[test]
fn totals_daily_and_rankings() {
    let _g = tz::test_env_lock();
    std::env::set_var("WORKLOG_TZ", "-05:00");
    let c = open_memory().unwrap();
    seed(&c);
    c.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('ABC-1', 'Do it')",
        [],
    )
    .unwrap();
    let r = report(&c, "2026-09-03");
    std::env::remove_var("WORKLOG_TZ");

    let t = &r.totals;
    assert_eq!(
        (t.work_seconds, t.personal_seconds, t.ignored_seconds),
        (10800, 600, 300)
    );
    assert_eq!((t.blocks, t.days_worked), (4, 2));
    assert_eq!(
        (t.prompts, t.tool_calls, t.shell_commands, t.slack_messages),
        (4, 3, 2, 3)
    );
    assert_eq!((t.commits, t.reflog_entries, t.browser_minutes), (2, 2, 3));
    assert_eq!(
        (
            t.meetings,
            t.meeting_seconds,
            t.helpers,
            t.claude_busy_minutes
        ),
        (1, 1800, 1, 1)
    );
    assert_eq!(r.daily.len(), 3);
    assert_eq!(r.daily[1].work_seconds, 7200);
    assert_eq!(
        (
            r.daily[1].prompts,
            r.daily[1].tool_calls,
            r.daily[1].commits
        ),
        (2, 3, 2)
    );
    assert_eq!((r.daily[1].folders, r.daily[1].tickets), (1, 1));
    // The 22:30 local shell lands on 09-01 under UTC-5.
    assert_eq!(r.daily[0].first_at.as_deref(), Some("10:00"));
    assert_eq!(r.daily[0].last_at.as_deref(), Some("22:30"));
    assert_eq!(r.first_day.as_deref(), Some("2026-09-01"));

    assert_eq!((find(&r.tools, "Bash"), find(&r.tools, "Read")), (2, 1));
    assert_eq!((find(&r.shell, "cargo"), find(&r.shell, "git")), (1, 1));
    assert_eq!(find(&r.slack_channels, "#dev"), 2);
    assert_eq!(
        r.domains,
        vec![Ranked {
            label: "example.com".into(),
            value: 2
        }]
    );
    assert_eq!(find(&r.helpers, "subagent"), 1);
    assert_eq!(find(&r.folders, "(no folder)"), 10800);
    assert_eq!(find(&r.estimates, "auto"), 4);
    assert_eq!(find(&r.ticket_origin, "none"), 4);
    assert_eq!(
        r.sync,
        SyncStats {
            synced_seconds: 7200,
            unsynced_seconds: 3600,
            exported_blocks: 1
        }
    );
    assert_eq!(r.flow_blocks.len(), 4);
    assert_eq!(r.flow_blocks[3].kind, "ignored");
    let k = &r.tickets[0];
    assert_eq!(
        (
            k.key.as_str(),
            k.summary.as_deref(),
            k.seconds,
            k.blocks,
            k.days_active
        ),
        ("ABC-1", Some("Do it"), 10800, 2, 2)
    );
    assert_eq!(
        k.days,
        vec!["2026-09-01".to_string(), "2026-09-02".to_string()]
    );
    assert_eq!(
        (k.first_day.as_str(), k.last_day.as_str()),
        ("2026-09-01", "2026-09-02")
    );
}

#[test]
fn punchcard_uses_local_weekday_and_hour() {
    let _g = tz::test_env_lock();
    std::env::set_var("WORKLOG_TZ", "-05:00");
    let c = open_memory().unwrap();
    seed(&c);
    let r = report(&c, "2026-09-03");
    std::env::remove_var("WORKLOG_TZ");
    let wd = d("2026-09-01").weekday().num_days_from_monday() as usize;
    assert_eq!(r.punchcard.len(), 7);
    assert!(r.punchcard.iter().all(|row| row.len() == 24));
    assert_eq!(r.punchcard[wd][22], 1);
    assert_eq!(r.punchcard[wd][3], 0);
    // human events only: 2 shell + 4 prompts + 3 slack + 3 firefox + 1 commit.
    let total: i64 = r.punchcard.iter().flatten().sum();
    assert_eq!(total, 2 + 4 + 3 + 3 + 1);
}

#[test]
fn prompt_counters() {
    let _g = tz::test_env_lock();
    let c = open_memory().unwrap();
    seed(&c);
    let p = report(&c, "2026-09-03").prompt;
    assert_eq!((p.count, p.avg_chars, p.longest_chars), (4, 19, 31));
    assert_eq!(
        (
            p.questions,
            p.please,
            p.thanks,
            p.sorry,
            p.swears,
            p.exclaims
        ),
        (1, 1, 1, 1, 1, 1)
    );
    let openers: Vec<&str> = p.top_openers.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(openers, ["hello", "please", "thanks", "wtf"]);
}

#[test]
fn interrupts_and_slash_commands_are_not_prompts() {
    let mut acc = crate::stats_prompt::PromptAcc::default();
    acc.add("[Request interrupted by user]");
    acc.add("[Request interrupted by user for tool use]");
    acc.add("<command-message>flow:next</command-message>");
    acc.add("<task-notification>done</task-notification>");
    acc.add("ok please go");
    let p = acc.finish();
    assert_eq!(
        (p.count, p.interrupts, p.slash_commands, p.please),
        (1, 2, 1, 1)
    );
    let openers: Vec<&str> = p.top_openers.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(openers, ["ok"]);
}

#[test]
fn records_and_streaks() {
    let _g = tz::test_env_lock();
    std::env::set_var("WORKLOG_TZ", "-05:00");
    let c = open_memory().unwrap();
    seed(&c);
    let r = report(&c, "2026-09-03");
    let long = stats_report(&c, d("2026-09-01"), d("2026-09-10"), d("2026-09-10")).unwrap();
    std::env::remove_var("WORKLOG_TZ");
    let rec = r.records;
    let day = |s: &str| s.to_string();
    assert_eq!(
        rec.busiest_day,
        Some(DayRecord {
            day: day("2026-09-02"),
            seconds: 7200
        })
    );
    assert_eq!(rec.longest_block.unwrap().seconds, 7200);
    assert_eq!(
        rec.earliest_start,
        Some(TimeRecord {
            day: day("2026-09-02"),
            time: day("09:00")
        })
    );
    assert_eq!(
        rec.latest_finish,
        Some(TimeRecord {
            day: day("2026-09-01"),
            time: day("22:30")
        })
    );
    assert_eq!(
        rec.most_prompts,
        Some(CountRecord {
            day: day("2026-09-02"),
            n: 2
        })
    );
    assert_eq!(
        rec.most_tools,
        Some(CountRecord {
            day: day("2026-09-02"),
            n: 3
        })
    );
    // 09-03 has no work yet: the streak ending 09-02 still counts.
    assert_eq!((rec.longest_streak, rec.current_streak), (2, 2));
    assert_eq!(
        (long.records.longest_streak, long.records.current_streak),
        (2, 0)
    );
}

#[test]
fn empty_db_is_all_zeros_and_nulls() {
    let c = open_memory().unwrap();
    let r = report(&c, "2026-09-03");
    assert_eq!(r.totals, StatsTotals::default());
    assert_eq!(r.daily.len(), 3);
    assert!(r
        .daily
        .iter()
        .all(|x| x.work_seconds == 0 && x.first_at.is_none()));
    assert_eq!(r.first_day, None);
    assert_eq!(r.records, StatsRecords::default());
    assert_eq!(r.prompt, PromptStats::default());
    assert!(r.tools.is_empty() && r.tickets.is_empty() && r.flow_blocks.is_empty());
    assert_eq!(r.punchcard.iter().flatten().sum::<i64>(), 0);
    serde_json::to_string(&r).unwrap();
}

#[test]
fn single_day_range() {
    let c = open_memory().unwrap();
    let r = stats_report(&c, d("2026-09-01"), d("2026-09-01"), d("2026-09-01")).unwrap();
    assert_eq!(r.daily.len(), 1);
}

#[test]
fn host_parsing() {
    assert_eq!(
        host_of("https://user@WWW.a.io:9/x").as_deref(),
        Some("a.io")
    );
    assert_eq!(host_of("nope"), None);
    assert_eq!(host_of("https:///x"), None);
}

#[test]
fn folder_and_flow_sources_come_from_linked_events() {
    let _g = tz::test_env_lock();
    let c = open_memory().unwrap();
    block(
        &c,
        B {
            day: "2026-09-02",
            secs: 3600,
            ticket: None,
            personal: 0,
            ignored: false,
            tempo: None,
            exported: false,
        },
    );
    let block_id = c.last_insert_rowid();
    for (id, source, path) in [
        ("l1", "shell", Some("/tmp/proj/foo/.claude/worktrees/x")),
        ("l2", "shell", Some("/tmp/proj/foo")),
        ("l3", "slack", None),
    ] {
        c.execute(
            "INSERT INTO events (source, source_id, started_at, title, project_path)
             VALUES (?1, ?2, '2026-09-02T12:00:00Z', 't', ?3)",
            params![source, id, path],
        )
        .unwrap();
        c.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, c.last_insert_rowid()],
        )
        .unwrap();
    }
    let r = stats_report(&c, d("2026-09-02"), d("2026-09-02"), d("2026-09-02")).unwrap();
    assert_eq!(find(&r.folders, "foo"), 3600);
    let got: Vec<(&str, i64)> = r.flow_blocks[0]
        .sources
        .iter()
        .map(|s| (s.source.as_str(), s.n))
        .collect();
    assert_eq!(got, [("shell", 2), ("slack", 1)]);
}
