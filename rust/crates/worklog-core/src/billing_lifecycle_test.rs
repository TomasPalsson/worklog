// Lifecycle riders (SessionStart/Stop/SessionEnd) must never outvote a
// block's real events in the billing folder vote. Split file for
// billing.rs's line budget — see billing_tenant_test.rs for the same
// convention.

use super::*;
use crate::db::open_memory;
use crate::models::Event;
use crate::repo as repository;
use rusqlite::params;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

fn work(sub: &str) -> String {
    format!("{}/Desktop/Work/{sub}", home())
}

fn seed_block(conn: &Connection, started_at: &str, duration_seconds: i64) -> i64 {
    conn.execute(
        "INSERT INTO blocks
            (day, started_at, ended_at, duration_seconds, description, is_personal)
         VALUES ('2026-09-25', ?1, ?1, ?2, 'apro-skills work', 0)",
        params![started_at, duration_seconds],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn seed_event(conn: &Connection, block_id: i64, source_id: &str, project_path: &str, title: &str) {
    let mut ev = Event::minimal("claude", source_id, "2026-09-25T10:36:00Z", title);
    ev.project_path = Some(project_path.to_string());
    let eid = repository::upsert_event(conn, &ev).unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![block_id, eid],
    )
    .unwrap();
}

#[test]
fn work_folder_for_block_skips_lifecycle_riders() {
    // Regression: a real apro-skills block outvoted 5-to-2 by
    // SessionStart/SessionEnd lifecycle riders from other sessions that
    // happened to cwd into a different folder must still bill under the
    // folder its real work happened in.
    let c = open_memory().unwrap();
    let b = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    seed_event(&c, b, "real1", &work("apro-skills"), "commit");
    seed_event(&c, b, "real2", &work("apro-skills"), "commit");
    for i in 0..5 {
        let title = if i % 2 == 0 {
            "SessionStart"
        } else {
            "SessionEnd"
        };
        seed_event(&c, b, &format!("rider{i}"), &work("worklog"), title);
    }
    assert_eq!(
        work_folder_for_block(&c, b).unwrap(),
        Some("apro-skills".into()),
        "lifecycle riders must never outvote a block's real events"
    );
}

fn compress(conn: &Connection, block_id: i64) {
    let card = crate::block_digest::build_digest(conn, block_id).unwrap();
    assert!(crate::block_digest::write_digest(conn, block_id, &card).unwrap());
    conn.execute("DELETE FROM block_events", []).unwrap();
    conn.execute("DELETE FROM events", []).unwrap();
}

#[test]
fn billing_and_personal_readers_survive_compression() {
    let c = open_memory().unwrap();
    let b = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    seed_event(&c, b, "hook1", &work("apro-skills"), "PreToolUse");
    seed_event(&c, b, "hook2", &work("apro-skills"), "PreToolUse");
    let mut commit = Event::minimal("github", "c1", "2026-09-25T10:40:00Z", "Fix the export");
    commit.project_path = Some(work("apro-skills/api"));
    let eid = repository::upsert_event(&c, &commit).unwrap();
    c.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        params![b, eid],
    )
    .unwrap();

    compress(&c, b);

    assert_eq!(
        work_folder_for_block(&c, b).unwrap(),
        Some("apro-skills".into())
    );
    assert_eq!(
        dominant_title_for_blocks(&c, &[b]).unwrap(),
        Some("Fix the export".into())
    );
    assert_eq!(
        distinct_paths_for_blocks(&c, &[b]).unwrap(),
        vec![work("apro-skills"), work("apro-skills/api")]
    );
    assert_eq!(
        crate::personal::dominant_project_path_for_block(&c, b).unwrap(),
        Some(work("apro-skills"))
    );
}

#[test]
fn pinned_customer_and_branch_clues_survive_compression() {
    use crate::billing_registry::{Customer, FolderMap};
    use crate::tenant_contract::{ClueStrength, SplitOrigin};

    let c = open_memory().unwrap();
    let b = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    let card = crate::digest_contract::BlockDigest {
        folder: Some("infra".into()),
        pinned_customer: Some("Acme".into()),
        branches: vec!["feature/globex-login".into()],
        first_at: Some("2026-09-25T10:36:00+00:00".into()),
        ..Default::default()
    };
    crate::block_digest::write_digest(&c, b, &card).unwrap();
    let registry = Registry {
        customers: ["Acme", "Globex"]
            .iter()
            .map(|name| Customer {
                id: None,
                name: (*name).to_string(),
                aliases: Vec::new(),
            })
            .collect(),
        folders: vec![FolderMap {
            id: None,
            folder: "infra".into(),
            customer: None,
            verkefni: None,
            billable: true,
            multi_tenant: true,
        }],
    };
    let block = repository::get_block(&c, b).unwrap().unwrap();

    let slices = crate::tenant_split::tenant_slices_for_block(&c, &block, "infra", &registry)
        .unwrap()
        .unwrap();
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].customer.as_deref(), Some("Acme"));
    assert_eq!(slices[0].origin, SplitOrigin::Pinned);

    let clues =
        crate::tenant_clues::clues_for_block(&c, b, "infra", &Default::default(), &registry)
            .unwrap();
    assert_eq!(clues.len(), 1);
    assert_eq!(clues[0].customer, "Globex");
    assert_eq!(clues[0].strength, ClueStrength::Branch);
}

