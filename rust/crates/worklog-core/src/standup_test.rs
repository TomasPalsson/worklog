use super::*;
use crate::db;
use crate::models::{Event, JiraTicket};
use crate::tempo_hub_contract::StatusCategory;
use rusqlite::params;
use std::cell::RefCell;

const TODAY: &str = "2026-10-06";
const YESTERDAY: &str = "2026-10-05";
const BEARER_BODY: &str = "abcdef0123456789abcdef0123456789";

fn today() -> NaiveDate {
    TODAY.parse().unwrap()
}

struct Recorder {
    reply: Result<Value, String>,
    inputs: RefCell<Vec<String>>,
}

impl Recorder {
    fn replying(reply: Value) -> Self {
        Self {
            reply: Ok(reply),
            inputs: RefCell::default(),
        }
    }

    fn failing() -> Self {
        Self {
            reply: Err("model down".into()),
            inputs: RefCell::default(),
        }
    }

    fn input(&self) -> String {
        self.inputs.borrow().last().cloned().unwrap()
    }
}

impl ModelInvoker for Recorder {
    fn invoke(&self, _system: &str, user: &str, _schema: &Value, _model: &str) -> Result<Value> {
        self.inputs.borrow_mut().push(user.to_string());
        self.reply.clone().map_err(|e| anyhow::anyhow!(e))
    }
}

fn reply() -> Value {
    json!({"today": ["a"], "next": ["b"], "blockers": ["c"]})
}

fn run(conn: &Connection) -> String {
    let model = Recorder::replying(reply());
    draft(conn, today(), &model).unwrap();
    model.input()
}

fn block(conn: &Connection, day: &str, ticket: &str, description: &str, personal: bool) {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, jira_issue, description, is_personal)
         VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00', 1800, ?2, ?3, ?4)",
        params![day, ticket, description, personal],
    )
    .unwrap();
}

fn event(conn: &Connection, source: &str, id: &str, day: &str, tweak: impl FnOnce(&mut Event)) {
    let mut e = Event::minimal(source, id, format!("{day}T10:00:00+00:00"), id);
    tweak(&mut e);
    let row = repo::upsert_event(conn, &e).unwrap();
    // upsert_event scrubs at storage time; write raw values so the draft's own scrubbing is tested.
    conn.execute(
        "UPDATE events SET title = ?1, details = ?2, raw_json = ?3 WHERE id = ?4",
        params![e.title, e.details, e.raw_json, row],
    )
    .unwrap();
}

fn prompt(conn: &Connection, session: &str, day: &str, text: &str) {
    event(
        conn,
        "claude",
        &format!("{session}-{day}-{}", text.len()),
        day,
        |e| {
            e.session_id = Some(session.into());
            e.details = Some(text.into());
        },
    );
}

fn pr(conn: &Connection, title: &str, merged_at: Option<&str>) {
    event(conn, "github_pr", title, YESTERDAY, |e| {
        e.raw_json = Some(
            serde_json::to_string(&RawRecord::Commit {
                sha: String::new(),
                body: String::new(),
                local_folder: None,
                merged_at: merged_at.map(str::to_owned),
            })
            .unwrap(),
        );
    });
}

fn ticket(conn: &Connection, key: &str, summary: &str, status: &str, category: StatusCategory) {
    repo::upsert_ticket(
        conn,
        &JiraTicket {
            key: key.into(),
            summary: summary.into(),
            status: Some(status.into()),
            project_key: None,
            updated: None,
            issue_id: None,
        },
    )
    .unwrap();
    repo::set_ticket_status(conn, key, status, Some(category)).unwrap();
}

fn api_key() -> String {
    format!("ghp_{}", "A1b2".repeat(9))
}

fn bearer() -> String {
    format!("Bearer {BEARER_BODY}")
}

#[test]
fn draft_returns_the_models_three_answers_in_order() {
    let conn = db::open_memory().unwrap();
    let model = Recorder::replying(json!({"today": ["t1", "t2"], "next": ["n"], "blockers": []}));
    let got = draft(&conn, today(), &model).unwrap();
    // catches: lists swapped or merged
    assert_eq!(got.today, vec!["t1", "t2"]);
    assert_eq!(got.next, vec!["n"]);
    assert!(got.blockers.is_empty());
}

