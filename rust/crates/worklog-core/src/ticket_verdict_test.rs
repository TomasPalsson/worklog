use super::*;
use crate::db::open_memory;
use crate::models::{Event, JiraTicket};
use crate::repo;
use crate::routing_contract::{DEFAULT_ABSTAIN_MARGIN, DEFAULT_RUNNER_UP_RATIO};
use crate::verdict_contract::{RankedOption, Ranking};
use crate::verdict_decisions::list_since;
use std::cell::Cell;

const DAY: &str = "2026-04-20";

fn rule() -> RouteRule {
    RouteRule {
        abstain_margin: DEFAULT_ABSTAIN_MARGIN,
        runner_up_ratio: DEFAULT_RUNNER_UP_RATIO,
    }
}

fn open(conn: &Connection, key: &str) {
    repo::upsert_ticket(
        conn,
        &JiraTicket {
            key: key.into(),
            summary: "Real ticket".into(),
            status: Some("In Progress".into()),
            project_key: None,
            updated: Some("2026-04-19T00:00:00Z".into()),
            issue_id: None,
        },
    )
    .unwrap();
}

fn folder_path(tail: &str) -> String {
    format!("{}/{tail}", crate::billing::work_prefix().unwrap())
}

fn block_on(conn: &Connection, day: &str, jira: Option<&str>, origin: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, ticket_origin, started_at, ended_at, duration_seconds)
         VALUES (?1, ?2, ?3, ?1 || 'T10:00:00+00:00', ?1 || 'T10:30:00+00:00', 1800)",
        rusqlite::params![day, jira, origin],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn event_in(conn: &Connection, block: i64, title: &str, path: Option<&str>, session: Option<&str>) {
    let sid = format!("{block}-{title}");
    let mut e = Event::minimal("claude", sid, format!("{DAY}T10:00:00+00:00"), title);
    e.project_path = path.map(str::to_owned);
    e.session_id = session.map(str::to_owned);
    let id = repo::upsert_event(conn, &e).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        rusqlite::params![block, id],
    )
    .unwrap();
}

fn block(conn: &Connection, id: i64) -> Block {
    repo::get_block(conn, id).unwrap().unwrap()
}

fn options(conn: &Connection, id: i64) -> Vec<String> {
    ticket_options(conn, &block(conn, id)).unwrap()
}

/// A block on `day` in folder `tail`, already logged under `key`.
fn past(conn: &Connection, day: &str, key: &str, tail: &str) {
    let b = block_on(conn, day, Some(key), Some("manual"));
    event_in(
        conn,
        b,
        &format!("past-{day}-{key}"),
        Some(&folder_path(tail)),
        None,
    );
}

#[test]
fn options_run_session_then_events_then_recent_in_that_order() {
    let conn = open_memory().unwrap();
    for k in ["VIT-212", "EVT-1", "REC-1", "REC-2"] {
        open(&conn, k);
    }
    past(&conn, "2026-04-10", "REC-1", "acme");
    past(&conn, "2026-04-15", "REC-2", "acme");
    let b = block_on(&conn, DAY, None, None);
    event_in(
        &conn,
        b,
        "fix EVT-1 login",
        Some(&folder_path("acme")),
        Some("s1"),
    );
    crate::session_tickets::set(&conn, "s1", "VIT-212").unwrap();
    // Session first, event key next, then recents newest-first (REC-2 before REC-1):
    // catches alphabetical sorting and recents-before-events.
    assert_eq!(options(&conn, b), ["VIT-212", "EVT-1", "REC-2", "REC-1"]);
}

#[test]
fn a_key_found_by_two_sources_is_offered_once() {
    let conn = open_memory().unwrap();
    open(&conn, "VIT-212");
    let b = block_on(&conn, DAY, None, None);
    event_in(&conn, b, "VIT-212 again", None, Some("s1"));
    crate::session_tickets::set(&conn, "s1", "VIT-212").unwrap();
    assert_eq!(options(&conn, b), ["VIT-212"]); // catches a missing dedupe
}

#[test]
fn seven_candidates_are_cut_to_the_first_six_and_six_stay_six() {
    let conn = open_memory().unwrap();
    let keys: Vec<String> = (1..=7).map(|n| format!("AAA-{n}")).collect();
    for k in &keys {
        open(&conn, k);
    }
    let b = block_on(&conn, DAY, None, None);
    event_in(&conn, b, &keys[..6].join(" "), None, None);
    assert_eq!(options(&conn, b), keys[..6]); // exactly at the cap: nothing lost
    let b7 = block_on(&conn, DAY, None, None);
    event_in(&conn, b7, &keys.join(" "), None, None);
    assert_eq!(options(&conn, b7), keys[..6]); // just past: the 7th is dropped (> vs >=)
}

#[test]
fn event_keys_that_are_not_open_tickets_are_not_offered() {
    let conn = open_memory().unwrap();
    open(&conn, "OPN-1");
    let b = block_on(&conn, DAY, None, None);
    event_in(&conn, b, "OPN-1 and GHOST-9 and FINDING-01", None, None);
    // catches taking every regex hit instead of filtering to open tickets
    assert_eq!(options(&conn, b), ["OPN-1"]);
}

