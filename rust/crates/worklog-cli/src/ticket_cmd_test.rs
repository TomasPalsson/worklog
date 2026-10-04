use super::*;
use crate::cli::Cli;
use clap::Parser;
use worklog_core::jira_assist_contract::HintReason;
use worklog_core::tempo_hub_contract::{StatusCategory, TicketDetail};

fn detail(status: &str) -> TicketDetail {
    serde_json::from_value(json!({
        "key": "GENAI-7", "summary": "Do thing", "status": status,
        "status_category": null, "issue_type": null, "priority": null,
        "assignee": null, "updated": null, "url": "https://j/browse/GENAI-7",
        "description": "body text", "comments": []
    }))
    .unwrap()
}

fn render<F: FnOnce(&mut Vec<u8>) -> Result<()>>(f: F) -> String {
    let mut buf = Vec::new();
    f(&mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

#[test]
fn every_verb_parses() {
    for args in [
        "ticket get GENAI-1",
        "ticket start GENAI-1",
        "ticket find some words",
        "ticket move GENAI-1 Done",
        "ticket hints",
        "account allowed",
        "account suggest hello",
        "account relearn",
    ] {
        let mut argv = vec!["worklog"];
        argv.extend(args.split(' '));
        // `find some words` takes one quoted arg in real use.
        if args.starts_with("ticket find") {
            argv = vec!["worklog", "ticket", "find", "some words"];
        }
        Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{args}: {e}"));
    }
}

#[test]
fn create_parses_repeated_clues_and_guessed() {
    let cli = Cli::try_parse_from([
        "worklog",
        "ticket",
        "create",
        "--summary",
        "S",
        "--description-file",
        "d.md",
        "--account",
        "42",
        "--guessed",
        "7",
        "--clue",
        "a",
        "--clue",
        "b",
    ])
    .unwrap();
    let crate::cli::Cmd::Ticket {
        sub:
            TicketCmd::Create {
                guessed,
                clues,
                account,
                ..
            },
    } = cli.command
    else {
        panic!("not ticket create");
    };
    assert_eq!(guessed.as_deref(), Some("7"));
    assert_eq!(account, "42");
    assert_eq!(clues, ["a", "b"]);
}

#[test]
fn guessed_flag_carries_a_wrong_guess() {
    let b = create_body(
        "S".into(),
        "D".into(),
        "42".into(),
        Some("7".into()),
        vec!["x".into()],
    );
    assert_eq!(b.guessed_account_id.as_deref(), Some("7"));
    assert_eq!(b.account_id, "42");
    assert_eq!(b.clues, ["x"]);
    let b = create_body("S".into(), "D".into(), "42".into(), None, vec![]);
    assert_eq!(b.guessed_account_id, None);
}

#[test]
fn view_text_shows_key_status_account_description() {
    let v = TicketView {
        detail: detail("To Do"),
        account_id: Some("9".into()),
        account_name: Some("Acme".into()),
    };
    let s = render(|o| render_view(o, &v));
    assert!(s.contains("GENAI-7  Do thing"), "{s}");
    assert!(s.contains("status:  To Do"), "{s}");
    assert!(s.contains("account: Acme"), "{s}");
    assert!(s.contains("body text"), "{s}");
}

#[test]
fn start_text_names_each_outcome() {
    let mk = |outcome| StartResult {
        view: TicketView {
            detail: detail("In Progress"),
            account_id: None,
            account_name: None,
        },
        outcome,
    };
    assert!(render(|o| render_start(o, &mk(StartOutcome::Moved))).contains("GENAI-7 started"));
    assert!(
        render(|o| render_start(o, &mk(StartOutcome::AlreadyStarted))).contains("already started")
    );
    assert!(
        render(|o| render_start(o, &mk(StartOutcome::NotWritable))).contains("not a GENAI ticket")
    );
}

#[test]
fn find_and_suggestions_and_hints_render_rows() {
    let hit = JiraTicket {
        key: "ABC-1".into(),
        summary: "Found".into(),
        status: Some("Done".into()),
        project_key: None,
        updated: None,
        issue_id: None,
    };
    assert_eq!(
        render(|o| render_find(o, &[hit])).trim(),
        "ABC-1  [Done]  Found"
    );
    let sug = AccountSuggestion {
        account: AllowedAccount {
            id: "9".into(),
            name: "Acme".into(),
        },
        matched_clues: vec!["x".into(), "y".into()],
        past_tickets: 3,
        score: 1.5,
    };
    assert_eq!(
        render(|o| render_suggestions(o, &[sug])).trim(),
        "9  Acme  score 1.50  (3 past tickets; clues: x, y)"
    );
    let hint = StatusHint {
        key: "GENAI-7".into(),
        summary: "Do thing".into(),
        to_category: StatusCategory::Done,
        reason: HintReason::PrMerged {
            repo: "r".into(),
            number: 1,
            merged_at: "t".into(),
        },
    };
    let s = render(|o| render_hints(o, &[hint]));
    assert!(s.starts_with("GENAI-7  Do thing  -> "), "{s}");
}
