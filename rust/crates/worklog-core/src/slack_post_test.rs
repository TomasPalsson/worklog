use super::*;
use httpmock::prelude::*;
use serde_json::json;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()
}

fn hit(text: &str, ts: &str, thread_ts: Option<&str>) -> Value {
    let mut m = json!({
        "text": text, "ts": ts, "permalink": format!("https://x.slack.com/p{ts}"),
        "channel": {"id": "C123", "name": "daily"},
    });
    if let Some(t) = thread_ts {
        m["thread_ts"] = json!(t);
    }
    m
}

fn search_body(matches: Vec<Value>) -> Value {
    json!({"ok": true, "messages": {"matches": matches}})
}

fn run(server: &MockServer, channel: Option<&str>) -> PostOutcome {
    post_to_daily(
        &Client::new(),
        &server.base_url(),
        "xoxp-t",
        channel,
        today(),
        "the standup",
    )
    .unwrap()
}

fn mock_search(server: &MockServer, body: Value) -> httpmock::Mock<'_> {
    server.mock(|when, then| {
        when.method(GET)
            .path("/search.messages")
            .header("authorization", "Bearer xoxp-t")
            .query_param("query", "in:#daily on:2026-10-06 \"Daily:thread\"");
        then.status(200).json_body(body);
    })
}

#[test]
fn replies_in_the_thread_of_todays_daily_message() {
    let server = MockServer::start();
    mock_search(
        &server,
        search_body(vec![hit("Daily:thread 6 Oct", "111.1", None)]),
    );
    // wrong impl: top-level post (no thread_ts) or wrong channel id/text
    let post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .header("authorization", "Bearer xoxp-t")
            .json_body(json!({"channel": "C123", "thread_ts": "111.1", "text": "the standup"}));
        then.status(200)
            .json_body(json!({"ok": true, "ts": "222.2"}));
    });
    let link = server.mock(|when, then| {
        when.method(GET)
            .path("/chat.getPermalink")
            .query_param("channel", "C123")
            .query_param("message_ts", "222.2");
        then.status(200)
            .json_body(json!({"ok": true, "permalink": "https://x.slack.com/reply"}));
    });
    let out = run(&server, Some("daily"));
    post.assert();
    link.assert();
    assert_eq!(
        out,
        PostOutcome::Posted {
            permalink: "https://x.slack.com/reply".into()
        }
    );
}

#[test]
fn leading_hash_in_the_setting_is_not_doubled() {
    let server = MockServer::start();
    // wrong impl: formats "in:##daily"; the strict query_param in mock_search fails
    let search = mock_search(&server, search_body(vec![]));
    run(&server, Some("#daily"));
    search.assert();
}

#[test]
fn unset_or_blank_channel_is_no_channel_and_makes_no_call() {
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(json!({"ok": true}));
    });
    // wrong impl: only checks is_none, so "" and "  " would search in:#
    for c in [None, Some(""), Some("   ")] {
        assert_eq!(run(&server, c), PostOutcome::NoChannel, "{c:?}");
    }
    any.assert_hits(0);
}

#[test]
fn no_matching_message_is_no_thread_and_nothing_is_posted() {
    let server = MockServer::start();
    mock_search(&server, search_body(vec![]));
    let post = server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(json!({"ok": true}));
    });
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::NoThread {
            channel: "daily".into()
        }
    );
    post.assert_hits(0);
}

#[test]
fn only_a_message_beginning_with_the_prefix_counts_and_the_first_such_is_used() {
    let server = MockServer::start();
    // wrong impl: contains() or taking matches[0] picks 111.1 / 333.3
    mock_search(
        &server,
        search_body(vec![
            hit("Re: Daily:thread 6 Oct", "111.1", None),
            hit("Daily:thread 6 Oct", "333.3", Some("111.1")),
            hit("Daily:thread 6 Oct", "444.4", None),
        ]),
    );
    let post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .json_body_partial(r#"{"thread_ts": "444.4"}"#);
        then.status(200)
            .json_body(json!({"ok": true, "ts": "555.5"}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/chat.getPermalink");
        then.status(200)
            .json_body(json!({"ok": true, "permalink": "L"}));
    });
    run(&server, Some("daily"));
    post.assert();
}

#[test]
fn a_reply_that_begins_with_the_prefix_is_not_the_thread() {
    let server = MockServer::start();
    // wrong impl: ignores thread_ts != ts and would reply inside a reply
    mock_search(
        &server,
        search_body(vec![hit("Daily:thread x", "333.3", Some("111.1"))]),
    );
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::NoThread {
            channel: "daily".into()
        }
    );
}

#[test]
fn slack_refusing_the_search_is_surfaced_and_nothing_is_posted() {
    let server = MockServer::start();
    // wrong impl: swallows ok:false as "no thread"
    mock_search(&server, json!({"ok": false, "error": "invalid_auth"}));
    let post = server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(json!({"ok": true}));
    });
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::SlackRefused {
            error: "invalid_auth".into()
        }
    );
    post.assert_hits(0);
}

#[test]
fn slack_refusing_the_post_is_surfaced() {
    let server = MockServer::start();
    mock_search(
        &server,
        search_body(vec![hit("Daily:thread", "111.1", None)]),
    );
    server.mock(|when, then| {
        when.method(POST).path("/chat.postMessage");
        then.status(200)
            .json_body(json!({"ok": false, "error": "not_in_channel"}));
    });
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::SlackRefused {
            error: "not_in_channel".into()
        }
    );
}

#[test]
fn http_error_status_is_a_refusal_naming_the_status() {
    let server = MockServer::start();
    // wrong impl: parses the 500 body as JSON and reports a decode error, or Ok
    server.mock(|when, then| {
        when.method(GET).path("/search.messages");
        then.status(500).body("boom");
    });
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::SlackRefused {
            error: "http 500".into()
        }
    );
}

#[test]
fn a_failed_permalink_lookup_does_not_undo_a_posted_reply() {
    let server = MockServer::start();
    mock_search(
        &server,
        search_body(vec![hit("Daily:thread", "111.1", None)]),
    );
    server.mock(|when, then| {
        when.method(POST).path("/chat.postMessage");
        then.status(200)
            .json_body(json!({"ok": true, "ts": "222.2"}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/chat.getPermalink");
        then.status(200)
            .json_body(json!({"ok": false, "error": "message_not_found"}));
    });
    // wrong impl: reports SlackRefused although the reply is already live
    assert_eq!(
        run(&server, Some("daily")),
        PostOutcome::Posted {
            permalink: "https://x.slack.com/p111.1".into()
        }
    );
}

#[test]
fn parse_ok_accepts_ok_true_and_returns_the_body() {
    assert_eq!(parse_ok(r#"{"ok":true,"ts":"1"}"#).unwrap()["ts"], "1");
}

#[test]
fn parse_ok_returns_slacks_error_string() {
    // wrong impl: returns Ok whenever the JSON parses
    assert_eq!(
        parse_ok(r#"{"ok":false,"error":"ratelimited"}"#).unwrap_err(),
        "ratelimited"
    );
}

#[test]
fn parse_ok_never_returns_an_empty_error() {
    // wrong impl: unwrap_or_default() on a missing error field
    assert!(!parse_ok(r#"{"ok":false}"#).unwrap_err().is_empty());
    assert!(!parse_ok("<html>").unwrap_err().is_empty());
}
