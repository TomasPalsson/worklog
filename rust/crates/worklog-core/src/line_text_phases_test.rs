//! Tests for `line_text_phases` — prepare/invoke/commit never holds a
//! connection across the model call, and a manual edit landing between
//! prepare and commit is never overwritten.

use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::db;
use crate::models::Event;
use rusqlite::params;

fn home_work(sub: &str) -> String {
    format!(
        "{}/Desktop/Work/{sub}",
        dirs::home_dir().unwrap().to_string_lossy()
    )
}

fn pin_folder(conn: &Connection, folder: &str, customer: &str) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.to_string(),
            customer: Some(customer.to_string()),
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
}

fn seed_block(conn: &Connection, day: &str, folder: &str, source_id: &str) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds) VALUES (?1, ?2, ?3, ?4)",
        params![
            day,
            "2026-07-01T09:00:00+00:00",
            "2026-07-01T09:30:00+00:00",
            1800
        ],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    let event_id = crate::repo::upsert_event(
        conn,
        &Event {
            project_path: Some(home_work(folder)),
            ..Event::minimal("shell", source_id, "2026-07-01T09:01:00+00:00", "ls")
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();
    block_id
}

const GOOD_TEXT: &str =
    "Verkefninu var lokið í dag. Allt gekk vel og viðskiptavinurinn var ánægður.";

struct GoodInvoker;
impl crate::estimate::ModelInvoker for GoodInvoker {
    fn invoke(
        &self,
        _s: &str,
        _u: &str,
        _sc: &serde_json::Value,
        _m: &str,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({"text": GOOD_TEXT}))
    }
}

#[test]
fn commit_skips_a_manual_edit_that_landed_between_prepare_and_invoke() {
    let conn = db::open_memory().unwrap();
    let day = "2026-07-01";
    let folder = "line-text-phase-race-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(&conn, day, folder, "p1");
    let key = crate::clues_contract::BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let prep = prepare(&conn, &key).unwrap();

    // The race: an edit lands while the model call would have been in
    // flight — after prepare, before invoke/commit run.
    crate::line_text::set_manual(&conn, &key, "Handskrifað af eigandanum sjálfum.").unwrap();

    let reply = invoke(&prep, &GoodInvoker, "model");
    assert_eq!(reply, Ok(GOOD_TEXT.to_string()));

    let result = commit(&conn, &prep, reply);
    assert_eq!(result, Err("hand-edited".to_string()));

    let (text, origin) = crate::line_text::text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, "Handskrifað af eigandanum sjálfum.");
    assert_eq!(origin, crate::clues_contract::LineTextOrigin::Manual);
}

#[test]
fn prepare_invoke_commit_round_trip_generates_and_stores() {
    let conn = db::open_memory().unwrap();
    let day = "2026-07-02";
    let folder = "line-text-phase-roundtrip-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(&conn, day, folder, "p2");
    let key = crate::clues_contract::BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let prep = prepare(&conn, &key).unwrap();
    let reply = invoke(&prep, &GoodInvoker, "model");
    commit(&conn, &prep, reply).unwrap();

    let (text, origin) = crate::line_text::text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, GOOD_TEXT);
    assert_eq!(origin, crate::clues_contract::LineTextOrigin::Generated);
}

/// SLICE T11 round 2: a fake invoker whose `invoke_many` reuses the same
/// production `bounded_concurrent_invoke` helper `ClaudeSubprocess` /
/// `LiteLLMInvoker` do — each call sleeps 200ms. Proves both the timing
/// win (bounded to `MAX_CONCURRENT_INVOKES` at once, not run one at a
/// time) and that `invoke_many` zips every reply back onto its OWN prep
/// regardless of completion order (`bounded_concurrent_invoke` always
/// returns in `users`' order, but a wrong caller could still zip a
/// shuffled `Vec` onto the wrong preps).
struct SlowEchoInvoker;

impl crate::estimate::ModelInvoker for SlowEchoInvoker {
    fn invoke(
        &self,
        _s: &str,
        user: &str,
        _sc: &serde_json::Value,
        _m: &str,
    ) -> anyhow::Result<serde_json::Value> {
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(serde_json::json!({"text": format!("{GOOD_TEXT} ({user})")}))
    }

    fn invoke_many(
        &self,
        system: &str,
        users: &[String],
        schema: &serde_json::Value,
        model: &str,
    ) -> Vec<anyhow::Result<serde_json::Value>> {
        crate::claude_subprocess::bounded_concurrent_invoke(self, system, users, schema, model)
    }
}

#[test]
fn invoke_many_runs_concurrently_and_keeps_reply_order() {
    let preps: Vec<Prep> = (0..6)
        .map(|i| Prep {
            key: crate::clues_contract::BillingLineKey {
                day: "2026-07-04".to_string(),
                folder: format!("line-text-phase-concurrent-folder-{i}"),
                customer: format!("Customer {i}"),
            },
            user_msg: format!("prompt-{i}"),
        })
        .collect();

    let started = std::time::Instant::now();
    let replies = invoke_many(&preps, &SlowEchoInvoker, "model");
    let elapsed = started.elapsed();

    for (i, reply) in replies.iter().enumerate() {
        assert_eq!(reply, &Ok(format!("{GOOD_TEXT} (prompt-{i})")));
    }
    // Sequential would be 6 * 200ms = 1200ms; bounded to 4 concurrent
    // (MAX_CONCURRENT_INVOKES) takes 2 batches, ~400ms. Generous headroom
    // for CI/scheduling noise while still proving it's nowhere near
    // sequential.
    assert!(
        elapsed < std::time::Duration::from_millis(900),
        "invoke_many took {elapsed:?}, expected well under the 1200ms sequential time"
    );
}

#[test]
fn prepare_skips_an_already_manual_line() {
    let conn = db::open_memory().unwrap();
    let key = crate::clues_contract::BillingLineKey {
        day: "2026-07-03".to_string(),
        folder: "line-text-phase-manual-folder".to_string(),
        customer: "Acme Corp".to_string(),
    };
    crate::line_text::set_manual(&conn, &key, "Eigandinn skrifaði þetta sjálfur.").unwrap();
    assert_eq!(prepare(&conn, &key).err(), Some("hand-edited".to_string()));
}
