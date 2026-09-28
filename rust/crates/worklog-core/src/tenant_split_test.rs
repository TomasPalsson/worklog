use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use rand::Rng;
use rusqlite::params;

use super::*;
use crate::billing_registry::{Customer, FolderMap};
use crate::db::open_memory;
use crate::models::{Block, Event};
use crate::session_pins;
use crate::tenant_contract::ClueStrength;

fn registry(customer_names: &[&str], folders: &[FolderMap]) -> Registry {
    Registry {
        customers: customer_names
            .iter()
            .map(|n| Customer {
                id: None,
                name: (*n).to_string(),
                aliases: Vec::new(),
            })
            .collect(),
        folders: folders.to_vec(),
    }
}

fn multi_tenant_folder(folder: &str) -> FolderMap {
    FolderMap {
        id: None,
        folder: folder.to_string(),
        customer: None,
        verkefni: None,
        billable: true,
        multi_tenant: true,
    }
}

fn at(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
}

fn clue(when: DateTime<Utc>, customer: &str, strength: ClueStrength) -> Clue {
    Clue {
        at: when,
        customer: customer.to_string(),
        strength,
    }
}

fn work(sub: &str) -> String {
    format!(
        "{}/Desktop/Work/{sub}",
        dirs::home_dir().unwrap().to_string_lossy()
    )
}

fn seed_block(conn: &Connection, started_at: &str, ended_at: &str, duration_seconds: i64) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds, description, is_personal)
         VALUES ('2026-01-01', NULL, ?1, ?2, ?3, NULL, 0)",
        params![started_at, ended_at, duration_seconds],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(
    conn: &Connection,
    block_id: i64,
    source_id: &str,
    session_id: &str,
    started_at: &str,
) {
    seed_titled_event(conn, block_id, source_id, session_id, started_at, "work");
}

fn seed_titled_event(
    conn: &Connection,
    block_id: i64,
    source_id: &str,
    session_id: &str,
    started_at: &str,
    title: &str,
) {
    let mut ev = Event::minimal("claude", source_id, started_at, title);
    ev.session_id = Some(session_id.to_string());
    // Every test in this file works in the "vitinn-infra" folder — a real
    // Claude hook event always carries the cwd it fired from
    // (`InferEvent::project_path`'s doc comment), which `session_contexts`
    // (via `infer_lanes::lane_folder`) needs to find this session's day-wide
    // context at all.
    ev.project_path = Some(work("vitinn-infra"));
    let event_id = crate::repo::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, event_id],
    )
    .unwrap();
}

// B5 / FR-06: "Block with clues Sjúkra@13:40, MMS@15:30 splits at the
// midpoint; the midpoint second goes to Sjúkra" — the spec's own example.
#[test]
fn split_gives_seconds_to_nearest_clue() {
    let start = at(2026, 1, 1, 12, 0, 0).timestamp();
    let end = at(2026, 1, 1, 17, 0, 0).timestamp();
    let clues = vec![
        clue(at(2026, 1, 1, 13, 40, 0), "Sjúkra", ClueStrength::Branch),
        clue(at(2026, 1, 1, 15, 30, 0), "MMS", ClueStrength::Branch),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(slices.len(), 2);

    // 13:40 to 15:30 is 110 minutes; the midpoint clock time is 14:35:00,
    // and that second belongs to Sjúkra (the earlier clue) — so Sjúkra's
    // slice runs up to and including 14:35:00, i.e. ends at 14:35:01.
    let boundary = at(2026, 1, 1, 14, 35, 1).timestamp();

    let sjukra = slices
        .iter()
        .find(|s| s.customer.as_deref() == Some("Sjúkra"))
        .unwrap();
    assert_eq!(sjukra.intervals, vec![(start, boundary)]);
    assert_eq!(sjukra.origin, SplitOrigin::Clues);

    let mms = slices
        .iter()
        .find(|s| s.customer.as_deref() == Some("MMS"))
        .unwrap();
    assert_eq!(mms.intervals, vec![(boundary, end)]);
    assert_eq!(mms.origin, SplitOrigin::Clues);
}

// B6 / FR-07: "Block with APRÓ + Sjúkra clues → 100% Sjúkra".
#[test]
fn house_clues_dropped_when_other_customer_present() {
    let start = at(2026, 1, 1, 9, 0, 0).timestamp();
    let end = at(2026, 1, 1, 11, 0, 0).timestamp();
    let clues = vec![
        clue(
            at(2026, 1, 1, 9, 15, 0),
            HOUSE_CUSTOMER,
            ClueStrength::Branch,
        ),
        clue(at(2026, 1, 1, 10, 0, 0), "Sjúkra", ClueStrength::Branch),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start, end)],
            origin: SplitOrigin::Clues,
        }]
    );
}

