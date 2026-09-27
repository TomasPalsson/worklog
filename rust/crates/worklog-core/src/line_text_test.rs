//! Tests for T020 — `line_text::validate` / `generate_for_day` /
//! `set_manual` / `text_for` (spec 006, D-12, D-14, FR-26, FR-31, FR-35).

use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::db;
use crate::estimate::FixedInvoker;
use crate::models::Event;
use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::json;

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

#[allow(clippy::too_many_arguments)]
fn seed_block(
    conn: &Connection,
    day: &str,
    started_at: &str,
    ended_at: &str,
    duration_seconds: i64,
    folder: &str,
    source_id: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![day, started_at, ended_at, duration_seconds],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    let event_id = crate::repo::upsert_event(
        conn,
        &Event {
            project_path: Some(home_work(folder)),
            ..Event::minimal("shell", source_id, started_at, "ls")
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        rusqlite::params![block_id, event_id],
    )
    .unwrap();
    block_id
}

const GOOD_TEXT: &str =
    "Verkefninu var lokið í dag. Allt gekk vel og viðskiptavinurinn var ánægður.";
const ENGLISH_TEXT: &str = "Fixed the login bug. Added tests.";

struct ErrInvoker;
impl crate::estimate::ModelInvoker for ErrInvoker {
    fn invoke(
        &self,
        _s: &str,
        _u: &str,
        _sc: &serde_json::Value,
        _m: &str,
    ) -> anyhow::Result<serde_json::Value> {
        Err(anyhow!("model unreachable"))
    }
}

#[test]
fn line_text_validate_accepts_good_icelandic_text() {
    assert_eq!(validate(GOOD_TEXT).unwrap(), GOOD_TEXT);
}

#[test]
fn line_text_validate_rejects_one_sentence() {
    assert!(validate("Verkefninu var lokið í dag").is_err());
}

#[test]
fn line_text_validate_rejects_four_sentences() {
    let text = "Það var gaman. Ég gerði hitt. Svo gerði ég þetta. Loks kláraði ég verkið.";
    assert!(validate(text).is_err());
}

#[test]
fn line_text_validate_rejects_too_long() {
    let text = format!("Þetta er löng setning. {}", "a".repeat(450));
    assert!(validate(&text).is_err());
}

#[test]
fn line_text_validate_rejects_digits() {
    assert!(validate("PR 12").is_err());
}

#[test]
fn line_text_validate_rejects_hash() {
    assert!(validate("#44").is_err());
}

#[test]
fn line_text_validate_rejects_jira_key() {
    assert!(validate("ABC-12").is_err());
}

#[test]
fn line_text_validate_rejects_path() {
    assert!(validate("src/main.rs").is_err());
}

#[test]
fn line_text_validate_rejects_bare_filename() {
    assert!(validate("main.rs").is_err());
}

#[test]
fn line_text_validate_rejects_tool_name() {
    assert!(validate("Bash").is_err());
}

#[test]
fn line_text_validate_rejects_english_only() {
    assert!(validate(ENGLISH_TEXT).is_err());
}

#[test]
fn line_text_validate_rejects_json_wrapper() {
    let wrapped = format!("{{\"text\": \"{GOOD_TEXT}\"}}");
    assert!(validate(&wrapped).is_err());
}

#[test]
fn line_text_unwraps_text_nested_as_json_inside_text() {
    // Seen from real `claude -p`: the schema's `text` held the whole
    // `{"text": "..."}` object again as a string.
    let conn = db::open_memory().unwrap();
    let day = "2026-06-01";
    pin_folder(&conn, "line-text-folder-a", "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-01T09:00:00+00:00",
        "2026-06-01T09:30:00+00:00",
        1800,
        "line-text-folder-a",
        "l1",
    );
    let nested = json!({ "text": GOOD_TEXT }).to_string();
    let invoker = FixedInvoker(json!({ "text": nested }));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();
    let (text, _) = text_for(&conn, &report.generated[0]).unwrap().unwrap();
    assert_eq!(text, GOOD_TEXT);
}

#[test]
fn line_text_generate_for_day_stores_one_row_per_line_key() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-01";
    pin_folder(&conn, "line-text-folder-a", "Acme Corp");
    pin_folder(&conn, "line-text-folder-b", "Beta ehf");
    seed_block(
        &conn,
        day,
        "2026-06-01T09:00:00+00:00",
        "2026-06-01T09:30:00+00:00",
        1800,
        "line-text-folder-a",
        "l1",
    );
    seed_block(
        &conn,
        day,
        "2026-06-01T10:00:00+00:00",
        "2026-06-01T10:30:00+00:00",
        1800,
        "line-text-folder-b",
        "l2",
    );

    let invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();

    assert_eq!(
        report.generated.len(),
        2,
        "not_generated: {:?}",
        report.not_generated
    );
    assert!(report.not_generated.is_empty());
    for key in &report.generated {
        let (text, origin) = text_for(&conn, key).unwrap().unwrap();
        assert_eq!(text, GOOD_TEXT);
        assert_eq!(origin, LineTextOrigin::Generated);
    }
}

