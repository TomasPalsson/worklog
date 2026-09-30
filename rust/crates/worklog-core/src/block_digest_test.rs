use super::*;
use crate::clues_contract::RawRecord;
use crate::db;
use crate::digest_contract::*;
use rusqlite::params;

const DAY: &str = "2026-04-18";

fn seed_block(conn: &Connection, is_personal: bool, jira_issue: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, is_personal, jira_issue)
         VALUES (?1, '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, ?2, ?3)",
        params![DAY, is_personal, jira_issue],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(
    conn: &Connection,
    block_id: i64,
    source: &str,
    minute: usize,
    title: &str,
    tweak: impl FnOnce(&mut Event),
) -> i64 {
    let mut event = Event::minimal(
        source,
        format!("{source}-{block_id}-{minute}-{title}"),
        format!("2026-04-18T09:{minute:02}:00+00:00"),
        title,
    );
    tweak(&mut event);
    let event_id = crate::repo::upsert_event(conn, &event).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();
    event_id
}

fn raw(record: &RawRecord) -> Option<String> {
    Some(serde_json::to_string(record).unwrap())
}

fn token() -> String {
    format!("ghp_{}", "A1b2".repeat(9))
}

fn long_title(index: usize) -> String {
    format!("{index}-{}", "y".repeat(130))
}

/// 11 commits: titles 0..9 plus a repeat of title 0, paths p0..p4 with p2 seen 3 times.
fn seed_commit_block(conn: &Connection) -> i64 {
    let block_id = seed_block(conn, false, None);
    for index in 0..11 {
        let title_index = if index == 10 { 0 } else { index };
        let path_index = if index == 10 { 2 } else { index % 5 };
        seed_event(
            conn,
            block_id,
            "github_commit",
            index,
            &long_title(title_index),
            |event| {
                event.repo = Some("acme/api".to_string());
                event.project_path = Some(format!("/tmp/p{path_index}"));
                event.session_id = Some(if index < 4 { "s1" } else { "s2" }.to_string());
            },
        );
    }
    block_id
}

#[test]
fn horizon_is_ninety_days_back() {
    let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    assert_eq!(horizon(today), NaiveDate::from_ymd_opt(2026, 7, 2).unwrap());
}

#[test]
fn card_matches_eval_evidence_and_caps() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_commit_block(&conn);
    let events = crate::repo::list_events_for_block(&conn, block_id).unwrap();

    let card = build_digest(&conn, block_id).unwrap();

    let expected_titles: Vec<String> = (0..MAX_EVAL_TITLES)
        .map(|index| long_title(index).chars().take(EVAL_TITLE_CHARS).collect())
        .collect();
    assert_eq!(card.eval_repos, vec!["acme/api".to_string()]);
    assert_eq!(card.eval_titles, expected_titles);
    assert_eq!(
        (card.eval_repos.clone(), card.eval_titles.clone()),
        eval_evidence(&events)
    );

    assert_eq!(card.paths, vec!["/tmp/p2", "/tmp/p0", "/tmp/p1", "/tmp/p3"]);
    assert_eq!(card.project_path.as_deref(), Some("/tmp/p2"));
    assert_eq!(
        card.folder,
        crate::billing::work_folder_for_block(&conn, block_id).unwrap()
    );
    let expected_invoice: Vec<String> = (0..MAX_INVOICE_TITLES)
        .map(|index| long_title(index).chars().take(INVOICE_TITLE_CHARS).collect())
        .collect();
    assert_eq!(card.invoice_titles, expected_invoice);

    assert_eq!(card.event_count, 11);
    assert_eq!(card.events_by_source.get("github_commit"), Some(&11));
    assert_eq!(card.session_count, 2);
    assert_eq!(card.active_minutes, 30);
    assert_eq!(card.first_at.as_deref(), Some("2026-04-18T09:00:00+00:00"));
    assert_eq!(card.last_at.as_deref(), Some("2026-04-18T09:10:00+00:00"));

    assert_eq!(card.change_titles.len(), MAX_CHANGE_TITLES);
    assert_eq!(card.change_titles[0].chars().count(), CHANGE_TITLE_CHARS);
    assert_eq!(card.pinned_customer, None);
}