// B7 / FR-08: "`mms` branch + `sjukra` path in one minute → Sjúkra"
// (path beats branch).
#[test]
fn stronger_clue_wins_its_minute() {
    let start = at(2026, 1, 1, 10, 0, 0).timestamp();
    let end = at(2026, 1, 1, 10, 1, 0).timestamp();
    let clues = vec![
        clue(at(2026, 1, 1, 10, 0, 5), "MMS", ClueStrength::Branch),
        clue(
            at(2026, 1, 1, 10, 0, 40),
            "Sjúkra",
            ClueStrength::TenantPath,
        ),
    ];

    let slices = split_block(start, end, &clues).unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start, end)],
            origin: SplitOrigin::Clues,
        }]
    );
}

// B8 / FR-09: no timestamped clue → `split_block` is `None`, and
// `tenant_slices_for_block` falls back to a `Fallback` slice for billing to
// resolve (here: no events, no matching summary text).
#[test]
fn no_clue_block_falls_back() {
    assert_eq!(split_block(0, 100, &[]), None);

    let conn = open_memory().unwrap();
    let reg = registry(
        &["Sjúkra", HOUSE_CUSTOMER],
        &[multi_tenant_folder("vitinn-infra")],
    );
    let start = at(2026, 1, 1, 9, 0, 0);
    let block = Block {
        id: 1,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: at(2026, 1, 1, 10, 0, 0).to_rfc3339(),
        duration_seconds: 3600,
        description: Some("Unrelated maintenance work".to_string()),
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: None,
            intervals: vec![(start.timestamp(), start.timestamp() + 3600)],
            origin: SplitOrigin::Fallback,
        }]
    );
}

// B9 / FR-12: over random clues, slices are disjoint and sum exactly to the
// block's duration.
#[test]
fn slices_sum_exactly_property() {
    let mut rng = rand::thread_rng();
    let customers = ["Sjúkra", "MMS", HOUSE_CUSTOMER];
    let strengths = [
        ClueStrength::Summary,
        ClueStrength::Branch,
        ClueStrength::TenantPath,
    ];
    let start = 1_700_000_000_i64;

    for _ in 0..1000 {
        let duration = rng.gen_range(1..=3600);
        let end = start + duration;
        let clue_count = rng.gen_range(1..=6);
        let clues: Vec<Clue> = (0..clue_count)
            .map(|_| Clue {
                at: DateTime::<Utc>::from_timestamp(rng.gen_range(start..end), 0).unwrap(),
                customer: customers[rng.gen_range(0..customers.len())].to_string(),
                strength: strengths[rng.gen_range(0..strengths.len())],
            })
            .collect();

        let slices = split_block(start, end, &clues)
            .expect("clues is non-empty, split_block must produce a split");

        let mut intervals: Vec<(i64, i64)> =
            slices.iter().flat_map(|s| s.intervals.clone()).collect();
        intervals.sort_by_key(|&(from, _)| from);

        let mut cursor = start;
        for (from, to) in &intervals {
            assert_eq!(*from, cursor, "slices must never gap or overlap");
            cursor = *to;
        }
        assert_eq!(
            cursor, end,
            "slices must sum exactly to the block's duration"
        );
    }
}

// FR-07: a block whose only session is pinned to Sjúkra from before the
// block starts gets a single Pinned slice for the whole block, ahead of
// clue splitting.
#[test]
fn pinned_session_gives_pinned_slice() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra"], &[multi_tenant_folder("vitinn-infra")]);

    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Sjúkra",
        at(2026, 1, 1, 8, 0, 0),
        None,
    )
    .unwrap();

    let start = at(2026, 1, 1, 9, 0, 0);
    let end = at(2026, 1, 1, 10, 0, 0);
    let block_id = seed_block(&conn, &start.to_rfc3339(), &end.to_rfc3339(), 3600);
    seed_event(&conn, block_id, "e1", "sess-1", &start.to_rfc3339());

    let block = Block {
        id: block_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: end.to_rfc3339(),
        duration_seconds: 3600,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start.timestamp(), end.timestamp())],
            origin: SplitOrigin::Pinned,
        }]
    );
}

// Contract THE FIVE #2: a block whose sessions' pins name two different
// customers emits no Pinned slice — it falls through to the fallback rules.
#[test]
fn conflicting_pins_do_not_emit_pinned_slice() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra", "MMS"], &[multi_tenant_folder("vitinn-infra")]);

    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Sjúkra",
        at(2026, 1, 1, 8, 0, 0),
        None,
    )
    .unwrap();
    session_pins::pin(
        &conn,
        &reg,
        "sess-2",
        Path::new(&work("vitinn-infra")),
        "MMS",
        at(2026, 1, 1, 8, 0, 0),
        None,
    )
    .unwrap();

    let start = at(2026, 1, 1, 9, 0, 0);
    let end = at(2026, 1, 1, 10, 0, 0);
    let block_id = seed_block(&conn, &start.to_rfc3339(), &end.to_rfc3339(), 3600);
    seed_event(&conn, block_id, "e1", "sess-1", &start.to_rfc3339());
    seed_event(&conn, block_id, "e2", "sess-2", &start.to_rfc3339());

    let block = Block {
        id: block_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: end.to_rfc3339(),
        duration_seconds: 3600,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert!(slices.iter().all(|s| s.origin != SplitOrigin::Pinned));
}

