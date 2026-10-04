use super::*;
use crate::db;
use crate::http;
use httpmock::prelude::*;
use serde_json::json;

const FIELD: &str = "customfield_10100";

fn auth(server: &MockServer) -> JiraAuth {
    JiraAuth {
        base_url: server.base_url(),
        email: "a@b.c".into(),
        token: "tok".into(),
    }
}

fn issue_body(status: &str, category: &str) -> serde_json::Value {
    json!({ "fields": {
        "summary": "Do the thing",
        "status": { "name": status, "statusCategory": { "key": category } },
        "issuetype": { "name": "Story" }
    }})
}

/// Serves the account read, then the detail read, for `key`.
fn mock_ticket(server: &MockServer, key: &str, status: &str, category: &str) {
    let path = format!("/rest/api/3/issue/{key}");
    server.mock(|when, then| {
        when.method(GET).path(&path).query_param("fields", FIELD);
        then.status(200)
            .json_body(json!({ "fields": { FIELD: { "id": 42, "name": "Acme" } } }));
    });
    server.mock(|when, then| {
        when.method(GET).path(&path);
        then.status(200).json_body(issue_body(status, category));
    });
}

fn mock_transitions(server: &MockServer, key: &str) {
    server.mock(|when, then| {
        when.method(GET)
            .path(format!("/rest/api/3/issue/{key}/transitions"));
        then.status(200).json_body(json!({ "transitions": [
            { "id": "11", "name": "Start", "to": { "name": "In Progress", "statusCategory": { "key": "indeterminate" } } },
            { "id": "21", "name": "Finish", "to": { "name": "Done", "statusCategory": { "key": "done" } } },
            { "id": "31", "name": "Block", "to": { "name": "Blocked", "statusCategory": { "key": "indeterminate" } } }
        ]}));
    });
}

fn mock_post_transition<'a>(server: &'a MockServer, key: &str, id: &str) -> httpmock::Mock<'a> {
    let id = id.to_owned();
    server.mock(move |when, then| {
        when.method(POST)
            .path(format!("/rest/api/3/issue/{key}/transitions"))
            .json_body(json!({ "transition": { "id": id } }));
        then.status(204);
    })
}

fn start(server: &MockServer, key: &str) -> Result<StartResult> {
    start_ticket_with(&auth(server), Some(FIELD), key, &http::client().unwrap())
}

#[test]
fn start_moves_a_todo_genai_ticket_to_in_progress() {
    let server = MockServer::start();
    mock_ticket(&server, "GENAI-1", "To Do", "new");
    mock_transitions(&server, "GENAI-1");
    let post = mock_post_transition(&server, "GENAI-1", "11");
    let got = start(&server, "GENAI-1").unwrap();
    post.assert_hits(1);
    assert_eq!(got.outcome, StartOutcome::Moved);
    assert_eq!(got.view.detail.status.as_deref(), Some("In Progress"));
    assert_eq!(got.view.account_id.as_deref(), Some("42"));
    assert_eq!(got.view.account_name.as_deref(), Some("Acme"));
}

#[test]
fn start_leaves_started_done_and_foreign_tickets_alone() {
    for (key, status, category, outcome) in [
        (
            "GENAI-2",
            "In Progress",
            "indeterminate",
            StartOutcome::AlreadyStarted,
        ),
        ("GENAI-3", "Done", "done", StartOutcome::AlreadyStarted),
        ("GOJ-4", "To Do", "new", StartOutcome::NotWritable),
    ] {
        let server = MockServer::start();
        mock_ticket(&server, key, status, category);
        mock_transitions(&server, key);
        let post = server.mock(|when, then| {
            when.method(POST);
            then.status(204);
        });
        let got = start(&server, key).unwrap();
        post.assert_hits(0);
        assert_eq!(got.outcome, outcome, "{key}");
        assert_eq!(got.view.detail.status.as_deref(), Some(status));
    }
}