#[test]
fn reply_missing_a_list_is_an_error() {
    let conn = db::open_memory().unwrap();
    let model = Recorder::replying(json!({"today": ["t"], "next": ["n"]}));
    // catches: defaulting a missing list to empty instead of failing the whole draft
    assert!(draft(&conn, today(), &model).is_err());
}

#[test]
fn model_failure_propagates() {
    let conn = db::open_memory().unwrap();
    let err = draft(&conn, today(), &Recorder::failing()).unwrap_err();
    // catches: swallowing the invoker error into an empty draft
    assert!(err.to_string().contains("model down"));
}

#[test]
fn planted_secrets_never_reach_the_model() {
    let conn = db::open_memory().unwrap();
    block(
        &conn,
        TODAY,
        "GENAI-1",
        &format!("deploy with {}", api_key()),
        false,
    );
    block(
        &conn,
        YESTERDAY,
        "GENAI-1",
        &format!("curl -H \"Authorization: {}\"", bearer()),
        false,
    );
    ticket(
        &conn,
        "GENAI-2",
        &format!("rotate {}", api_key()),
        "In Progress",
        StatusCategory::Indeterminate,
    );
    pr(
        &conn,
        &format!("PR #1: fix {}", bearer()),
        Some("2026-10-05T12:00:00Z"),
    );
    prompt(
        &conn,
        "s1",
        TODAY,
        &format!("use {} and {}", api_key(), bearer()),
    );
    let input = run(&conn);
    // catches: scrubbing only some of the sources
    assert!(!input.contains(&api_key()));
    assert!(!input.contains(BEARER_BODY));
    assert!(input.contains("GENAI-1"));
}

#[test]
fn first_prompt_is_cut_at_exactly_300_chars() {
    let conn = db::open_memory().unwrap();
    prompt(&conn, "keep", TODAY, &format!("{}W", "y".repeat(299)));
    prompt(&conn, "cut", TODAY, &format!("{}Z", "x".repeat(300)));
    let input = run(&conn);
    // catches: cutting at 299 (> for >=)
    assert!(input.contains(&format!("{}W", "y".repeat(299))));
    // catches: no cut, or a cut at 301
    assert!(input.contains(&"x".repeat(300)));
    assert!(!input.contains('Z'));
}

#[test]
fn only_the_first_prompt_of_each_session_is_used() {
    let conn = db::open_memory().unwrap();
    prompt(&conn, "s1", YESTERDAY, "first-of-s1");
    prompt(&conn, "s1", TODAY, "second-of-s1");
    prompt(&conn, "s2", TODAY, "first-of-s2");
    let input = run(&conn);
    // catches: every prompt included
    assert!(!input.contains("second-of-s1"));
    // catches: only one session included, or yesterday dropped
    assert!(input.contains("first-of-s1"));
    assert!(input.contains("first-of-s2"));
}

#[test]
fn yesterday_without_blocks_says_so() {
    let conn = db::open_memory().unwrap();
    block(&conn, TODAY, "GENAI-9", "today work", false);
    let input = run(&conn);
    // catches: an empty Yesterday section with no marker for the model
    assert!(input.contains("Yesterday's blocks (2026-10-05):\n- none recorded"));
    assert!(input.contains("GENAI-9: today work"));
}

#[test]
fn only_yesterdays_and_todays_non_personal_blocks_are_used() {
    let conn = db::open_memory().unwrap();
    block(&conn, "2026-10-04", "OLD-1", "two days ago", false);
    block(&conn, "2026-10-07", "NEW-1", "tomorrow", false);
    block(&conn, TODAY, "PRIV-1", "dentist", true);
    block(&conn, YESTERDAY, "GENAI-5", "yesterday work", false);
    let input = run(&conn);
    // catches: a window too wide on either side
    assert!(!input.contains("two days ago"));
    assert!(!input.contains("tomorrow"));
    // catches: personal blocks leaking
    assert!(!input.contains("dentist"));
    assert!(input.contains("GENAI-5: yesterday work"));
}

