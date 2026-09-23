use super::*;
use crate::db::open_memory;
use httpmock::prelude::*;
use serde_json::json;

fn auth() -> SlackAuth {
    SlackAuth {
        token: "xoxp-test".into(),
    }
}

#[test]
fn collect_writes_events_with_channel_and_text() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/search.messages")
            .query_param("query", "from:me after:2026-04-17 before:2026-04-19");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C123", "name": "general" },
                        "ts": "1776513600.000100",
                        "text": "shipped the fix"
                    }
                ]
            }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    assert_eq!(report.events_written, 1);
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(ev.source, "slack");
    assert_eq!(ev.source_id, "C123:1776513600.000100");
    assert_eq!(ev.title, "general");
    assert_eq!(ev.details.as_deref(), Some("shipped the fix"));
    // 1776513600 = 2026-04-18T12:00:00Z (hand-computed via
    // datetime.timestamp() on that UTC instant).
    assert_eq!(ev.started_at, "2026-04-18T12:00:00Z");
}

#[test]
fn collect_is_idempotent_by_source_id() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C1", "name": "eng" },
                        "ts": "1776513600.000100",
                        "text": "hello"
                    }
                ]
            }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();
    collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(
        events.len(),
        1,
        "dedupe on (source, source_id) must prevent duplicates"
    );
}

#[test]
fn collect_surfaces_ok_false_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200)
            .json_body(json!({ "ok": false, "error": "invalid_auth" }));
    });

    let conn = open_memory().unwrap();
    let err = format!(
        "{:#}",
        collect_with(
            &conn,
            &auth(),
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &http::client().unwrap(),
            &server.base_url(),
        )
        .unwrap_err()
    );
    assert!(err.contains("invalid_auth"), "err = {err}");
}

#[test]
fn collect_surfaces_http_errors() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(403).body("rate limited");
    });
    let conn = open_memory().unwrap();
    let err = format!(
        "{:#}",
        collect_with(
            &conn,
            &auth(),
            NaiveDate::from_ymd_opt(2026, 4, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 19).unwrap(),
            &http::client().unwrap(),
            &server.base_url(),
        )
        .unwrap_err()
    );
    assert!(err.contains("HTTP 403"), "err = {err}");
}

#[test]
fn collect_paginates_via_page_param() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/search.messages")
            .query_param("sort", "timestamp")
            .query_param("page", "1");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C1", "name": "eng" },
                        "ts": "1776513600.000100",
                        "text": "page one"
                    }
                ],
                "paging": { "page": 1, "pages": 2 }
            }
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/search.messages")
            .query_param("page", "2");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C2", "name": "eng" },
                        "ts": "1776513700.000100",
                        "text": "page two"
                    }
                ],
                "paging": { "page": 2, "pages": 2 }
            }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    assert_eq!(report.events_written, 2, "both pages must be consumed");
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    let ids: Vec<_> = events.iter().map(|e| e.source_id.as_str()).collect();
    assert!(ids.contains(&"C1:1776513600.000100"));
    assert!(ids.contains(&"C2:1776513700.000100"));
}

#[test]
fn collect_skips_malformed_timestamp_and_continues() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C1", "name": "eng" },
                        "ts": "1776513600.000100",
                        "text": "good message"
                    },
                    {
                        "channel": { "id": "C2", "name": "eng" },
                        "ts": "not-a-number",
                        "text": "bad message"
                    }
                ]
            }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    assert_eq!(
        report.events_written, 1,
        "the well-formed message must still be written"
    );
    assert_eq!(report.skipped, 1);
    assert_eq!(report.errors.len(), 1);
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source_id, "C1:1776513600.000100");
}

#[test]
fn collect_titles_dm_with_real_name_via_users_info() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "D1", "name": "U999", "is_im": true },
                        "ts": "1776513600.000100",
                        "text": "hey"
                    }
                ]
            }
        }));
    });
    let users_info = server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .query_param("user", "U999");
        then.status(200).json_body(json!({
            "ok": true,
            "user": { "profile": { "real_name": "Jane Doe", "display_name": "janed" } }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    users_info.assert_hits(1);
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "Jane Doe");
}

#[test]
fn collect_titles_dm_falls_back_to_display_name() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "D1", "name": "U999", "is_im": true },
                        "ts": "1776513600.000100",
                        "text": "hey"
                    }
                ]
            }
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .query_param("user", "U999");
        then.status(200).json_body(json!({
            "ok": true,
            "user": { "profile": { "display_name": "janed" } }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events[0].title, "janed");
}

#[test]
fn collect_titles_dm_falls_back_to_id_on_lookup_failure_and_continues() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "D1", "name": "U999", "is_im": true },
                        "ts": "1776513600.000100",
                        "text": "hey"
                    }
                ]
            }
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .query_param("user", "U999");
        then.status(200)
            .json_body(json!({ "ok": false, "error": "user_not_found" }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    assert_eq!(
        report.events_written, 1,
        "lookup failure must not fail the collect"
    );
    assert_eq!(report.errors, vec!["users.info U999: user_not_found"]);
    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events[0].title, "U999");
}

#[test]
fn collect_caches_users_info_lookup_per_user_id() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "D1", "name": "U999", "is_im": true },
                        "ts": "1776513600.000100",
                        "text": "first"
                    },
                    {
                        "channel": { "id": "D1", "name": "U999", "is_im": true },
                        "ts": "1776513700.000100",
                        "text": "second"
                    }
                ]
            }
        }));
    });
    let users_info = server.mock(|when, then| {
        when.method(GET)
            .path("/users.info")
            .query_param("user", "U999");
        then.status(200).json_body(json!({
            "ok": true,
            "user": { "profile": { "real_name": "Jane Doe" } }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    let report = collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    assert_eq!(report.events_written, 2);
    users_info.assert_hits(1);
}

#[test]
fn collect_leaves_non_dm_title_as_channel_name() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": {
                "matches": [
                    {
                        "channel": { "id": "C1", "name": "eng" },
                        "ts": "1776513600.000100",
                        "text": "hi"
                    }
                ]
            }
        }));
    });

    let conn = open_memory().unwrap();
    let since = NaiveDate::from_ymd_opt(2026, 4, 18).unwrap();
    let until = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
    collect_with(
        &conn,
        &auth(),
        since,
        until,
        &http::client().unwrap(),
        &server.base_url(),
    )
    .unwrap();

    let events = repo::load_day_events(&conn, "2026-04-18").unwrap();
    assert_eq!(events[0].title, "eng");
}
