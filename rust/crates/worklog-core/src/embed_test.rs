use super::*;
use crate::db::open_memory;
use httpmock::prelude::*;
use serde_json::json;

fn unit(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; EMBED_DIM];
    v[i] = 1.0;
    v
}

fn at(server: &MockServer) -> EmbedClient {
    EmbedClient::with_client(reqwest::blocking::Client::new(), server.base_url())
}

fn reply(server: &MockServer, vectors: Vec<Vec<f32>>) -> httpmock::Mock<'_> {
    server.mock(|when, then| {
        when.method(POST).path("/embed");
        then.status(200).json_body(json!({ "vectors": vectors }));
    })
}

fn block(conn: &Connection, description: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, description)
         VALUES ('2026-04-20', '2026-04-20T09:00:00Z', '2026-04-20T10:00:00Z', 3600, ?1)",
        params![description],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn event(conn: &Connection, block_id: i64, source: &str, title: &str, details: &str, at: &str) {
    conn.execute(
        "INSERT INTO events (source, source_id, started_at, title, details)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            source,
            format!("{source}-{title}-{at}-{details}"),
            at,
            title,
            details
        ],
    )
    .unwrap();
    let eid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();
}

fn prompt(conn: &Connection, block_id: i64, details: &str, at: &str) {
    event(conn, block_id, "claude", "UserPromptSubmit", details, at);
}

fn text(conn: &Connection, id: i64) -> Option<String> {
    block_text(conn, id).unwrap()
}

#[test]
fn production_embed_timeout_is_thirty_seconds() {
    assert_eq!(EMBED_TIMEOUT, std::time::Duration::from_secs(30));
}

#[test]
fn text_strips_jira_keys_from_description_and_prompts() {
    let conn = open_memory().unwrap();
    let b = block(&conn, Some("PROJ-12 fix login"));
    prompt(
        &conn,
        b,
        "please look at AB1-7 and the session",
        "2026-04-20T09:01:00Z",
    );
    // catches: leaving keys in (they would cluster blocks by ticket, not topic)
    // and failing to trim the joined text.
    assert_eq!(
        text(&conn, b).unwrap(),
        "fix login please look at  and the session"
    );
}

#[test]
fn text_keeps_lowercase_and_digitless_lookalikes() {
    let conn = open_memory().unwrap();
    let b = block(&conn, Some("proj-12 and ABC- and A-1 done"));
    // catches: a case-insensitive pattern, or one that accepts a one-letter prefix.
    assert_eq!(text(&conn, b).unwrap(), "proj-12 and ABC- and A-1 done");
}

#[test]
fn text_is_none_when_nothing_to_embed() {
    let conn = open_memory().unwrap();
    let null = block(&conn, None);
    let blank = block(&conn, Some("   "));
    let only_key = block(&conn, Some("PROJ-12"));
    // catches: Some("") for an empty block, a missing trim, trimming before stripping.
    assert_eq!(text(&conn, null), None);
    assert_eq!(text(&conn, blank), None);
    assert_eq!(text(&conn, only_key), None);
}

#[test]
fn text_is_none_for_an_unknown_block() {
    let conn = open_memory().unwrap();
    // catches: surfacing "no rows" as an error.
    assert_eq!(text(&conn, 999), None);
}

#[test]
fn text_uses_prompts_alone_when_description_is_empty() {
    let conn = open_memory().unwrap();
    let b = block(&conn, None);
    prompt(
        &conn,
        b,
        "a prompt that is long enough",
        "2026-04-20T09:01:00Z",
    );
    // catches: requiring a description before looking at prompts.
    assert_eq!(text(&conn, b).unwrap(), "a prompt that is long enough");
}

#[test]
fn prompt_length_filter_is_strictly_over_twenty() {
    let conn = open_memory().unwrap();
    let b = block(&conn, Some("d"));
    prompt(&conn, b, &"x".repeat(20), "2026-04-20T09:01:00Z");
    prompt(&conn, b, &"y".repeat(21), "2026-04-20T09:02:00Z");
    // catches: >= 20 (the 20-char prompt would appear).
    assert_eq!(text(&conn, b).unwrap(), format!("d {}", "y".repeat(21)));
}