#[test]
fn a_block_with_no_sources_has_no_options() {
    let conn = open_memory().unwrap();
    let b = block_on(&conn, DAY, None, None);
    event_in(&conn, b, "nothing here", Some(&folder_path("acme")), None);
    assert!(options(&conn, b).is_empty());
}

#[test]
fn recent_tickets_stop_at_the_thirty_day_window() {
    let conn = open_memory().unwrap();
    past(&conn, "2026-03-22", "IN-1", "acme"); // 30th day counting today: in
    past(&conn, "2026-03-21", "OUT-1", "acme"); // one day older: out
    let b = block_on(&conn, DAY, None, None);
    event_in(&conn, b, "work", Some(&folder_path("acme")), None);
    // catches a window of 31 days (> for >=) and one of 29
    assert_eq!(options(&conn, b), ["IN-1"]);
}

#[test]
fn recent_tickets_come_only_from_the_same_project_root() {
    let conn = open_memory().unwrap();
    past(&conn, "2026-04-15", "SAME-1", "acme");
    past(&conn, "2026-04-15", "OTHER-1", "acme-2"); // shares a prefix
    past(&conn, "2026-04-15", "OTHER-2", "zeta");
    let b = block_on(&conn, DAY, None, None);
    // A worktree path collapses to its project root.
    let wt = format!("{}/.claude/worktrees/feat", folder_path("acme"));
    event_in(&conn, b, "work", Some(&wt), None);
    // catches starts_with with no separator and comparing the raw path
    assert_eq!(options(&conn, b), ["SAME-1"]);
}

#[test]
fn recent_tickets_skip_personal_blocks_the_block_itself_and_later_days() {
    let conn = open_memory().unwrap();
    past(&conn, "2026-04-21", "FUT-1", "acme"); // tomorrow
    let personal = block_on(&conn, "2026-04-15", Some("PER-1"), Some("manual"));
    event_in(&conn, personal, "p", Some(&folder_path("acme")), None);
    conn.execute(
        "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
        [personal],
    )
    .unwrap();
    let b = block_on(&conn, DAY, Some("SELF-1"), Some("auto"));
    event_in(&conn, b, "work", Some(&folder_path("acme")), None);
    assert!(options(&conn, b).is_empty());
}

struct Stub {
    answer: Option<Ranking>,
    fail: bool,
    calls: Cell<u32>,
}

impl Stub {
    fn says(top: &str, p: f64, second: f64, abstain: f64, agreed: bool) -> Self {
        let ranking = vec![
            RankedOption {
                id: top.into(),
                probability: p,
            },
            RankedOption {
                id: "OTH-1".into(),
                probability: second,
            },
        ];
        Stub {
            answer: Some(Ranking {
                ranking,
                abstain,
                agreed,
            }),
            fail: false,
            calls: Cell::new(0),
        }
    }

    fn mute(fail: bool) -> Self {
        Stub {
            answer: None,
            fail,
            calls: Cell::new(0),
        }
    }
}

impl Classifier for Stub {
    fn classify(
        &self,
        _state: &serde_json::Value,
        _options: &[String],
        _examples: &std::collections::BTreeMap<String, Vec<String>>,
    ) -> anyhow::Result<Option<Ranking>> {
        self.calls.set(self.calls.get() + 1);
        if self.fail {
            anyhow::bail!("helper down");
        }
        Ok(self.answer.clone())
    }
}

/// A block whose events name VIT-212 and OTH-1 (both open), origin as given.
fn askable(conn: &Connection, origin: Option<&str>) -> i64 {
    open(conn, "VIT-212");
    open(conn, "OTH-1");
    let b = block_on(conn, DAY, None, origin);
    event_in(conn, b, "VIT-212-fix OTH-1", None, None);
    b
}

fn pick_for(conn: &Connection, id: i64, c: &Stub) -> Option<String> {
    pick(conn, c, &block(conn, id), rule()).unwrap()
}

#[test]
fn a_clear_answer_picks_the_ticket_and_logs_the_guess() {
    let conn = open_memory().unwrap();
    let b = askable(&conn, None);
    let c = Stub::says("VIT-212", 0.6, 0.1, 0.1, true);
    assert_eq!(pick_for(&conn, b, &c).as_deref(), Some("VIT-212"));
    let rows = list_since(&conn, DecisionKind::Ticket, "").unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.source, DecisionSource::Verdict);
    assert_eq!(r.subject, b.to_string());
    assert_eq!(r.chosen.as_deref(), Some("VIT-212"));
    assert_eq!(r.options, ["VIT-212", "OTH-1"]);
    assert!(r.ranking.is_some());
}

#[test]
fn an_unsure_answer_picks_nothing_but_is_still_logged_once() {
    let conn = open_memory().unwrap();
    let b = askable(&conn, None);
    let c = Stub::says("VIT-212", 0.5, 0.49, 0.01, true); // runner-up too close
    assert_eq!(pick_for(&conn, b, &c), None);
    assert_eq!(pick_for(&conn, b, &c), None);
    let rows = list_since(&conn, DecisionKind::Ticket, "").unwrap();
    // one row after two runs: catches an append per run
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].chosen, None);
}