#[test]
fn stored_card_round_trips_and_marks_the_day_compressed() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_commit_block(&conn);
    assert!(!day_is_compressed(&conn, DAY).unwrap());
    assert_eq!(digest_for_block(&conn, block_id).unwrap(), None);

    let card = build_digest(&conn, block_id).unwrap();
    assert!(write_digest(&conn, block_id, &card).unwrap());
    assert!(!write_digest(&conn, block_id, &BlockDigest::default()).unwrap());

    assert_eq!(digest_for_block(&conn, block_id).unwrap(), Some(card));
    assert!(day_is_compressed(&conn, DAY).unwrap());
    assert!(!day_is_compressed(&conn, "2026-04-19").unwrap());
}

#[test]
fn undecodable_card_reads_as_missing() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_block(&conn, false, None);
    conn.execute(
        "INSERT INTO block_digest (block_id, version, built_at, json) VALUES (?1, 1, 'now', '{not json')",
        [block_id],
    )
    .unwrap();
    assert_eq!(digest_for_block(&conn, block_id).unwrap(), None);
}

fn seed_evidence(conn: &Connection, block_id: i64) {
    let secret = token();
    let prompt = format!("{} {secret}", "a".repeat(190));
    seed_event(conn, block_id, "claude_turn", 1, "turn", |event| {
        event.raw_json = raw(&RawRecord::ClaudePrompt {
            session_id: "s1".to_string(),
            text: prompt,
        });
    });
    seed_event(
        conn,
        block_id,
        "github_commit",
        2,
        &format!("Rotate {secret}"),
        |_| {},
    );
    seed_event(conn, block_id, "claude_work", 3, "work", |event| {
        event.details = Some(format!("branch feat/{secret}"));
    });
    seed_event(conn, block_id, "claude_tool", 4, "tool", |event| {
        event.raw_json = raw(&RawRecord::ClaudeTool {
            session_id: "s1".to_string(),
            tool: "Edit".to_string(),
            input: serde_json::json!({}),
            output: None,
            output_cut_bytes: 0,
            files: vec![format!("/src/{secret}.txt")],
        });
    });
}

#[test]
fn personal_and_secrets() {
    let conn = db::open_memory().unwrap();

    let work = seed_block(&conn, false, None);
    seed_evidence(&conn, work);
    let card = build_digest(&conn, work).unwrap();
    for text in card
        .prompts
        .iter()
        .chain(&card.change_titles)
        .chain(&card.branches)
        .chain(&card.files)
    {
        assert!(!text.contains("ghp"), "leaked a token fragment: {text}");
    }
    assert_eq!(card.prompts.len(), 1);
    assert_eq!(card.prompt_count, 1);
    assert_eq!(card.change_titles.len(), 1);
    assert_eq!(card.branches.len(), 1);
    assert_eq!(card.files.len(), 1);
    assert_eq!(card.files_distinct, 1);
    assert_eq!(card.tool_counts.get("Edit"), Some(&1));

    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('ABC-1', 'Fix the thing')",
        [],
    )
    .unwrap();
    let personal = seed_block(&conn, true, Some("ABC-1"));
    seed_evidence(&conn, personal);
    let card = build_digest(&conn, personal).unwrap();
    assert!(card.prompts.is_empty());
    assert_eq!(card.prompt_count, 0);
    assert!(card.change_titles.is_empty());
    assert!(card.branches.is_empty());
    assert!(card.files.is_empty());
    assert_eq!(card.files_distinct, 0);
    assert!(card.tool_counts.is_empty());
    assert_eq!(card.jira_summary.as_deref(), Some("Fix the thing"));
}

#[test]
fn eval_titles_stay_unscrubbed() {
    let conn = db::open_memory().unwrap();
    let block_id = seed_block(&conn, false, None);
    let title = format!("Rotate {}", token());
    seed_event(&conn, block_id, "github_commit", 1, &title, |_| {});
    let card = build_digest(&conn, block_id).unwrap();
    assert_eq!(card.eval_titles, vec![title]);
}