#[test]
fn only_claude_prompt_submit_events_count() {
    let conn = open_memory().unwrap();
    let b = block(&conn, Some("d"));
    let long = "z".repeat(30);
    event(
        &conn,
        b,
        "git",
        "UserPromptSubmit",
        &long,
        "2026-04-20T09:01:00Z",
    );
    event(&conn, b, "claude", "Stop", &long, "2026-04-20T09:02:00Z");
    event(
        &conn,
        b,
        "claude",
        "UserPromptSubmit: fix",
        &"w".repeat(30),
        "2026-04-20T09:03:00Z",
    );
    // catches: dropping the source filter, dropping the title filter,
    // or an exact title match instead of the 'UserPromptSubmit%' prefix.
    assert_eq!(text(&conn, b).unwrap(), format!("d {}", "w".repeat(30)));
}

#[test]
fn prompts_are_oldest_first_and_capped_at_five() {
    let conn = open_memory().unwrap();
    let b = block(&conn, Some("d"));
    // inserted newest first so row order differs from time order
    for n in (1..=6).rev() {
        prompt(
            &conn,
            b,
            &format!("prompt number {n} is long enough"),
            &format!("2026-04-20T09:0{n}:00Z"),
        );
    }
    // catches: insertion-order or newest-first selection, and a cap of 6.
    assert_eq!(
        text(&conn, b).unwrap(),
        "d prompt number 1 is long enough prompt number 2 is long enough \
         prompt number 3 is long enough prompt number 4 is long enough \
         prompt number 5 is long enough"
    );
}

#[test]
fn prompt_is_cut_at_exactly_three_hundred_chars() {
    let conn = open_memory().unwrap();
    let exact = block(&conn, None);
    prompt(&conn, exact, &"é".repeat(300), "2026-04-20T09:01:00Z");
    let over = block(&conn, None);
    prompt(&conn, over, &"é".repeat(301), "2026-04-20T09:01:00Z");
    // catches: cutting at 299, not cutting, and cutting by bytes (é is 2 bytes).
    assert_eq!(text(&conn, exact).unwrap(), "é".repeat(300));
    assert_eq!(text(&conn, over).unwrap(), "é".repeat(300));
}

#[test]
fn prompt_keys_are_stripped_before_the_cut() {
    let conn = open_memory().unwrap();
    let b = block(&conn, None);
    prompt(
        &conn,
        b,
        &format!("PROJ-123 {}", "q".repeat(300)),
        "2026-04-20T09:01:00Z",
    );
    // stripped " qqq..." keeps 300 chars = a space + 299 q's; trimmed -> 299.
    // catches: cutting first (would leave 291 q's) and then stripping.
    assert_eq!(text(&conn, b).unwrap(), "q".repeat(299));
}

#[test]
fn ensure_returns_vectors_by_block_id_in_input_order() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let b = block(&conn, Some("beta"));
    let server = MockServer::start();
    reply(&server, vec![unit(0), unit(1)]);
    let got = ensure_vectors(&conn, &at(&server), &[a, b]).unwrap();
    // catches: reversed pairing and returning fewer/more entries than ids.
    assert_eq!(got.len(), 2);
    assert_eq!(got[&a], unit(0));
    assert_eq!(got[&b], unit(1));
}

#[test]
fn ensure_skips_blocks_without_text() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let empty = block(&conn, None);
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(POST)
            .path("/embed")
            .json_body(json!({ "texts": ["alpha"] }));
        then.status(200).json_body(json!({ "vectors": [unit(0)] }));
    });
    let got = ensure_vectors(&conn, &at(&server), &[a, empty]).unwrap();
    // catches: sending an empty string to the helper or inventing a vector for it.
    m.assert_hits(1);
    assert_eq!(got.keys().copied().collect::<Vec<_>>(), vec![a]);
}

#[test]
fn ensure_does_not_re_embed_an_unchanged_block() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let server = MockServer::start();
    let m = reply(&server, vec![unit(3)]);
    let first = ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    let second = ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    // catches: no cache, and a cache that stores but does not read back the vector.
    m.assert_hits(1);
    assert_eq!(first[&a], unit(3));
    assert_eq!(second[&a], unit(3));
}

#[test]
fn ensure_re_embeds_when_the_text_changed() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let server = MockServer::start();
    let m = reply(&server, vec![unit(0)]);
    ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    conn.execute(
        "UPDATE blocks SET description = 'alpha two' WHERE id = ?1",
        [a],
    )
    .unwrap();
    ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    // catches: keying the cache on block_id alone.
    m.assert_hits(2);
}

#[test]
fn ensure_re_embeds_when_the_model_revision_changed() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let server = MockServer::start();
    let m = reply(&server, vec![unit(0)]);
    ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    conn.execute("UPDATE block_embeddings SET model = 'old-revision'", [])
        .unwrap();
    ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    // catches: ignoring the model column.
    m.assert_hits(2);
}