#[test]
fn line_text_manual_never_overwritten() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-02";
    let folder = "line-text-manual-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-02T09:00:00+00:00",
        "2026-06-02T09:30:00+00:00",
        1800,
        folder,
        "m1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };
    let manual_text = "Handskrifaður texti sem eigandinn skrifaði sjálfur.";
    set_manual(&conn, &key, manual_text).unwrap();

    let invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(!report.not_generated.iter().any(|(k, _)| k == &key));
    let (text, origin) = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, manual_text);
    assert_eq!(origin, LineTextOrigin::Manual);
}

#[test]
fn line_text_failure_keeps_previous_text_on_invoker_error() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-03";
    let folder = "line-text-fail-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-03T09:00:00+00:00",
        "2026-06-03T09:30:00+00:00",
        1800,
        folder,
        "f1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let good_invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let first = generate_for_day(&conn, day, &good_invoker, "model").unwrap();
    assert!(first.generated.contains(&key));
    let before = text_for(&conn, &key).unwrap().unwrap();

    let report = generate_for_day(&conn, day, &ErrInvoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(report.not_generated.iter().any(|(k, _)| k == &key));
    let after = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(before, after);
}

#[test]
fn line_text_failure_keeps_previous_text_on_invalid_reply() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-04";
    let folder = "line-text-invalid-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-04T09:00:00+00:00",
        "2026-06-04T09:30:00+00:00",
        1800,
        folder,
        "i1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let good_invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    generate_for_day(&conn, day, &good_invoker, "model").unwrap();
    let before = text_for(&conn, &key).unwrap().unwrap();

    let bad_invoker = FixedInvoker(json!({"text": ENGLISH_TEXT}));
    let report = generate_for_day(&conn, day, &bad_invoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(report.not_generated.iter().any(|(k, _)| k == &key));
    let after = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(before, after);
}

#[test]
fn line_text_set_manual_empty_deletes_row() {
    let conn = db::open_memory().unwrap();
    let key = BillingLineKey {
        day: "2026-06-05".to_string(),
        folder: "line-text-delete-folder".to_string(),
        customer: "Acme Corp".to_string(),
    };
    set_manual(&conn, &key, "Eitthvað sem eigandinn skrifaði.").unwrap();
    assert!(text_for(&conn, &key).unwrap().is_some());

    set_manual(&conn, &key, "   ").unwrap();
    assert!(text_for(&conn, &key).unwrap().is_none());
}

#[test]
fn line_text_text_for_round_trips_origin() {
    let conn = db::open_memory().unwrap();
    let key = BillingLineKey {
        day: "2026-06-06".to_string(),
        folder: "line-text-roundtrip-folder".to_string(),
        customer: "Acme Corp".to_string(),
    };
    set_manual(&conn, &key, "Handskrifaður texti með ö og allt sem virkar.").unwrap();
    let (text, origin) = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, "Handskrifaður texti með ö og allt sem virkar.");
    assert_eq!(origin, LineTextOrigin::Manual);
}

#[test]
fn line_text_invoker_error_reports_every_line_key_not_generated() {
    // FR-35: when the model invoker itself can't be built (misconfigured
    // provider), every distinct billing line of the day must surface in
    // `not_generated` with that reason — not silently vanish into an
    // empty report (the previous behaviour daemon.rs's `run_estimate`
    // fell back to on any `Err` from `generate_with_default_provider`).
    let conn = db::open_memory().unwrap();
    let day = "2026-06-07";
    pin_folder(&conn, "line-text-folder-err-a", "Acme Corp");
    pin_folder(&conn, "line-text-folder-err-b", "Beta ehf");
    seed_block(
        &conn,
        day,
        "2026-06-07T09:00:00+00:00",
        "2026-06-07T09:30:00+00:00",
        1800,
        "line-text-folder-err-a",
        "e1",
    );
    seed_block(
        &conn,
        day,
        "2026-06-07T10:00:00+00:00",
        "2026-06-07T10:30:00+00:00",
        1800,
        "line-text-folder-err-b",
        "e2",
    );

    let report = report_for_invoker_error(&conn, day, "provider not configured").unwrap();
    assert!(report.generated.is_empty());
    assert_eq!(report.not_generated.len(), 2);
    assert!(report
        .not_generated
        .iter()
        .all(|(_, reason)| reason == "provider not configured"));
}
