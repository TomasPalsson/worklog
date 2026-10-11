use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::daemon::daemon_stats::topics_report;
use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;
use crate::embed::EmbedClient;
use crate::topics_contract::{TopicsStatus, EMBED_DIM};
use chrono::NaiveDate;
use httpmock::prelude::*;
use rusqlite::{params, Connection};
use serde_json::json;

async fn call(path: &str) -> (StatusCode, serde_json::Value) {
    let state = state_from_conn(open_memory().unwrap());
    let response = router(state)
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test(flavor = "current_thread")]
async fn stats_returns_the_contract_keys() {
    let (status, v) = call("/stats?from=2026-09-01&to=2026-09-03").await;
    assert_eq!(status, StatusCode::OK);
    for key in [
        "from",
        "to",
        "today",
        "first_day",
        "totals",
        "daily",
        "punchcard",
        "flow_blocks",
        "tools",
        "shell",
        "slack_channels",
        "domains",
        "folders",
        "helpers",
        "tickets",
        "prompt",
        "records",
        "estimates",
        "ticket_origin",
        "sync",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(v["daily"].as_array().unwrap().len(), 3);
    assert_eq!(v["punchcard"].as_array().unwrap().len(), 7);
}

#[tokio::test(flavor = "current_thread")]
async fn stats_defaults_to_an_empty_one_day_range() {
    let (status, v) = call("/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["daily"].as_array().unwrap().len(), 1);
    assert!(v["first_day"].is_null());
}

#[tokio::test(flavor = "current_thread")]
async fn stats_rejects_bad_ranges_and_dates() {
    for path in [
        "/stats?from=2026-09-05&to=2026-09-04",
        "/stats?from=soon",
        "/stats?to=2026-13-40",
        "/stats?from=0001-01-01&to=9999-12-31",
        "/stats?from=1960-01-01&to=1960-01-02",
    ] {
        assert_eq!(call(path).await.0, StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn topics_route_with_no_blocks_is_too_few_not_an_error() {
    let _env = crate::envfile::ENV_TEST_LOCK.lock().await;
    {
        // Never read the developer's real env file.
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_ENV_FILE", "/nonexistent/worklog.env");
    }
    let (status, v) = call("/stats/topics?from=2026-10-05&to=2026-10-11").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["status"], "too_few_blocks");
    assert_eq!(v["from"], "2026-10-05");
    assert_eq!(v["to"], "2026-10-11");
}

#[tokio::test(flavor = "current_thread")]
async fn topics_route_rejects_bad_ranges_and_dates() {
    for path in [
        "/stats/topics?from=2026-09-05&to=2026-09-04",
        "/stats/topics?from=soon",
        "/stats/topics?from=0001-01-01&to=9999-12-31",
    ] {
        assert_eq!(call(path).await.0, StatusCode::BAD_REQUEST, "{path}");
    }
}

fn unit(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; EMBED_DIM];
    v[i] = 1.0;
    v
}

fn block(
    conn: &Connection,
    day: &str,
    seconds: i64,
    description: Option<&str>,
    personal: i64,
    ignored: Option<&str>,
) {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description,
                             is_personal, ignored_at)
         VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6)",
        params![day, format!("{day}T09:00:00Z"), seconds, description, personal, ignored],
    )
    .unwrap();
}

fn day(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn serve(server: &MockServer, status: u16, vectors: Vec<Vec<f32>>) {
    server.mock(|when, then| {
        when.method(POST).path("/embed");
        then.status(status).json_body(json!({ "vectors": vectors }));
    });
}

fn client(server: &MockServer) -> EmbedClient {
    EmbedClient::with_client(reqwest::blocking::Client::new(), server.base_url())
}

#[test]
fn topics_report_loads_exactly_the_right_blocks_and_counts_skipped() {
    let conn = open_memory().unwrap();
    // Oldest first: ids ascend, and the mock answers in id order.
    block(&conn, "2026-08-05", 100, Some("too old"), 0, None); // from-61: excluded
    block(&conn, "2026-08-06", 100, Some("history edge"), 0, None); // from-60: included
    block(&conn, "2026-10-01", 600, Some("previous range"), 0, None);
    for (d, secs) in [("2026-10-05", 3600), ("2026-10-06", 3600), ("2026-10-07", 3600)] {
        block(&conn, d, secs, Some("alpha work"), 0, None);
    }
    for (d, secs) in [("2026-10-08", 1800), ("2026-10-09", 1800), ("2026-10-10", 1800)] {
        block(&conn, d, secs, Some("beta work"), 0, None);
    }
    block(&conn, "2026-10-11", 900, Some("private"), 1, None); // personal: excluded
    block(&conn, "2026-10-11", 900, Some("ignored"), 0, Some("2026-10-11T10:00:00Z"));
    block(&conn, "2026-10-11", 900, None, 0, None); // no text: skipped
    let server = MockServer::start();
    // Embedded: history edge, previous range, then the six in-range blocks.
    // A wrong window or filter changes the text count, the mock's vector
    // count no longer matches, and the result would be HelperDown.
    let mut vectors = vec![unit(2), unit(0)];
    vectors.extend([unit(0), unit(0), unit(0), unit(1), unit(1), unit(1)]);
    serve(&server, 200, vectors);

    let r = topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap();

    assert_eq!(r.status, TopicsStatus::Ready);
    assert_eq!(r.embedded_blocks, 6);
    assert_eq!(r.skipped_blocks, 1);
    assert_eq!(r.topics[0].seconds, 3 * 3600);
    // The previous-range block (600 s, alpha-like) lands in the alpha topic.
    assert_eq!(r.drift[0].seconds_before, 600);
    assert_eq!(r.drift[1].seconds_before, 0);
}

#[test]
fn topics_report_maps_a_helper_error_to_helper_down() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-05", 600, Some("alpha work"), 0, None);
    let server = MockServer::start();
    serve(&server, 500, vec![]);

    let r = topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap();

    assert_eq!(r.status, TopicsStatus::HelperDown);
    assert!(r.topics.is_empty());
    assert_eq!(r.embedded_blocks, 0);
}

#[test]
fn topics_report_propagates_a_db_error_instead_of_helper_down() {
    let conn = open_memory().unwrap();
    block(&conn, "2026-10-05", 600, Some("alpha work"), 0, None);
    conn.execute("DROP TABLE block_embeddings", []).unwrap();
    let server = MockServer::start();
    serve(&server, 200, vec![unit(0)]);

    // catches: `let Ok(..) = ensure_vectors(..) else { HelperDown }`.
    assert!(topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).is_err());
}

#[test]
fn topics_report_stores_nothing_when_a_later_batch_is_bad() {
    let conn = open_memory().unwrap();
    for n in 0..40 {
        block(&conn, "2026-10-05", 600, Some(&format!("b{n}")), 0, None);
    }
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/embed").body_contains("\"b0\"");
        then.status(200)
            .json_body(json!({ "vectors": (0..32).map(unit).collect::<Vec<_>>() }));
    });
    server.mock(|when, then| {
        when.method(POST).path("/embed").body_contains("\"b39\"");
        then.status(200).json_body(json!({ "vectors": [unit(0)] }));
    });

    let r = topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap();

    assert_eq!(r.status, TopicsStatus::HelperDown);
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM block_embeddings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
