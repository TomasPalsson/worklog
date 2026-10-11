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
fn collect_cleans_up_leftover_personal_rows_from_before_github_user_was_set() {
    // FR-02/D-06: if `run_upgrade_006` ran before `github_user` was
    // configured (secret lookup failed/None), a personal-owner row could
    // survive forever. Every later collect_with call has auth.user in
    // hand, so it's the natural place to keep the deletion re-runnable.
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": []}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });

    let conn = open_memory().unwrap();
    // A leftover personal-owner row, stored before FR-02 could delete it.
    let leftover = Event {
        repo: Some("TomasPalsson/dotfiles".into()),
        ..Event::minimal(
            "github_commit",
            "leftover-personal-sha",
            "2026-04-18T09:00:00+00:00",
            "personal commit",
        )
    };
    crate::repo::upsert_event(&conn, &leftover).unwrap();

    collect_with(
        &conn,
        &auth(server.base_url()),
        NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
        NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
        &http::client().unwrap(),
    )
    .unwrap();

    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE source_id = 'leftover-personal-sha'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        remaining, 0,
        "a leftover personal-owner row must be cleaned up on the next collect"
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
            ..
        } => {
            assert_eq!(sha, "abc123");
            assert_eq!(body, "uses token [secret] to auth");
            assert_eq!(local_folder, None);
        }
        other => panic!("expected RawRecord::Commit, got {other:?}"),
    }
}

#[test]
fn github_commit_details_are_scrubbed_too() {
    // FR-11/D-03: details isn't just raw_json — the full commit message
    // stored in events.details must never carry a live token either.
    let server = MockServer::start();
    let token = format!("ghp_{}", "a".repeat(36));
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": [
            {"sha": "abc456", "repository": {"full_name": "org/repo"},
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
    let details = commit.details.as_deref().expect("details set");
    assert!(
        !details.contains(&token),
        "details must not contain the raw token: {details}"
    );
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
            ..
        } => {
            assert_eq!(sha, "");
            assert_eq!(body, "fixes the thing");
            assert_eq!(local_folder, None);
        }
        other => panic!("expected RawRecord::Commit, got {other:?}"),
    }
}

#[test]
fn records_merged_at() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": []}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues").query_param(
            "q",
            "author:TomasPalsson type:pr created:2026-04-18..2026-04-19",
        );
        then.status(200).json_body(json!({"items": [
            {"id": 1001, "number": 12, "title": "Open PR GENAI-1", "body": null,
             "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/org/repo",
             "pull_request": {"merged_at": null}}
        ]}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues").query_param(
            "q",
            "author:TomasPalsson type:pr merged:2026-04-18..2026-04-19",
        );
        then.status(200).json_body(json!({"items": [
            {"id": 1001, "number": 12, "title": "Open PR GENAI-1", "body": null,
             "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
             "repository_url": "https://api.github.com/repos/org/repo",
             "pull_request": {"merged_at": null}},
            {"id": 2002, "number": 7, "title": "Older PR GENAI-9129", "body": null,
             "created_at": "2026-04-01T10:00:00Z", "closed_at": "2026-04-18T11:00:00Z",
             "repository_url": "https://api.github.com/repos/org/repo",
             "pull_request": {"merged_at": "2026-04-18T11:00:00Z"}}
        ]}));
    });

    let (conn, report, _events) = run(server.base_url());
    assert_eq!(report.events_written, 2);

    let merged_at = |source_id: &str| {
        let raw = conn
            .query_row(
                "SELECT raw_json FROM events WHERE source = 'github_pr' AND source_id = ?1",
                [source_id],
                |r| crate::raw_json::decode_raw_json(r, 0),
            )
            .unwrap()
            .expect("raw_json set");
        match serde_json::from_str::<RawRecord>(&raw).unwrap() {
            RawRecord::Commit { merged_at, .. } => merged_at,
            other => panic!("expected RawRecord::Commit, got {other:?}"),
        }
    };
    assert_eq!(merged_at("1001"), None);
    assert_eq!(merged_at("2002").as_deref(), Some("2026-04-18T11:00:00Z"));
}

fn commit_items(prefix: &str, n: usize) -> serde_json::Value {
    let items: Vec<_> = (0..n)
        .map(|i| {
            json!({"sha": format!("{prefix}{i}"), "repository": {"full_name": "org/repo"},
                   "commit": {"author": {"date": "2026-04-18T09:00:00Z"}, "message": "work"}})
        })
        .collect();
    json!({ "items": items })
}

fn pr_items(first_id: i64, n: usize) -> serde_json::Value {
    let items: Vec<_> = (0..n as i64)
        .map(|i| {
            json!({"id": first_id + i, "number": first_id + i, "title": "pr", "body": null,
                   "created_at": "2026-04-18T10:00:00Z", "closed_at": null,
                   "repository_url": "https://api.github.com/repos/org/repo"})
        })
        .collect();
    json!({ "items": items })
}

fn empty_issues(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET).path("/search/issues");
        then.status(200).json_body(json!({"items": []}));
    });
}

