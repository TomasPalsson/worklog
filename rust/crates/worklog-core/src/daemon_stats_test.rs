use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::daemon::daemon_stats::topics_report;
use crate::daemon::{router, state_from_conn};
use crate::db::open_memory;
use crate::embed::EmbedClient;
use crate::topics_contract::{NamedBy, Naming, TopicsStatus, EMBED_DIM};
use chrono::NaiveDate;
use httpmock::prelude::*;
use rusqlite::{params, Connection};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::daemon::daemon_stats::{name_in_background, BACKOFF};
use crate::estimate::ModelInvoker;
use crate::topic_names::{self, TopicInput};

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

fn snapshot(conn: &Connection) -> std::collections::BTreeMap<String, (i64, String)> {
    let names: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    names
        .into_iter()
        .map(|t| {
            let cols: Vec<String> = conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            let row = cols
                .iter()
                .map(|c| format!("quote(\"{c}\")"))
                .collect::<Vec<_>>()
                .join("||'|'||");
            let snap = conn
                .query_row(
                    &format!(
                        "SELECT count(*), coalesce(group_concat(r, char(10)), '')
                           FROM (SELECT {row} AS r FROM \"{t}\" ORDER BY 1)"
                    ),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            (t, snap)
        })
        .collect()
}

#[test]
fn topics_report_writes_only_block_embeddings() {
    let conn = open_memory().unwrap();
    conn.execute(
        "INSERT INTO jira_tickets (key, summary) VALUES ('ABC-1', 'Fix the thing')",
        [],
    )
    .unwrap();
    for (d, text) in [
        ("2026-10-05", "alpha one"),
        ("2026-10-06", "alpha two"),
        ("2026-10-07", "alpha three"),
        ("2026-10-08", "beta one"),
        ("2026-10-09", "beta two"),
        ("2026-10-10", "beta three"),
    ] {
        block(&conn, d, 3600, Some(text), 0, None);
    }
    conn.execute("UPDATE blocks SET jira_issue = 'ABC-1'", []).unwrap();
    let server = MockServer::start();
    serve(
        &server,
        200,
        vec![unit(0), unit(0), unit(0), unit(1), unit(1), unit(1)],
    );
    let before = snapshot(&conn);

    let r = topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap();
    let after = snapshot(&conn);

    assert_eq!(r.status, TopicsStatus::Ready);
    assert_eq!(r.embedded_blocks, 6);
    // catches: a stray write to any other table (blocks, tickets, FTS, change log).
    for (table, snap) in &before {
        if table != "block_embeddings" {
            assert_eq!(&after[table], snap, "table {table} changed");
        }
    }
    // catches: a vacuous snapshot that never sees the embeddings land.
    assert_eq!(before["block_embeddings"].0, 0);
    assert_eq!(after["block_embeddings"].0, 6);
    assert_eq!(after.keys().collect::<Vec<_>>(), before.keys().collect::<Vec<_>>());
}

/// Six embedded blocks in two clear themes: ids 1-3 "alpha" (3600 s each,
/// topic 0), ids 4-6 "beta" (1800 s each, topic 1).
fn seed_two_themes(conn: &Connection) {
    for d in ["2026-10-05", "2026-10-06", "2026-10-07"] {
        block(conn, d, 3600, Some("alpha onboarding flow"), 0, None);
    }
    for d in ["2026-10-08", "2026-10-09", "2026-10-10"] {
        block(conn, d, 1800, Some("beta invoice export"), 0, None);
    }
}

fn store_vectors(conn: &Connection, vectors: Vec<Vec<f32>>) {
    let ids: Vec<i64> = (1..=vectors.len() as i64).collect();
    let pending = crate::embed::pending(conn, &ids).unwrap();
    crate::embed::store(conn, &pending, &vectors).unwrap();
}

fn two_theme_vectors() -> Vec<Vec<f32>> {
    vec![unit(0), unit(0), unit(0), unit(1), unit(1), unit(1)]
}

fn cache_name(conn: &Connection, members: &[i64], name: &str) {
    conn.execute(
        "INSERT INTO topic_names VALUES (?1, ?2, ?3, 'Described.', 'm', '2026-10-01T00:00:00Z')",
        params![
            crate::topic_names::member_hash(members),
            crate::topic_names::join_ids(members),
            name
        ],
    )
    .unwrap();
}

fn names_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM topic_names", [], |r| r.get(0))
        .unwrap()
}

fn range_report(conn: &Connection) -> crate::topics_contract::TopicsReport {
    let server = MockServer::start();
    serve(&server, 200, vec![]);
    topics_report(conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap()
}

#[test]
fn naming_fully_cached_topics_get_ai_names_and_naming_is_ai() {
    let conn = open_memory().unwrap();
    seed_two_themes(&conn);
    store_vectors(&conn, two_theme_vectors());
    cache_name(&conn, &[1, 2, 3], "Alpha Cached");
    cache_name(&conn, &[4, 5, 6], "Beta Cached");

    let r = range_report(&conn);

    assert_eq!(r.naming, Naming::Ai);
    assert_eq!(r.topics[0].name, "Alpha Cached");
    assert_eq!(r.topics[0].description.as_deref(), Some("Described."));
    assert_eq!(r.topics[0].named_by, NamedBy::Ai);
    assert_eq!(r.topics[1].name, "Beta Cached");
    // catches: keywords skipped when a cached name exists.
    assert!(r.topics[0].keywords.contains(&"alpha".to_string()), "{:?}", r.topics[0].keywords);
    assert!(!r.topics[0].keywords.contains(&"beta".to_string()));
}

#[test]
fn naming_one_uncached_topic_is_pending_with_a_keyword_name() {
    let conn = open_memory().unwrap();
    seed_two_themes(&conn);
    store_vectors(&conn, two_theme_vectors());
    cache_name(&conn, &[1, 2, 3], "Alpha Cached");

    let r = range_report(&conn);

    // catches: reporting Ai as soon as any topic is named.
    assert_eq!(r.naming, Naming::Pending);
    assert_eq!(r.topics[0].named_by, NamedBy::Ai);
    assert_eq!(r.topics[1].named_by, NamedBy::Keywords);
    assert!(r.topics[1].name.contains("beta"), "{}", r.topics[1].name);
    assert_eq!(r.topics[1].description, None);
    // Computing the report never writes a name.
    assert_eq!(names_rows(&conn), 1);
}

#[test]
fn naming_helper_down_writes_no_names() {
    let conn = open_memory().unwrap();
    seed_two_themes(&conn);
    let server = MockServer::start();
    serve(&server, 500, vec![]);

    let r = topics_report(&conn, &client(&server), day("2026-10-05"), day("2026-10-11")).unwrap();

    assert_eq!(r.status, TopicsStatus::HelperDown);
    assert_eq!(names_rows(&conn), 0);
}

#[test]
fn naming_without_an_invoker_is_off_and_keeps_keyword_names() {
    let conn = open_memory().unwrap();
    seed_two_themes(&conn);
    store_vectors(&conn, two_theme_vectors());
    // Built before the runtime: the blocking http client cannot live inside one.
    let mut r = range_report(&conn);
    let pending = r.topics.iter().map(|t| pending_input(t.id)).collect();
    let state = state_from_conn(conn);

    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(crate::daemon::daemon_stats::name_in_background(
            &state,
            &mut r,
            pending,
            || Err(anyhow::anyhow!("no model configured")),
            crate::daemon::daemon_stats::BACKOFF,
        ));

    // catches: leaving Pending forever when the invoker cannot be built.
    assert_eq!(r.naming, Naming::Off);
    assert_eq!(r.topics[0].named_by, NamedBy::Keywords);
    assert_eq!(names_rows(&state.conn.blocking_lock()), 0);
}

fn pending_input(id: usize) -> crate::topic_names::TopicInput {
    crate::topic_names::TopicInput {
        id,
        members: vec![id as i64 + 500],
        keywords: vec![],
        examples: vec![],
        blocks: 1,
        seconds: 60,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn naming_route_is_pending_first_then_ai_once_the_job_lands() {
    let _env = crate::envfile::ENV_TEST_LOCK.lock().await;
    {
        let _g = crate::tz::test_env_lock();
        std::env::set_var("WORKLOG_ENV_FILE", "/nonexistent/worklog.env");
    }
    let conn = open_memory().unwrap();
    seed_two_themes(&conn);
    store_vectors(&conn, two_theme_vectors());
    let state = state_from_conn(conn);
    *crate::topic_names::TEST_REPLY.lock().unwrap() = Some(json!({"topics": [
        {"id": 0, "name": "Alpha Onboarding", "description": "Onboarding work."},
        {"id": 1, "name": "Beta Invoicing", "description": "Invoice exports."},
    ]}));
    let get = || async {
        let response = router(state.clone())
            .oneshot(
                Request::get("/stats/topics?from=2026-10-05&to=2026-10-11")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    };

    let first = get().await;
    assert_eq!(first["status"], "ready");
    // catches: blocking the page on the model call, or reporting ai before names exist.
    assert_eq!(first["naming"], "pending");
    assert_eq!(first["topics"][0]["named_by"], "keywords");

    let mut last = first;
    for _ in 0..250 {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        last = get().await;
        if last["naming"] == "ai" {
            break;
        }
    }
    *crate::topic_names::TEST_REPLY.lock().unwrap() = None;

    assert_eq!(last["naming"], "ai", "naming job never landed: {last}");
    assert_eq!(last["topics"][0]["name"], "Alpha Onboarding");
    assert_eq!(last["topics"][1]["description"], "Invoice exports.");
}

fn map_blocks(conn: &Connection, n: usize) {
    for i in 0..n {
        let desc = if i == 0 { "x".repeat(120) } else { format!("map block {i}") };
        block(conn, &format!("2026-10-{:02}", i + 1), 600, Some(&desc), 0, None);
    }
}

fn map_vectors(n: usize) -> Vec<Vec<f32>> {
    (0..n)
        .map(|i| {
            // Blocks 9 and 22 sit alone on their own axes; the rest are three loose groups.
            let mut v = match i {
                9 => unit(5),
                22 => {
                    // Sits at the group mean on axes 0..3 so centering leaves only its own axis.
                    let mut v = unit(6);
                    v[..3].fill(1.0 / 3.0);
                    v
                }
                _ => unit(i % 3),
            };
            v[10 + i] = 0.1;
            v
        })
        .collect()
}

fn map_report(n: usize) -> crate::topics_contract::TopicsReport {
    let conn = open_memory().unwrap();
    map_blocks(&conn, n);
    store_vectors(&conn, map_vectors(n));
    let server = MockServer::start();
    serve(&server, 200, vec![]);
    topics_report(&conn, &client(&server), day("2026-10-01"), day(&format!("2026-10-{:02}", n.max(11)))).unwrap()
}

#[test]
fn map_points_are_withheld_below_ten_blocks_and_present_at_ten() {
    // catches: `>` instead of `>=` on MAP_MIN_POINTS.
    let nine = map_report(9);
    assert_eq!(nine.status, TopicsStatus::Ready);
    assert!(nine.points.is_empty());
    let ten = map_report(10);
    assert_eq!(ten.points.len(), 10);
}

#[test]
fn map_points_carry_label_topic_novelty_and_oddball_from_the_blocks() {
    let r = map_report(25);
    let by_id = |id: i64| r.points.iter().find(|p| p.block_id == id).unwrap();

    // catches: an untruncated label.
    assert_eq!(by_id(1).label.chars().count(), 90);
    assert_eq!(by_id(2).label, "map block 1");
    assert_eq!(by_id(2).day, "2026-10-02");
    assert_eq!(by_id(2).seconds, 600);
    for p in &r.points {
        assert!(p.topic_id < r.topics.len());
        assert!((0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y), "{p:?}");
    }
    // catches: warm-up novelty leaking (block 1 has nothing before it, raw novelty 1.0).
    assert_eq!(by_id(1).novelty, 0.0);
    assert_eq!(by_id(20).novelty, 0.0);
    // catches: novelty not carried (always 0.0) or `<` vs `<=` shifting the warm-up edge to block 21.
    assert!(by_id(23).novelty > 0.9, "{}", by_id(23).novelty);
    assert!(by_id(22).novelty < 0.1, "{}", by_id(22).novelty);
    // catches: oddball hardcoded or inverted.
    assert!(by_id(10).oddball);
    assert!(!by_id(1).oddball);
}

/// Replies with a name for every theme in the prompt; call `fail_on` (1-based) errors.
struct Scripted {
    calls: &'static AtomicUsize,
    sizes: &'static Mutex<Vec<usize>>,
    fail_on: usize,
}

impl ModelInvoker for Scripted {
    fn invoke(&self, _: &str, user: &str, _: &serde_json::Value, _: &str) -> anyhow::Result<serde_json::Value> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let ids: Vec<usize> = user
            .lines()
            .filter_map(|l| l.strip_prefix("Theme ")?.split(' ').next()?.parse().ok())
            .collect();
        self.sizes.lock().unwrap().push(ids.len());
        if n == self.fail_on {
            anyhow::bail!("scripted failure");
        }
        let topics: Vec<_> = ids
            .iter()
            .map(|id| json!({"id": id, "name": format!("Name {id}"), "description": "D."}))
            .collect();
        Ok(json!({"topics": topics}))
    }
}

fn batch_inputs(base: i64) -> Vec<TopicInput> {
    (0..10)
        .map(|i| TopicInput { members: vec![base + i as i64], ..pending_input(i) })
        .collect()
}

fn blank_report() -> crate::topics_contract::TopicsReport {
    let mut r = crate::topics_contract::TopicsReport::empty(TopicsStatus::Ready, "2026-10-05", "2026-10-11");
    r.naming = Naming::Pending;
    r
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test(flavor = "current_thread")]
async fn ten_topics_are_named_in_batches_of_four_four_two() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static SIZES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    let state = state_from_conn(open_memory().unwrap());
    let inputs = batch_inputs(8000);
    name_in_background(
        &state,
        &mut blank_report(),
        inputs,
        || Ok(Box::new(Scripted { calls: &CALLS, sizes: &SIZES, fail_on: 0 })),
        BACKOFF,
    )
    .await;

    wait_for("ten names", || state.conn.try_lock().is_ok_and(|c| names_rows(&c) == 10)).await;

    // catches: one call for all ten, or an unbatched per-topic loop.
    assert_eq!(*SIZES.lock().unwrap(), vec![4, 4, 2]);
    assert_eq!(CALLS.load(Ordering::SeqCst), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_second_batch_keeps_the_first_batchs_names_and_stops() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static SIZES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    let state = state_from_conn(open_memory().unwrap());
    let inputs = batch_inputs(8100);
    name_in_background(
        &state,
        &mut blank_report(),
        inputs.clone(),
        || Ok(Box::new(Scripted { calls: &CALLS, sizes: &SIZES, fail_on: 2 })),
        BACKOFF,
    )
    .await;

    wait_for("the failure to be recorded", || topic_names::backed_off(&inputs[9], BACKOFF)).await;

    // catches: names stored only after every batch succeeds.
    assert_eq!(names_rows(&*state.conn.lock().await), 4);
    // catches: carrying on to batch three after a failure.
    assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    // catches: backing off topics that were named, or only the failed batch.
    assert!(!topic_names::backed_off(&inputs[0], BACKOFF));
    assert!(!topic_names::backed_off(&inputs[3], BACKOFF));
    assert!(topic_names::backed_off(&inputs[4], BACKOFF));
    assert!(topic_names::backed_off(&inputs[9], BACKOFF));
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_topic_is_not_retried_until_the_window_passes() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static SIZES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    let make: fn() -> anyhow::Result<Box<dyn ModelInvoker>> =
        || Ok(Box::new(Scripted { calls: &CALLS, sizes: &SIZES, fail_on: 1 }));
    let state = state_from_conn(open_memory().unwrap());
    let inputs = batch_inputs(8200)[..3].to_vec();

    name_in_background(&state, &mut blank_report(), inputs.clone(), make, BACKOFF).await;
    wait_for("the failure to be recorded", || topic_names::backed_off(&inputs[0], BACKOFF)).await;
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);

    let mut second = blank_report();
    name_in_background(&state, &mut second, inputs.clone(), make, BACKOFF).await;
    // catches: relaunching on every GET, or leaving the chip on pending.
    assert_eq!(second.naming, Naming::Off);
    let free = topic_names::claim(&inputs);
    assert_eq!(free.len(), 3, "a job was started inside the window");
    topic_names::release(&inputs);
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);

    // catches: a window that never expires.
    let mut third = blank_report();
    name_in_background(&state, &mut third, inputs.clone(), make, Duration::ZERO).await;
    wait_for("the retry", || CALLS.load(Ordering::SeqCst) == 2).await;
    assert_eq!(third.naming, Naming::Pending);
}