#[test]
fn card_worktree_clues_only_count_for_their_own_folder() {
    use crate::billing_registry::Customer;

    let c = open_memory().unwrap();
    let b = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    let card = crate::digest_contract::BlockDigest {
        folder: Some("infra".into()),
        paths: vec![
            work("infra/.claude/worktrees/acme-fix"),
            work("other/.claude/worktrees/globex-fix"),
        ],
        first_at: Some("2026-09-25T10:36:00+00:00".into()),
        ..Default::default()
    };
    crate::block_digest::write_digest(&c, b, &card).unwrap();
    let registry = Registry {
        customers: ["Acme", "Globex"]
            .iter()
            .map(|name| Customer {
                id: None,
                name: (*name).to_string(),
                aliases: Vec::new(),
            })
            .collect(),
        folders: Vec::new(),
    };
    let clues =
        crate::tenant_clues::clues_for_block(&c, b, "infra", &Default::default(), &registry)
            .unwrap();
    assert_eq!(clues.len(), 1);
    assert_eq!(clues[0].customer, "Acme");
    let other =
        crate::tenant_clues::clues_for_block(&c, b, "other", &Default::default(), &registry)
            .unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].customer, "Globex");
}

fn seed_card(conn: &Connection, card: crate::digest_contract::BlockDigest) -> i64 {
    let b = seed_block(conn, "2026-09-25T10:36:00+00:00", 960);
    crate::block_digest::write_digest(conn, b, &card).unwrap();
    b
}

#[test]
fn title_and_path_counts_sum_across_cards_survive_compression() {
    use crate::digest_contract::BlockDigest;
    let c = open_memory().unwrap();
    let a = seed_card(
        &c,
        BlockDigest {
            invoice_titles: vec!["Alpha".into(), "Beta".into()],
            invoice_title_counts: vec![1, 3],
            paths: vec!["/p/a".into(), "/p/b".into()],
            path_counts: vec![5, 1],
            ..Default::default()
        },
    );
    let b = seed_card(
        &c,
        BlockDigest {
            invoice_titles: vec!["Alpha".into()],
            invoice_title_counts: vec![1],
            paths: vec!["/p/b".into()],
            path_counts: vec![6],
            ..Default::default()
        },
    );
    // Beta 3 beats Alpha 1+1; /p/b 1+6 beats /p/a 5.
    assert_eq!(
        dominant_title_for_blocks(&c, &[a, b]).unwrap(),
        Some("Beta".into())
    );
    assert_eq!(
        distinct_paths_for_blocks(&c, &[a, b]).unwrap(),
        vec!["/p/b".to_string(), "/p/a".to_string()]
    );
}

#[test]
fn v1_card_titles_weigh_one_each_survive_compression() {
    let c = open_memory().unwrap();
    let a = seed_card(
        &c,
        crate::digest_contract::BlockDigest {
            invoice_titles: vec!["Zed".into(), "Alpha".into()],
            ..Default::default()
        },
    );
    assert_eq!(
        dominant_title_for_blocks(&c, &[a]).unwrap(),
        Some("Alpha".into())
    );
}

#[test]
fn live_and_card_counts_merge_survive_compression() {
    let c = open_memory().unwrap();
    let card = seed_card(
        &c,
        crate::digest_contract::BlockDigest {
            paths: vec!["/p/a".into(), "/p/b".into()],
            path_counts: vec![2, 2],
            ..Default::default()
        },
    );
    let live = seed_block(&c, "2026-09-25T11:36:00+00:00", 960);
    seed_event(&c, live, "l1", "/p/b", "x");
    seed_event(&c, live, "l2", "/p/c", "x");
    seed_event(&c, live, "l3", "/p/c", "x");
    // b 2+1, a 2, c 2 (a/c tie breaks by path)
    assert_eq!(
        distinct_paths_for_blocks(&c, &[card, live]).unwrap(),
        vec!["/p/b".to_string(), "/p/a".to_string(), "/p/c".to_string()]
    );
}

#[test]
fn multi_block_titles_survive_compression() {
    let c = open_memory().unwrap();
    let b1 = seed_block(&c, "2026-09-25T10:36:00+00:00", 960);
    let b2 = seed_block(&c, "2026-09-25T11:36:00+00:00", 960);
    let mut n = 0;
    let mut add = |block: i64, title: &str, times: usize, path: &str| {
        for _ in 0..times {
            n += 1;
            let mut ev = Event::minimal(
                "github_commit",
                &format!("gc{n}"),
                "2026-09-25T10:40:00Z",
                title,
            );
            ev.project_path = Some(path.to_string());
            let eid = repository::upsert_event(&c, &ev).unwrap();
            c.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![block, eid],
            )
            .unwrap();
        }
    };
    // Per-card weight 1 would tie A 2 : B 2 and pick A; real counts give B 6.
    add(b1, "A", 1, &work("apro-skills"));
    add(b1, "B", 1, &work("apro-skills"));
    add(b2, "A", 1, &work("apro-skills/api"));
    add(b2, "B", 5, &work("apro-skills/api"));

    let title = dominant_title_for_blocks(&c, &[b1, b2]).unwrap();
    let paths = distinct_paths_for_blocks(&c, &[b1, b2]).unwrap();
    assert_eq!(title, Some("B".into()));

    for b in [b1, b2] {
        let card = crate::block_digest::build_digest(&c, b).unwrap();
        assert!(crate::block_digest::write_digest(&c, b, &card).unwrap());
    }
    c.execute("DELETE FROM block_events", []).unwrap();
    c.execute("DELETE FROM events", []).unwrap();

    assert_eq!(dominant_title_for_blocks(&c, &[b1, b2]).unwrap(), title);
    assert_eq!(distinct_paths_for_blocks(&c, &[b1, b2]).unwrap(), paths);
}
