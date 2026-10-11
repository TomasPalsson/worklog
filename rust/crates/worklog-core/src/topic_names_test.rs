use serde_json::json;

use super::*;
use crate::db::open_memory;
use crate::topics_contract::{NamedBy, Topic, TopicsStatus};

fn topic(id: usize) -> Topic {
    Topic {
        id,
        name: String::new(),
        description: None,
        named_by: NamedBy::Keywords,
        keywords: vec![],
        seconds: 3600,
        blocks: 3,
        days_active: 1,
        first_day: String::new(),
        last_day: String::new(),
        top_tickets: vec![],
    }
}

fn input(id: usize, members: &[i64], keywords: &[&str]) -> TopicInput {
    TopicInput {
        id,
        members: members.to_vec(),
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        examples: vec!["first example".into()],
        blocks: members.len() as i64,
        seconds: 3600,
    }
}

fn report(n: usize) -> TopicsReport {
    let mut r = TopicsReport::empty(TopicsStatus::Ready, "2026-10-05", "2026-10-11");
    r.topics = (0..n).map(topic).collect();
    r
}

fn cache(conn: &Connection, members: &[i64], name: &str) {
    conn.execute(
        "INSERT INTO topic_names VALUES (?1, ?2, ?3, 'desc', 'm', '2026-10-01T00:00:00Z')",
        params![member_hash(members), join_ids(members), name],
    )
    .unwrap();
}

fn rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM topic_names", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn member_hash_is_sha256_of_the_sorted_comma_joined_ids() {
    // catches: hashing in input order, or without the "," separator ("12" vs "1,2").
    assert_eq!(member_hash(&[3, 1, 2]), member_hash(&[1, 2, 3]));
    assert_eq!(
        member_hash(&[1, 2, 3]),
        crate::updater::crypto::sha256_hex(b"1,2,3")
    );
    assert_ne!(member_hash(&[1, 23]), member_hash(&[12, 3]));
}

#[test]
fn exact_hash_hit_names_the_topic_and_needs_no_ai() {
    let conn = open_memory().unwrap();
    cache(&conn, &[1, 2, 3], "Cached Name");
    let mut r = report(1);

    let out = resolve(&conn, &mut r, &[input(0, &[3, 2, 1], &["alpha"])]).unwrap();

    assert!(out.pending.is_empty());
    assert_eq!(r.topics[0].name, "Cached Name");
    assert_eq!(r.topics[0].description.as_deref(), Some("desc"));
    assert_eq!(r.topics[0].named_by, NamedBy::Ai);
    // catches: the reuse path inserting a duplicate row on an exact hit.
    assert_eq!(rows(&conn), 1);
}

#[test]
fn jaccard_at_the_threshold_reuses_the_name_and_caches_the_new_hash() {
    let conn = open_memory().unwrap();
    cache(&conn, &[1, 2, 3], "Old Name");
    let mut r = report(1);

    // overlap 3 / union 5 = 0.6 exactly. catches: `>` instead of `>=`.
    let out = resolve(&conn, &mut r, &[input(0, &[1, 2, 3, 4, 5], &["alpha"])]).unwrap();

    assert!(out.pending.is_empty());
    assert_eq!(r.topics[0].name, "Old Name");
    assert_eq!(r.topics[0].named_by, NamedBy::Ai);
    // catches: reusing without inserting a row for the new hash.
    assert_eq!(rows(&conn), 2);
}

#[test]
fn jaccard_below_the_threshold_falls_back_to_keywords_and_stays_pending() {
    let conn = open_memory().unwrap();
    cache(&conn, &[1, 2, 3, 4], "Old Name");
    let mut r = report(1);

    // overlap 4 / union 8 = 0.5. catches: a threshold of 0.5.
    let out = resolve(
        &conn,
        &mut r,
        &[input(
            0,
            &[1, 2, 3, 4, 5, 6, 7, 8],
            &["alpha", "beta", "gamma", "delta"],
        )],
    )
    .unwrap();

    assert_eq!(out.pending.len(), 1);
    assert_eq!(r.topics[0].name, "alpha · beta · gamma");
    assert_eq!(r.topics[0].description, None);
    assert_eq!(r.topics[0].named_by, NamedBy::Keywords);
    assert_eq!(rows(&conn), 1);
}

#[test]
fn only_the_newest_200_rows_are_scanned_for_reuse() {
    let conn = open_memory().unwrap();
    cache(&conn, &[1, 2, 3], "Ancient");
    conn.execute(
        "UPDATE topic_names SET created_at = '2020-01-01T00:00:00Z'",
        [],
    )
    .unwrap();
    for n in 0..200_i64 {
        cache(&conn, &[1000 + n], &format!("Filler {n}"));
    }

    // 3/4 overlap with the 201st-newest row. catches: LIMIT 201, no LIMIT,
    // or an oldest-first ORDER BY.
    let mut r = report(1);
    let out = resolve(&conn, &mut r, &[input(0, &[1, 2, 3, 4], &["alpha"])]).unwrap();
    assert_eq!(out.pending.len(), 1);

    // An exact hash still hits regardless of age.
    let mut r = report(1);
    let out = resolve(&conn, &mut r, &[input(0, &[1, 2, 3], &["alpha"])]).unwrap();
    assert!(out.pending.is_empty());
    assert_eq!(r.topics[0].name, "Ancient");
}

