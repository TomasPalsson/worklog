use super::*;
use crate::clues_contract::RawRecord;
use crate::db::open_memory;
use httpmock::prelude::*;
use serde_json::json;

fn auth(base: String) -> GitHubAuth {
    GitHubAuth {
        token: "ghp_test".into(),
        user: "TomasPalsson".into(),
        base,
    }
}

fn run(base: String) -> (Connection, CollectReport, Vec<Event>) {
    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(&conn, &auth(base), since, until, &http::client().unwrap()).unwrap();
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    (conn, report, events)
}

#[test]
fn collect_writes_commits_and_prs_with_jira_keys() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "abc123", "repository": {"full_name": "org/repo"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"},
                        "message": "PROJ-42 fix login bug\n\nlonger description"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": [
            {"id": 1001, "number": 12, "title": "Add dashboard for PROJ-100", "body": null,
             "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/org/repo"}
        ]}));
    });

    let (_conn, report, events) = run(server.base_url());
    assert_eq!(report.events_written, 2);
    assert_eq!(events.len(), 2);

    let commit = events.iter().find(|e| e.source == "github_commit").unwrap();
    assert_eq!(commit.source_id, "abc123");
    assert_eq!(commit.jira_issue.as_deref(), Some("PROJ-42"));
    assert_eq!(commit.repo.as_deref(), Some("org/repo"));

    let pr = events.iter().find(|e| e.source == "github_pr").unwrap();
    assert_eq!(pr.source_id, "1001");
    assert_eq!(pr.jira_issue.as_deref(), Some("PROJ-100"));
    assert_eq!(pr.repo.as_deref(), Some("org/repo"));
    assert!(pr.title.starts_with("PR #12:"));
}

#[test]
fn collect_is_idempotent_by_source_id() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "deadbeef", "repository": {"full_name": "o/r"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"}, "message": "hello"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    for _ in 0..2 {
        collect_with(
            &conn,
            &auth(server.base_url()),
            since,
            until,
            &http::client().unwrap(),
        )
        .unwrap();
    }
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events.len(),
        1,
        "dedupe on (source, source_id) must prevent duplicates"
    );
}

#[test]
fn personal_owner_is_skipped() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "personal1", "repository": {"full_name": "TomasPalsson/worklog"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"}, "message": "personal repo commit"}},
            {"sha": "org1", "repository": {"full_name": "aproorg/vitinn-infra"},
             "commit": {"author": {"date": "2026-04-18T09:05:00Z"}, "message": "org repo commit"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": [
            {"id": 1, "number": 1, "title": "personal PR", "body": null,
             "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/TomasPalsson/worklog"},
            {"id": 2, "number": 2, "title": "org PR", "body": null,
             "created_at": "2026-04-18T10:05:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/aproorg/vitinn-infra"}
        ]}));
    });

    let (_conn, report, events) = run(server.base_url());
    assert_eq!(
        report.events_written, 2,
        "only the two org-owned events should be written"
    );
    assert!(
        events
            .iter()
            .all(|e| e.repo.as_deref() != Some("TomasPalsson/worklog")),
        "personal-owner repo events must not be stored"
    );
    assert_eq!(events.len(), 2);
}

#[test]
fn unresolvable_org_commit_is_marked_elsewhere() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "nowhere1", "repository": {"full_name": "org/definitely-not-cloned-xyz"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"}, "message": "no local clone"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });

    let (conn, report, events) = run(server.base_url());
    assert_eq!(report.events_written, 1);
    let commit = events
        .iter()
        .find(|e| e.source_id == "nowhere1")
        .expect("commit event stored");
    assert_eq!(commit.project_path, None);

    let elsewhere: i64 = conn
        .query_row(
            "SELECT elsewhere FROM events WHERE source_id = ?1",
            ["nowhere1"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        elsewhere, 1,
        "commit with no local clone must be flagged elsewhere"
    );
}

#[test]
fn move_into_block_then_recollect_keeps_elsewhere_moved() {
    // FR-06: an owner-moved event (`elsewhere = 2`) must never be flipped
    // back by a later collect, even when the commit is still unresolvable
    // locally.
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "movedsha", "repository": {"full_name": "org/definitely-not-cloned-xyz"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"}, "message": "moved commit"}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let client = http::client().unwrap();
    collect_with(&conn, &auth(server.base_url()), since, until, &client).unwrap();

    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    let event_id: i64 = conn
        .query_row(
            "SELECT id FROM events WHERE source_id = 'movedsha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    crate::elsewhere::move_into_block(&conn, event_id, block_id).unwrap();

    // Re-collect: the same commit is seen again, still unresolvable.
    collect_with(&conn, &auth(server.base_url()), since, until, &client).unwrap();

    let elsewhere: i64 = conn
        .query_row(
            "SELECT elsewhere FROM events WHERE source_id = 'movedsha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        elsewhere, 2,
        "a re-collect must never flip an owner-moved event back"
    );

    let listed =
        crate::elsewhere::list_for_day(&conn, NaiveDate::from_ymd_opt(2026, 4, 18).unwrap())
            .unwrap();
    assert!(
        listed.is_empty(),
        "an owner-moved event must not appear in the elsewhere list"
    );
}

#[test]
fn collect_surfaces_http_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(403).body("rate limited");
    });
    let conn = open_memory().unwrap();
    let err = format!(
        "{:#}",
        collect_with(
            &conn,
            &auth(server.base_url()),
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &http::client().unwrap(),
        )
        .unwrap_err()
    );
    assert!(err.contains("HTTP 403"), "err = {err}");
}

#[test]
fn github_raw_commit_body_stored_and_scrubbed() {
    let server = MockServer::start();
    let token = format!("ghp_{}", "a".repeat(36));
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "abc123", "repository": {"full_name": "org/repo"},
             "commit": {"author": {"date": "2026-04-18T09:00:00Z"},
                        "message": format!("PROJ-42 fix login bug\n\nuses token {token} to auth")}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });

    let (_conn, _report, events) = run(server.base_url());
    let commit = events.iter().find(|e| e.source == "github_commit").unwrap();
    let raw = commit.raw_json.as_deref().expect("raw_json set");
    assert!(
        !raw.contains(&token),
        "raw_json must not contain the raw token: {raw}"
    );
    let record: RawRecord = serde_json::from_str(raw).unwrap();
    match record {
        RawRecord::Commit {
            sha,
            body,
            local_folder,
        } => {
            assert_eq!(sha, "abc123");
            assert_eq!(body, "uses token [secret] to auth");
            assert_eq!(local_folder, None);
        }
        other => panic!("expected RawRecord::Commit, got {other:?}"),
    }
}

#[test]
fn github_raw_pr_body_stored_with_empty_sha() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": []}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": [
            {"id": 1001, "number": 12, "title": "Add dashboard", "body": "fixes the thing",
             "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/org/repo"}
        ]}));
    });

    let (_conn, _report, events) = run(server.base_url());
    let pr = events.iter().find(|e| e.source == "github_pr").unwrap();
    let raw = pr.raw_json.as_deref().expect("raw_json set");
    let record: RawRecord = serde_json::from_str(raw).unwrap();
    match record {
        RawRecord::Commit {
            sha,
            body,
            local_folder,
        } => {
            assert_eq!(sha, "");
            assert_eq!(body, "fixes the thing");
            assert_eq!(local_folder, None);
        }
        other => panic!("expected RawRecord::Commit, got {other:?}"),
    }
}