#[test]
fn ensure_re_embeds_a_wrong_length_stored_vector() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let server = MockServer::start();
    let m = reply(&server, vec![unit(0)]);
    ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    conn.execute("UPDATE block_embeddings SET vec = x'00010203'", [])
        .unwrap();
    let got = ensure_vectors(&conn, &at(&server), &[a]).unwrap();
    // catches: returning a truncated vector (or panicking on it) instead of re-embedding.
    m.assert_hits(2);
    assert_eq!(got[&a], unit(0));
}

#[test]
fn ensure_sends_exactly_one_request_for_a_full_batch() {
    let conn = open_memory().unwrap();
    let ids: Vec<i64> = (0..EMBED_BATCH)
        .map(|n| block(&conn, Some(&format!("b{n}"))))
        .collect();
    let server = MockServer::start();
    let m = reply(&server, (0..EMBED_BATCH).map(unit).collect());
    ensure_vectors(&conn, &at(&server), &ids).unwrap();
    // catches: an extra empty request after a full chunk.
    m.assert_hits(1);
}

#[test]
fn ensure_chunks_into_embed_batch_sized_requests() {
    let conn = open_memory().unwrap();
    let ids: Vec<i64> = (0..EMBED_BATCH * 2)
        .map(|n| block(&conn, Some(&format!("b{n}"))))
        .collect();
    let server = MockServer::start();
    let m = reply(&server, (0..EMBED_BATCH).map(unit).collect());
    let got = ensure_vectors(&conn, &at(&server), &ids).unwrap();
    // catches: one oversized request, and a batch smaller than EMBED_BATCH.
    m.assert_hits(2);
    assert_eq!(got.len(), EMBED_BATCH * 2);
}

#[test]
fn ensure_fails_when_the_helper_is_down_and_stores_nothing() {
    let conn = open_memory().unwrap();
    let a = block(&conn, Some("alpha"));
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let down = EmbedClient::with_client(
        reqwest::blocking::Client::new(),
        format!("http://127.0.0.1:{port}"),
    );
    // catches: swallowing the connection error into an empty map.
    assert!(ensure_vectors(&conn, &down, &[a]).is_err());
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM block_embeddings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn embed_rejects_an_error_status() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/embed");
        then.status(500).body("boom");
    });
    // catches: parsing the body of a 500 as success.
    assert!(at(&server).embed(&["a".to_string()]).is_err());
}

#[test]
fn embed_rejects_a_vector_count_mismatch() {
    let server = MockServer::start();
    reply(&server, vec![unit(0)]);
    // catches: zipping a short reply against the texts and silently dropping blocks.
    assert!(at(&server)
        .embed(&["a".to_string(), "b".to_string()])
        .is_err());
}

#[test]
fn embed_rejects_a_wrong_dimension_vector() {
    let server = MockServer::start();
    reply(&server, vec![vec![1.0, 0.0, 0.0]]);
    // catches: storing a vector that would later poison every cosine.
    assert!(at(&server).embed(&["a".to_string()]).is_err());
}

#[test]
fn from_env_honours_verdict_url_then_falls_back_to_classifier_addr() {
    let _g = crate::tz::test_env_lock();
    std::env::set_var("WORKLOG_VERDICT_URL", "http://127.0.0.1:1");
    assert_eq!(EmbedClient::from_env().base_url, "http://127.0.0.1:1");
    std::env::remove_var("WORKLOG_VERDICT_URL");
    assert_eq!(
        EmbedClient::from_env().base_url,
        format!("http://{CLASSIFIER_ADDR}")
    );
}

#[test]
fn embed_all_validates_every_batch_and_stores_nothing_on_a_late_failure() {
    let conn = open_memory().unwrap();
    let ids: Vec<i64> = (0..EMBED_BATCH + 8)
        .map(|n| block(&conn, Some(&format!("b{n}"))))
        .collect();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/embed").body_contains("\"b0\"");
        then.status(200)
            .json_body(json!({ "vectors": (0..EMBED_BATCH).map(unit).collect::<Vec<_>>() }));
    });
    server.mock(|when, then| {
        when.method(POST).path("/embed").body_contains("\"b39\"");
        then.status(200).json_body(json!({ "vectors": [unit(0)] }));
    });
    // catches: storing batch 1 before batch 2 is known to be good (FR-19).
    assert!(ensure_vectors(&conn, &at(&server), &ids).is_err());
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM block_embeddings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