#[test]
fn each_filing_condition_must_hold_at_the_call_site() {
    let conn = open_memory().unwrap();
    let b = askable(&conn, None);
    // order check failed: catches ignoring `agreed`
    let flipped = Stub::says("VIT-212", 0.8, 0.1, 0.1, false);
    assert_eq!(pick_for(&conn, b, &flipped), None);
    // top not offered: catches trusting a made-up key
    let made_up = Stub::says("MADE-9", 0.8, 0.1, 0.1, true);
    assert_eq!(pick_for(&conn, b, &made_up), None);
    // abstain too high: catches skipping the abstain margin
    let abstains = Stub::says("VIT-212", 0.5, 0.1, 0.45, true);
    assert_eq!(pick_for(&conn, b, &abstains), None);
}

#[test]
fn a_dead_or_silent_helper_picks_nothing() {
    let conn = open_memory().unwrap();
    let b = askable(&conn, None);
    // an error must read as "no pick", not propagate or fall through to a key
    assert_eq!(pick_for(&conn, b, &Stub::mute(true)), None);
    assert_eq!(pick_for(&conn, b, &Stub::mute(false)), None);
}

#[test]
fn owner_and_event_tickets_are_never_asked_about_and_auto_or_unset_are() {
    let conn = open_memory().unwrap();
    let c = Stub::says("VIT-212", 0.6, 0.1, 0.1, true);
    for origin in ["manual", "event"] {
        let b = askable(&conn, Some(origin));
        assert_eq!(pick_for(&conn, b, &c), None, "{origin}");
    }
    assert_eq!(c.calls.get(), 0); // locked blocks are not even sent to Verdict
    for origin in [Some("auto"), None] {
        let b = askable(&conn, origin);
        assert_eq!(
            pick_for(&conn, b, &c).as_deref(),
            Some("VIT-212"),
            "{origin:?}"
        );
    }
}

#[test]
fn no_options_means_no_call() {
    let conn = open_memory().unwrap();
    let b = block_on(&conn, DAY, None, None);
    let c = Stub::says("VIT-212", 0.6, 0.1, 0.1, true);
    assert_eq!(pick_for(&conn, b, &c), None);
    assert_eq!(c.calls.get(), 0);
}

#[test]
fn apply_sets_auto_picks_and_leaves_locked_synced_and_personal_blocks() {
    let conn = open_memory().unwrap();
    let c = Stub::says("VIT-212", 0.6, 0.1, 0.1, true);
    let auto = askable(&conn, Some("auto"));
    conn.execute(
        "UPDATE blocks SET jira_issue = 'OTH-1' WHERE id = ?1",
        [auto],
    )
    .unwrap();
    let manual = askable(&conn, Some("manual"));
    let synced = askable(&conn, None);
    conn.execute(
        "UPDATE blocks SET tempo_worklog_id = 'TW-1' WHERE id = ?1",
        [synced],
    )
    .unwrap();
    let personal = askable(&conn, None);
    conn.execute(
        "UPDATE blocks SET is_personal = 1 WHERE id = ?1",
        [personal],
    )
    .unwrap();

    assert_eq!(apply(&conn, &c, DAY.parse().unwrap(), rule()).unwrap(), 1);

    let got = block(&conn, auto);
    assert_eq!(got.jira_issue.as_deref(), Some("VIT-212"));
    assert_eq!(got.ticket_origin, Some(TicketOrigin::Auto));
    for id in [manual, synced, personal] {
        assert_eq!(block(&conn, id).jira_issue, None, "block {id}");
    }
    assert_eq!(
        block(&conn, manual).ticket_origin,
        Some(TicketOrigin::Manual)
    );
}

#[test]
fn apply_leaves_the_current_ticket_when_verdict_is_unsure() {
    let conn = open_memory().unwrap();
    let b = askable(&conn, Some("auto"));
    conn.execute("UPDATE blocks SET jira_issue = 'OTH-1' WHERE id = ?1", [b])
        .unwrap();
    let c = Stub::says("VIT-212", 0.5, 0.49, 0.01, true);
    assert_eq!(apply(&conn, &c, DAY.parse().unwrap(), rule()).unwrap(), 0);
    assert_eq!(block(&conn, b).jira_issue.as_deref(), Some("OTH-1"));
}

#[test]
fn an_owner_ticket_swap_is_logged_with_the_ticket_it_replaced() {
    let conn = open_memory().unwrap();
    let b = block_on(&conn, DAY, Some("OLD-1"), Some("auto"));
    record_swap(&conn, b, Some("OLD-1".into()), Some("NEW-1")).unwrap();
    record_swap(&conn, b, Some("NEW-1".into()), Some("NEW-1")).unwrap(); // not a change
    let rows = list_since(&conn, DecisionKind::Ticket, "").unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.source, DecisionSource::Owner);
    assert_eq!(r.subject, b.to_string());
    assert_eq!(r.previous.as_deref(), Some("OLD-1"));
    assert_eq!(r.chosen.as_deref(), Some("NEW-1"));
}