#[test]
fn commits_page_through_every_page() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/commits")
            .query_param("page", "1");
        then.status(200).json_body(commit_items("a", 100));
    });
    let p2 = server.mock(|when, then| {
        when.method(GET)
            .path("/search/commits")
            .query_param("page", "2");
        then.status(200).json_body(commit_items("b", 1));
    });
    empty_issues(&server);

    let (_conn, report, events) = run(server.base_url());
    // single-page fetch stores 100
    assert_eq!(report.events_written, 101);
    assert_eq!(events.len(), 101);
    // a short page 2 must end the loop: no page 3 request
    assert_eq!(p2.hits(), 1);
}

#[test]
fn short_first_page_requests_no_second_page() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(commit_items("a", 99));
    });
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    assert_eq!(report.events_written, 99);
    // always-fetch-next would hit more than once
    assert_eq!(any.hits(), 1);
}

#[test]
fn commit_paging_stops_at_github_max_pages() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(commit_items("a", 100));
    });
    empty_issues(&server);

    let _ = run(server.base_url());
    // unbounded loop never ends; `>` for `>=` would make 11
    assert_eq!(
        any.hits(),
        crate::time_leaks_contract::GITHUB_MAX_PAGES as usize
    );
}

#[test]
fn prs_page_through_every_page() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": []}));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/issues")
            .query_param("page", "1");
        then.status(200).json_body(pr_items(1, 100));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/issues")
            .query_param("page", "2");
        then.status(200).json_body(pr_items(101, 1));
    });

    let (_conn, report, events) = run(server.base_url());
    // first-page-only PR fetch stores 100; created+merged return the same set, deduped to 101
    assert_eq!(report.events_written, 101);
    assert_eq!(events.len(), 101);
}

#[test]
fn page_two_error_fails_the_collect() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/commits")
            .query_param("page", "1");
        then.status(200).json_body(commit_items("a", 100));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/commits")
            .query_param("page", "2");
        then.status(403).body("rate limited");
    });
    empty_issues(&server);
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
    // swallowing the page-2 error would return Ok with 100 events
    assert!(err.contains("HTTP 403"), "err = {err}");
}

const COMMITS_Q: &str = "author:TomasPalsson author-date:2026-04-18..2026-04-19";

fn mock_commit_page(server: &MockServer, page: u32, body: serde_json::Value) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/search/commits")
            .query_param("q", COMMITS_Q)
            .query_param("page", page.to_string());
        then.status(200).json_body(body);
    });
}

#[test]
fn full_last_page_at_cap_warns_naming_query_and_cap() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(commit_items("a", 100));
    });
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    // silently dropping the truncated tail leaves errors empty
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(report.errors[0].contains(COMMITS_Q), "{:?}", report.errors);
    assert!(report.errors[0].contains("10"), "{:?}", report.errors);
}

#[test]
fn short_page_exactly_at_cap_does_not_warn() {
    let server = MockServer::start();
    for page in 1..=9 {
        mock_commit_page(&server, page, commit_items(&format!("p{page}-"), 100));
    }
    mock_commit_page(&server, 10, commit_items("last", 99));
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    // warning on page == cap regardless of fullness would flag this
    assert_eq!(report.events_written, 999);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn full_page_before_cap_does_not_warn() {
    let server = MockServer::start();
    mock_commit_page(&server, 1, commit_items("a", 100));
    mock_commit_page(&server, 2, commit_items("b", 1));
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    // warning on any full page would flag this
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn incomplete_results_true_warns_even_on_short_page() {
    let server = MockServer::start();
    let mut body = commit_items("a", 3);
    body["incomplete_results"] = json!(true);
    mock_commit_page(&server, 1, body);
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    // ignoring the flag leaves errors empty
    assert_eq!(report.events_written, 3);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(report.errors[0].contains(COMMITS_Q), "{:?}", report.errors);
    assert!(
        report.errors[0].contains("incomplete"),
        "{:?}",
        report.errors
    );
}

#[test]
fn incomplete_results_false_does_not_warn() {
    let server = MockServer::start();
    let mut body = commit_items("a", 3);
    body["incomplete_results"] = json!(false);
    mock_commit_page(&server, 1, body);
    empty_issues(&server);

    let (_conn, report, _events) = run(server.base_url());
    // warning whenever the field is present would flag this
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn pr_search_at_cap_warns_naming_its_query() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search/commits");
        then.status(200).json_body(json!({"items": []}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues").query_param(
            "q",
            "author:TomasPalsson type:pr created:2026-04-18..2026-04-19",
        );
        then.status(200).json_body(pr_items(1, 100));
    });
    server.mock(|when, then| {
        when.method(GET).path("/search/issues").query_param(
            "q",
            "author:TomasPalsson type:pr merged:2026-04-18..2026-04-19",
        );
        then.status(200).json_body(pr_items(1, 1));
    });

    let (_conn, report, _events) = run(server.base_url());
    // only the commit search warning path wired up would leave errors empty
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(
        report.errors[0].contains("type:pr created:"),
        "{:?}",
        report.errors
    );
}