#[test]
fn start_without_an_in_progress_transition_names_the_status_and_posts_nothing() {
    let server = MockServer::start();
    mock_ticket(&server, "GENAI-5", "To Do", "new");
    server.mock(|when, then| {
        when.method(GET).path("/rest/api/3/issue/GENAI-5/transitions");
        then.status(200).json_body(json!({ "transitions": [
            { "id": "21", "name": "Finish", "to": { "name": "Done", "statusCategory": { "key": "done" } } }
        ]}));
    });
    let post = server.mock(|when, then| {
        when.method(POST);
        then.status(204);
    });
    let err = format!("{:#}", start(&server, "GENAI-5").unwrap_err());
    post.assert_hits(0);
    assert!(
        err.contains("In Progress") && err.contains("To Do"),
        "{err}"
    );
}

#[test]
fn move_matches_the_target_status_case_insensitively() {
    let server = MockServer::start();
    mock_transitions(&server, "GENAI-6");
    let post = mock_post_transition(&server, "GENAI-6", "31");
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/GENAI-6")
            .query_param("fields", "status");
        then.status(200)
            .json_body(issue_body("Blocked", "indeterminate"));
    });
    let got = move_ticket_with(
        &auth(&server),
        "GENAI-6",
        "bLoCkEd",
        &http::client().unwrap(),
    )
    .unwrap();
    post.assert_hits(1);
    assert_eq!(
        got,
        TicketStatus {
            key: "GENAI-6".into(),
            status: "Blocked".into(),
            status_category: Some(StatusCategory::Indeterminate),
        }
    );
}

#[test]
fn move_refuses_a_foreign_key_without_calling_jira() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let err =
        move_ticket_with(&auth(&server), "GOJ-1", "Done", &http::client().unwrap()).unwrap_err();
    any.assert_hits(0);
    assert!(err.to_string().contains("GENAI"), "{err}");
}

#[test]
fn move_to_a_status_jira_does_not_offer_posts_nothing() {
    let server = MockServer::start();
    mock_transitions(&server, "GENAI-7");
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/GENAI-7")
            .query_param("fields", "status");
        then.status(200).json_body(issue_body("To Do", "new"));
    });
    let post = server.mock(|when, then| {
        when.method(POST);
        then.status(204);
    });
    let err = move_ticket_with(
        &auth(&server),
        "GENAI-7",
        "Archived",
        &http::client().unwrap(),
    )
    .unwrap_err();
    post.assert_hits(0);
    assert!(
        err.to_string()
            .contains("Jira offers no Archived transition from To Do"),
        "{err}"
    );
}

fn body() -> AssistCreateBody {
    AssistCreateBody {
        summary: "Innnes - SSO login".into(),
        description: "## Goal\n\n- one\n- two".into(),
        account_id: "42".into(),
        guessed_account_id: Some("7".into()),
        clues: vec!["innnes".into()],
        assignee_account_id: None,
    }
}

fn mock_createmeta(server: &MockServer, accounts: serde_json::Value) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes");
        then.status(200)
            .json_body(json!({ "issueTypes": [{ "id": "7", "name": "Story" }] }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/createmeta/GENAI/issuetypes/7");
        then.status(200).json_body(json!({ "fields": [
            { "fieldId": FIELD, "allowedValues": accounts }
        ]}));
    });
}

fn mock_create<'a>(server: &'a MockServer) -> httpmock::Mock<'a> {
    server.mock(|when, then| {
        when.method(POST).path("/rest/api/3/issue");
        then.status(201)
            .json_body(json!({ "id": "900", "key": "GENAI-900" }));
    })
}

fn create(
    server: &MockServer,
    conn: &Connection,
    body: &AssistCreateBody,
) -> Result<AssistCreated> {
    assist_create_with(conn, &auth(server), FIELD, body, &http::client().unwrap())
}