// FR-07 / billing.rs's "nothing that lands on an invoice is invented": a
// pin's from_at must cover EVERY session event in the block, not just some
// of them — an event before the pin took effect must not be swept into the
// pinned customer's slice.
#[test]
fn partially_pinned_block_is_not_pinned() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra"], &[multi_tenant_folder("vitinn-infra")]);

    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Sjúkra",
        at(2026, 1, 1, 9, 30, 0),
        None,
    )
    .unwrap();

    let start = at(2026, 1, 1, 9, 0, 0);
    let end = at(2026, 1, 1, 10, 0, 0);
    let block_id = seed_block(&conn, &start.to_rfc3339(), &end.to_rfc3339(), 3600);
    seed_event(&conn, block_id, "e1", "sess-1", &start.to_rfc3339());
    seed_event(
        &conn,
        block_id,
        "e2",
        "sess-1",
        &at(2026, 1, 1, 9, 45, 0).to_rfc3339(),
    );

    let block = Block {
        id: block_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: end.to_rfc3339(),
        duration_seconds: 3600,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert!(slices.iter().all(|s| s.origin != SplitOrigin::Pinned));
}

// Finding A: a REAL session always records a SessionStart hook row (and
// usually the first prompt) before Claude gets around to running `worklog
// pin` — the race that made Pinned origin never fire on real data.
// SessionStart at t0, first prompt at t0+5s, pin at t0+60s, tool events
// after: the setup-race reach-back (session_pins::SETUP_GRACE) covers the
// SessionStart/prompt gap, so the whole block is one Pinned slice.
#[test]
fn real_session_start_is_pinned() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Sjúkra"], &[multi_tenant_folder("vitinn-infra")]);

    let t0 = at(2026, 1, 1, 9, 0, 0);
    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Sjúkra",
        t0 + chrono::Duration::seconds(60),
        None,
    )
    .unwrap();

    let start = t0;
    let end = at(2026, 1, 1, 10, 0, 0);
    let block_id = seed_block(&conn, &start.to_rfc3339(), &end.to_rfc3339(), 3600);
    seed_titled_event(
        &conn,
        block_id,
        "hook-1",
        "sess-1",
        &t0.to_rfc3339(),
        "SessionStart",
    );
    seed_titled_event(
        &conn,
        block_id,
        "hook-2",
        "sess-1",
        &(t0 + chrono::Duration::seconds(5)).to_rfc3339(),
        "UserPromptSubmit",
    );
    seed_titled_event(
        &conn,
        block_id,
        "hook-3",
        "sess-1",
        &(t0 + chrono::Duration::seconds(120)).to_rfc3339(),
        "PostToolUse",
    );
    seed_titled_event(
        &conn,
        block_id,
        "hook-4",
        "sess-1",
        &(t0 + chrono::Duration::seconds(180)).to_rfc3339(),
        "PostToolUse",
    );

    let block = Block {
        id: block_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: end.to_rfc3339(),
        duration_seconds: 3600,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert_eq!(
        slices,
        vec![CustomerSlice {
            customer: Some("Sjúkra".to_string()),
            intervals: vec![(start.timestamp(), end.timestamp())],
            origin: SplitOrigin::Pinned,
        }]
    );
}

// Finding P2: an event that resolves to its session's own text guess
// (not the pin) must block the Pinned origin — a block can't go Pinned
// to a customer the lanes would tag differently. A pre-pin event titled
// "Globex ticket work" (Globex a registered customer) names Globex via
// text; the setup-race reach-back must lose to that text guess exactly
// like `session_customers::resolve_events` does, so a pin to Acme 5
// minutes later must NOT turn this into a Pinned-to-Acme block.
#[test]
fn text_guess_disagreeing_with_pin_blocks_pinned_slice() {
    let conn = open_memory().unwrap();
    let reg = registry(&["Globex", "Acme"], &[multi_tenant_folder("vitinn-infra")]);

    let t0 = at(2026, 1, 1, 9, 0, 0);
    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Acme",
        t0 + chrono::Duration::minutes(5),
        None,
    )
    .unwrap();

    let start = t0;
    let end = at(2026, 1, 1, 10, 0, 0);
    let block_id = seed_block(&conn, &start.to_rfc3339(), &end.to_rfc3339(), 3600);
    seed_titled_event(
        &conn,
        block_id,
        "hook-1",
        "sess-1",
        &t0.to_rfc3339(),
        "Globex ticket work",
    );
    seed_titled_event(
        &conn,
        block_id,
        "hook-2",
        "sess-1",
        &(t0 + chrono::Duration::minutes(10)).to_rfc3339(),
        "PostToolUse",
    );

    let block = Block {
        id: block_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: start.to_rfc3339(),
        ended_at: end.to_rfc3339(),
        duration_seconds: 3600,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert!(slices.iter().all(|s| s.origin != SplitOrigin::Pinned));
}