#[test]
fn only_prs_merged_yesterday_or_today_are_used() {
    let conn = db::open_memory().unwrap();
    pr(
        &conn,
        "PR #1: merged-yesterday",
        Some("2026-10-05T23:59:59Z"),
    );
    pr(&conn, "PR #2: merged-today", Some("2026-10-06T00:00:00Z"));
    pr(&conn, "PR #3: still-open", None);
    pr(&conn, "PR #4: merged-earlier", Some("2026-10-04T23:59:59Z"));
    let input = run(&conn);
    // catches: window boundaries off by one day
    assert!(input.contains("merged-yesterday"));
    assert!(input.contains("merged-today"));
    assert!(!input.contains("merged-earlier"));
    // catches: treating every PR event as merged
    assert!(!input.contains("still-open"));
}

#[test]
fn a_pr_stored_compressed_is_read_not_an_error() {
    let conn = db::open_memory().unwrap();
    // upsert_event stores raw_json as a deflate BLOB, as on a real db (the pr() helper rewrites it as TEXT).
    let mut e = Event::minimal(
        "github_pr",
        "PR #9: packed",
        format!("{YESTERDAY}T10:00:00+00:00"),
        "PR #9: packed",
    );
    e.raw_json = Some(
        serde_json::to_string(&RawRecord::Commit {
            sha: String::new(),
            body: String::new(),
            local_folder: None,
            merged_at: Some("2026-10-05T12:00:00Z".into()),
        })
        .unwrap(),
    );
    repo::upsert_event(&conn, &e).unwrap();
    // catches: reading raw_json with row.get::<String>, which fails "Invalid column type Blob"
    assert!(run(&conn).contains("PR #9: packed"));
}

#[test]
fn open_tickets_exclude_done_and_dead_statuses() {
    let conn = db::open_memory().unwrap();
    ticket(
        &conn,
        "T-1",
        "in-flight",
        "In Progress",
        StatusCategory::Indeterminate,
    );
    ticket(&conn, "T-2", "finished", "Done", StatusCategory::Done);
    ticket(&conn, "T-3", "parked", "Backlog", StatusCategory::New);
    let input = run(&conn);
    assert!(input.contains("T-1 in-flight [In Progress]"));
    // catches: listing done tickets
    assert!(!input.contains("finished"));
    // catches: listing dead statuses
    assert!(!input.contains("parked"));
}

#[test]
fn ticket_description_reaches_the_model_on_one_line_scrubbed_and_cut() {
    let conn = db::open_memory().unwrap();
    ticket(&conn, "T-1", "errors", "To Do", StatusCategory::New);
    let description = format!(
        "Users see\n\ntimeouts. key {} {}END",
        api_key(),
        "d".repeat(DESCRIPTION_CHARS)
    );
    repo::set_ticket_details(
        &conn,
        "T-1",
        &repo::TicketDetails {
            description: Some(description),
            ..Default::default()
        },
    )
    .unwrap();
    let input = run(&conn);
    // catches: description dropped, or its newlines breaking the bullet list
    assert!(input.contains("T-1 errors [To Do]: Users see timeouts. key"));
    // catches: a secret in the description reaching the model
    assert!(!input.contains(&api_key()));
    // catches: no cut on a long description
    assert!(!input.contains("END"));
}

#[test]
fn regenerate_sends_previous_draft_and_a_rewording_instruction() {
    let conn = db::open_memory().unwrap();
    let previous = StandupDraft {
        today: vec![format!("PREVIOUS-BULLET {}", api_key())],
        next: vec![],
        blockers: vec![],
    };
    let model = Recorder::replying(reply());
    draft(&conn, today(), &model).unwrap();
    // catches: the instruction leaking into the first draft
    assert!(!model.input().contains(REWORD_INSTRUCTION));
    regenerate(&conn, today(), &model, &previous).unwrap();
    let input = model.input();
    // catches: previous draft not sent, or instruction missing
    assert!(input.contains("PREVIOUS-BULLET"));
    assert!(input.contains(REWORD_INSTRUCTION));
    // catches: an edited previous draft bypassing the secret scrub
    assert!(!input.contains(&api_key()));
}