fn decision_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM account_decisions", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn create_sends_a_bare_number_account_then_moves_to_in_progress_and_logs() {
    let server = MockServer::start();
    mock_createmeta(&server, json!([{ "id": "42", "name": "Acme" }]));
    let created = server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue")
            .json_body_partial(
                json!({ "fields": {
                    "project": { "key": "GENAI" },
                    "issuetype": { "name": "Story" },
                    FIELD: 42
                }})
                .to_string(),
            );
        then.status(201)
            .json_body(json!({ "id": "900", "key": "GENAI-900" }));
    });
    mock_transitions(&server, "GENAI-900");
    let post = mock_post_transition(&server, "GENAI-900", "11");
    server.mock(|when, then| {
        when.method(GET)
            .path("/rest/api/3/issue/GENAI-900")
            .query_param("fields", "status");
        then.status(200)
            .json_body(issue_body("In Progress", "indeterminate"));
    });
    let conn = db::open_memory().unwrap();
    let got = create(&server, &conn, &body()).unwrap();
    created.assert_hits(1);
    post.assert_hits(1);
    assert_eq!(got.key, "GENAI-900");
    assert_eq!(got.url, format!("{}/browse/GENAI-900", server.base_url()));
    assert_eq!(got.status.as_deref(), Some("In Progress"));
    assert_eq!(
        got.account,
        AllowedAccount {
            id: "42".into(),
            name: "Acme".into()
        }
    );
    let (picked, guessed, correct): (String, String, i64) = conn
        .query_row(
            "SELECT picked_id, guessed_id, correct FROM account_decisions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((picked.as_str(), guessed.as_str(), correct), ("42", "7", 0));
}

#[test]
fn create_refuses_an_account_outside_the_fresh_list() {
    let server = MockServer::start();
    mock_createmeta(&server, json!([{ "id": "1", "name": "Other" }]));
    let created = mock_create(&server);
    let conn = db::open_memory().unwrap();
    let err = create(&server, &conn, &body()).unwrap_err();
    created.assert_hits(0);
    assert!(err.to_string().contains("42"), "{err}");
    assert_eq!(decision_rows(&conn), 0);
}

#[test]
fn create_refuses_when_the_allowed_list_is_empty_or_unavailable() {
    let empty = MockServer::start();
    mock_createmeta(&empty, json!([]));
    let down = MockServer::start();
    down.mock(|when, then| {
        when.method(GET);
        then.status(500).body("boom");
    });
    for server in [&empty, &down] {
        let created = mock_create(server);
        let conn = db::open_memory().unwrap();
        assert!(create(server, &conn, &body()).is_err());
        created.assert_hits(0);
        assert_eq!(decision_rows(&conn), 0);
    }
}

#[test]
fn create_refuses_emoji_in_title_or_description_before_any_call() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    let conn = db::open_memory().unwrap();
    for (summary, description) in [("Ship it \u{1F680}", "plain"), ("plain", "done \u{2705}")] {
        let mut b = body();
        b.summary = summary.into();
        b.description = description.into();
        let err = create(&server, &conn, &b).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("emoji"), "{err}");
    }
    any.assert_hits(0);
}

#[test]
fn failed_move_after_create_reports_the_key_and_never_retries_create() {
    let server = MockServer::start();
    mock_createmeta(&server, json!([{ "id": "42", "name": "Acme" }]));
    let created = mock_create(&server);
    mock_transitions(&server, "GENAI-900");
    server.mock(|when, then| {
        when.method(POST)
            .path("/rest/api/3/issue/GENAI-900/transitions");
        then.status(500).body("nope");
    });
    let conn = db::open_memory().unwrap();
    let err = format!("{:#}", create(&server, &conn, &body()).unwrap_err());
    created.assert_hits(1);
    assert!(err.contains("GENAI-900") && err.contains("nope"), "{err}");
    assert_eq!(decision_rows(&conn), 1);
}

#[test]
fn failed_decision_log_after_create_still_reports_the_key() {
    let server = MockServer::start();
    mock_createmeta(&server, json!([{ "id": "42", "name": "Acme" }]));
    let created = mock_create(&server);
    let conn = db::open_memory().unwrap();
    conn.execute("DROP TABLE account_decisions", []).unwrap();
    let err = format!("{:#}", create(&server, &conn, &body()).unwrap_err());
    created.assert_hits(1);
    assert!(err.contains("GENAI-900"), "{err}");
}