#[test]
fn best_overlap_wins_among_cached_rows() {
    let conn = open_memory().unwrap();
    cache(&conn, &[1, 2, 3, 4, 5, 6], "Weaker");
    cache(&conn, &[1, 2, 3, 4, 5, 7], "Stronger");
    let mut r = report(1);

    // vs Weaker 5/8, vs Stronger 6/7. catches: taking the first row over the threshold.
    resolve(&conn, &mut r, &[input(0, &[1, 2, 3, 4, 5, 7, 8], &["a"])]).unwrap();
    assert_eq!(r.topics[0].name, "Stronger");
}

#[test]
fn keyword_fallback_uses_up_to_three_keywords_then_a_numbered_name() {
    let conn = open_memory().unwrap();
    let mut r = report(3);
    resolve(
        &conn,
        &mut r,
        &[
            input(0, &[1], &["one", "two"]),
            input(1, &[2], &[]),
            input(2, &[3], &["x", "y", "z", "w"]),
        ],
    )
    .unwrap();
    assert_eq!(r.topics[0].name, "one · two");
    assert_eq!(r.topics[1].name, "Topic 2");
    assert_eq!(r.topics[2].name, "x · y · z");
}

#[test]
fn parse_reply_strips_a_json_fence_from_a_string_reply() {
    let reply =
        json!("```json\n[{\"id\": 1, \"name\": \"Billing\", \"description\": \"Invoices.\"}]\n```");
    // catches: serde_json::from_str on the raw fenced text.
    let got = parse_reply(&reply, &[0, 1]).unwrap();
    assert_eq!(got, vec![(1, "Billing".into(), "Invoices.".into())]);
}

#[test]
fn parse_reply_accepts_the_schema_object_and_a_bare_array() {
    let item = json!({"id": 0, "name": "A", "description": "d"});
    let want = vec![(0, "A".to_string(), "d".to_string())];
    assert_eq!(
        parse_reply(&json!({"topics": [item.clone()]}), &[0]).unwrap(),
        want
    );
    assert_eq!(parse_reply(&json!([item]), &[0]).unwrap(), want);
}

#[test]
fn parse_reply_drops_unknown_ids_and_blank_names_and_errors_when_nothing_is_left() {
    let reply = json!([
        {"id": 9, "name": "Ghost", "description": "d"},
        {"id": 0, "name": "  ", "description": "d"},
        {"id": 1, "name": "Real", "description": "d"},
    ]);
    // catches: trusting ids the prompt never asked for; storing blank names.
    assert_eq!(
        parse_reply(&reply, &[0, 1]).unwrap(),
        vec![(1, "Real".to_string(), "d".to_string())]
    );
    assert!(parse_reply(
        &json!([{"id": 9, "name": "Ghost", "description": "d"}]),
        &[0]
    )
    .is_err());
    assert!(parse_reply(&json!("not json at all"), &[0]).is_err());
    assert!(parse_reply(&json!({"other": 1}), &[0]).is_err());
}

#[test]
fn store_makes_the_next_resolve_an_ai_hit() {
    let conn = open_memory().unwrap();
    let pending = vec![input(0, &[4, 5, 6], &["alpha"])];
    store(
        &conn,
        &pending,
        &[(0, "Fresh".into(), "Made today.".into())],
        "m",
    )
    .unwrap();

    let mut r = report(1);
    let out = resolve(&conn, &mut r, &pending).unwrap();
    assert!(out.pending.is_empty());
    assert_eq!(r.topics[0].name, "Fresh");
    assert_eq!(r.topics[0].description.as_deref(), Some("Made today."));
}

#[test]
fn prompt_lists_every_theme_and_carries_the_odds_and_ends_rule() {
    let p = prompt(&[input(7, &[1, 2], &["alpha", "beta"])]);
    assert!(p.contains("Theme 7 (2 blocks, 1.0h)"), "{p}");
    assert!(p.contains("distinctive terms: alpha, beta"), "{p}");
    assert!(p.contains("- first example"), "{p}");
    assert!(SYSTEM_PROMPT.contains("If the items share nothing, name it 'Odds and Ends'."));
    assert!(p.contains("Reply with ONLY a JSON array"), "{p}");
}

#[test]
fn claim_blocks_a_second_job_for_the_same_hashes_until_released() {
    let a = vec![input(0, &[9_000_001], &[])];
    let first = claim(&a);
    assert_eq!(first.len(), 1);
    // catches: no in-flight guard (duplicate model calls).
    assert!(claim(&a).is_empty());
    release(&first);
    assert_eq!(claim(&a).len(), 1);
    release(&a);
}

#[test]
fn batch_size_is_four() {
    assert_eq!(BATCH, 4);
}

#[test]
fn backoff_holds_inside_the_window_and_lifts_at_its_end() {
    let a = input(0, &[7001], &[]);
    let b = input(1, &[7002], &[]);
    let w = std::time::Duration::from_secs(600);
    // catches: a failure that never registers.
    assert!(!backed_off(&a, w));
    record_failure(std::slice::from_ref(&a));
    assert!(backed_off(&a, w));
    // catches: backing off every topic after one failure.
    assert!(!backed_off(&b, w));
    // catches: `<=` instead of `<` — a zero window must never hold.
    assert!(!backed_off(&a, std::time::Duration::ZERO));
}