// Finding P2 (round 2): a session spans several blocks whenever there's
// an idle gap. `pinned_customer_for_block` must judge a block's events
// against the SAME session context (session_start, text_guess) the
// lanes use — derived from the WHOLE day's events — not just this
// block's own subset, or a later block could go Pinned to a customer
// its own session's earlier text guess (and the lanes) disagree with.
#[test]
fn session_spanning_blocks_agrees_with_lanes() {
    let _g = crate::tz::test_env_lock();
    std::env::remove_var("WORKLOG_TZ");

    let conn = open_memory().unwrap();
    let reg = registry(&["Globex", "Acme"], &[multi_tenant_folder("vitinn-infra")]);

    let t0 = at(2026, 1, 1, 9, 0, 0);
    session_pins::pin(
        &conn,
        &reg,
        "sess-1",
        Path::new(&work("vitinn-infra")),
        "Acme",
        t0 + chrono::Duration::hours(3) + chrono::Duration::minutes(1),
        None,
    )
    .unwrap();

    // Block A: the session's real first event, naming Globex in its own
    // text.
    let a_start = t0;
    let a_end = t0 + chrono::Duration::minutes(30);
    let block_a_id = seed_block(&conn, &a_start.to_rfc3339(), &a_end.to_rfc3339(), 1800);
    seed_titled_event(
        &conn,
        block_a_id,
        "hook-a1",
        "sess-1",
        &t0.to_rfc3339(),
        "Globex ticket work",
    );

    // Block B: same session, 3 hours later (the idle gap that opened a
    // new block) — a generic event with no text guess of its own. The
    // pin lands one minute after THIS block's own first event, but three
    // hours after the session's REAL first event (block A's).
    let b_start = t0 + chrono::Duration::hours(3);
    let b_end = b_start + chrono::Duration::minutes(30);
    let block_b_id = seed_block(&conn, &b_start.to_rfc3339(), &b_end.to_rfc3339(), 1800);
    seed_titled_event(
        &conn,
        block_b_id,
        "hook-b1",
        "sess-1",
        &b_start.to_rfc3339(),
        "PostToolUse",
    );

    // An unrelated session in the same folder naming Acme — the second
    // customer this folder needs before the lanes split at all.
    let mut sess2 = Event::minimal("claude", "hook-c1", t0.to_rfc3339(), "Acme ticket work");
    sess2.session_id = Some("sess-2".to_string());
    sess2.project_path = Some(work("vitinn-infra"));
    crate::repo::upsert_event(&conn, &sess2).unwrap();

    let block_b = Block {
        id: block_b_id,
        day: "2026-01-01".to_string(),
        jira_issue: None,
        started_at: b_start.to_rfc3339(),
        ended_at: b_end.to_rfc3339(),
        duration_seconds: 1800,
        description: None,
        estimated_by: None,
        flagged: false,
        tempo_worklog_id: None,
        is_personal: false,
        dirty: false,
        exported_at: None,
    };

    let slices = tenant_slices_for_block(&conn, &block_b, "vitinn-infra", &reg)
        .unwrap()
        .unwrap();
    assert!(
        slices.iter().all(|s| s.origin != SplitOrigin::Pinned),
        "block B must not go Pinned to Acme — the session's real first \
         event (block A) is 3 hours before the pin, well outside \
         SETUP_GRACE, and its own text guess is Globex"
    );

    // Lanes must agree: `tag_sessions`, given the same day's events, tags
    // block B's event Globex too (session sess-1's own text guess), never
    // Acme.
    let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let mut day_events = crate::infer::load_day_events(&conn, day).unwrap();
    let pins = crate::session_pins::pins_for_sessions(&conn, &["sess-1".to_string()]).unwrap();
    crate::session_customers::tag_sessions(&mut day_events, &reg, &pins);
    let block_b_event = day_events
        .iter()
        .find(|e| e.ts == b_start)
        .expect("block B's event must be in the day's events");
    assert_eq!(
        block_b_event.lane_tag.as_deref(),
        Some("Globex"),
        "lanes must tag block B's event Globex, matching its session's text guess"
    );
}
