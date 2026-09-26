use super::*;
use tempfile::tempdir;

#[test]
fn open_memory_has_all_tables() {
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in [
        "block_events",
        "blocks",
        "events",
        "jira_tickets",
        "sessions",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "missing table {expected}; got {tables:?}"
        );
    }
}

#[test]
fn migrate_is_idempotent() {
    let conn = open_memory().unwrap();
    migrate(&conn).unwrap();
    migrate(&conn).unwrap();
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn on_disk_open_enables_wal() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("w.db");
    let conn = open(&db).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn migrate_adds_is_personal_to_legacy_blocks_table() {
    // Simulate a v3 DB: create a blocks table without is_personal,
    // stamp user_version = 3, then run migrate() and assert the
    // column appears.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE blocks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            day TEXT NOT NULL,
            jira_issue TEXT,
            started_at TEXT NOT NULL,
            ended_at TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            description TEXT,
            estimated_by TEXT,
            flagged INTEGER NOT NULL DEFAULT 0,
            tempo_worklog_id TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 3).unwrap();

    migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(blocks)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"is_personal".to_string()),
        "is_personal missing after migrate; got {cols:?}"
    );
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn schema_version_is_bumped_for_exported_at_migration() {
    // B22. `exported_at` landed at v8, so the schema can never be
    // older than that. A `>=` floor rather than an equality keeps the
    // test meaningful without having to be edited by every later
    // migration (the billing registry took it to v9).
    let conn = open_memory().unwrap();
    let v = current_version(&conn).unwrap();
    assert!(
        v >= 8,
        "exported_at shipped in schema v8; got v{v} — did SCHEMA_VERSION regress?"
    );
    assert_eq!(v, SCHEMA_VERSION, "a fresh db is stamped current");
}

#[test]
fn fresh_db_blocks_table_has_exported_at_column() {
    // B22.
    let conn = open_memory().unwrap();
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(blocks)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"exported_at".to_string()),
        "fresh db must have blocks.exported_at; got {cols:?}"
    );
}

#[test]
fn migrate_adds_exported_at_to_legacy_blocks_table_and_backfills_null() {
    // B22. Simulate a pre-exported_at DB: create a blocks table
    // without exported_at (but with every column that shipped
    // before this slice), insert a row, then run migrate() and
    // assert the column appears with NULL for the pre-existing row.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE blocks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            day TEXT NOT NULL,
            jira_issue TEXT,
            started_at TEXT NOT NULL,
            ended_at TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            description TEXT,
            estimated_by TEXT,
            flagged INTEGER NOT NULL DEFAULT 0,
            tempo_worklog_id TEXT,
            is_personal INTEGER NOT NULL DEFAULT 0,
            dirty INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 7).unwrap();

    migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(blocks)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"exported_at".to_string()),
        "exported_at missing after migrate; got {cols:?}"
    );

    let exported_at: Option<String> = conn
        .query_row("SELECT exported_at FROM blocks LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(
        exported_at.is_none(),
        "pre-existing rows must backfill to NULL"
    );
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn meta_table_exists_and_schema_version_is_10() {
    // B22. The pruner's latch lives in a new generic `meta` table
    // (slice 002-billing-cycle-pruner §4), and the billing merge's
    // registry tables took the schema to v9 — this feature bumps it
    // to v10. A `>=` floor (like the exported_at test) keeps this
    // meaningful without an edit on every later migration — routing
    // took it to v11.
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        tables.contains(&"meta".to_string()),
        "missing meta table; got {tables:?}"
    );
    let v = current_version(&conn).unwrap();
    assert!(v >= 10, "meta table shipped at v10; got v{v}");
}

#[test]
fn routing_rules_table_exists_and_schema_version_is_11() {
    // Spec 003 T001: browser/Slack event routing needs a rules table
    // and takes the schema to v11.
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        tables.contains(&"routing_rules".to_string()),
        "missing routing_rules table; got {tables:?}"
    );
    // `>=` floor, not `==`: overlap_allocations took it to v12 — see
    // the `overlap_allocations_table_exists...` test below.
    assert!(current_version(&conn).unwrap() >= 11);
}

#[test]
fn fresh_db_events_table_has_routing_columns() {
    // Spec 003 T001.
    let conn = open_memory().unwrap();
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(events)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in ["container", "label_origin", "label_confidence"] {
        assert!(
            cols.contains(&expected.to_string()),
            "fresh db must have events.{expected}; got {cols:?}"
        );
    }
}

#[test]
fn migrate_adds_routing_columns_to_legacy_events_table_and_backfills_null() {
    // Spec 003 T001. Simulate a pre-v11 DB: an events table without
    // container/label_origin/label_confidence, insert a row, then run
    // migrate() and assert the columns appear with NULL for the
    // pre-existing row.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            duration_seconds INTEGER,
            title TEXT NOT NULL,
            details TEXT,
            repo TEXT,
            project_path TEXT,
            jira_issue TEXT,
            session_id TEXT,
            tempo_worklog_id TEXT,
            raw_json TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE(source, source_id)
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (source, source_id, started_at, title)
         VALUES ('github', 'abc', '2026-04-18T09:00:00+00:00', 'a commit')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 10).unwrap();

    migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(events)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in ["container", "label_origin", "label_confidence"] {
        assert!(
            cols.contains(&expected.to_string()),
            "events.{expected} missing after migrate; got {cols:?}"
        );
    }

    let container: Option<String> = conn
        .query_row("SELECT container FROM events LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(
        container.is_none(),
        "pre-existing rows must backfill to NULL"
    );
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn overlap_allocations_table_exists_and_schema_version_is_12() {
    // Overlap-split UI: the owner's manual share of an overlap window,
    // read back by infer_allocations. Takes the schema to v12.
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        tables.contains(&"overlap_allocations".to_string()),
        "missing overlap_allocations table; got {tables:?}"
    );
    // `>=` floor, not `==`: spec 005's tenant tables took it to v13 —
    // see the `billing_tenant_tables_exist...` test below.
    assert!(current_version(&conn).unwrap() >= 12);
}

#[test]
fn billing_tenant_tables_exist_and_schema_version_is_13() {
    // Spec 005 T001: multi-tenant infra folders need tenant roots,
    // tenant links and hand-set customer shares tables. Takes the
    // schema to v13.
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in [
        "billing_tenant_roots",
        "billing_tenant_links",
        "block_customer_shares",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "missing {expected} table; got {tables:?}"
        );
    }
    // `>=` floor, not `==`: spec 006's deildir + change-log tables took
    // it to v14 — see the `deildir_and_change_log_tables_exist...` test
    // below.
    assert!(current_version(&conn).unwrap() >= 13);
}

#[test]
fn deildir_and_change_log_tables_exist_and_schema_version_is_14() {
    // Spec 006 T001: a per-customer deildir list, the resolution
    // snapshot used to detect automatic changes, and the change log
    // itself. Takes the schema to v14.
    let conn = open_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for expected in [
        "billing_deildir",
        "block_resolution_snapshots",
        "block_changes",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "missing {expected} table; got {tables:?}"
        );
    }
    assert_eq!(current_version(&conn).unwrap(), 14);
}

#[test]
fn fresh_db_block_customer_shares_has_rows_json_column() {
    // Spec 006 T001.
    let conn = open_memory().unwrap();
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(block_customer_shares)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"rows_json".to_string()),
        "fresh db must have block_customer_shares.rows_json; got {cols:?}"
    );
}

#[test]
fn migrate_adds_rows_json_to_legacy_block_customer_shares_table_and_backfills_null() {
    // Spec 006 T001. Simulate a pre-v14 block_customer_shares: no
    // rows_json column, insert a v1 row, then run migrate() and assert
    // the column appears with NULL for the pre-existing row.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE block_customer_shares (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            day TEXT NOT NULL,
            started_at TEXT NOT NULL,
            shares TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            UNIQUE(day, started_at)
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_customer_shares (day, started_at, shares)
         VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '{\"Sjúkra\":1.0}')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();

    migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(block_customer_shares)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"rows_json".to_string()),
        "rows_json missing after migrate; got {cols:?}"
    );

    let rows_json: Option<String> = conn
        .query_row(
            "SELECT rows_json FROM block_customer_shares LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        rows_json.is_none(),
        "pre-existing rows must backfill to NULL"
    );
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn v14_seeds_deildir_from_pins() {
    // FR-14 (B14): a folder pinned to a customer and a Verkefni
    // becomes that customer's deild on upgrade. Simulate a pre-v14 db
    // with a billing_folder_map pin and no billing_deildir table yet.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE billing_folder_map (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            folder TEXT NOT NULL UNIQUE,
            customer TEXT,
            verkefni TEXT,
            billable INTEGER NOT NULL DEFAULT 1,
            multi_tenant INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO billing_folder_map (folder, customer, verkefni)
         VALUES ('sjukra', 'Sjúkra', 'Rekstur')",
        [],
    )
    .unwrap();
    // A shared folder (no verkefni) must not seed a blank-named deild.
    conn.execute(
        "INSERT INTO billing_folder_map (folder, customer, verkefni)
         VALUES ('genai-infra', NULL, NULL)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();

    migrate(&conn).unwrap();

    let deildir: Vec<(String, String)> = conn
        .prepare("SELECT customer, name FROM billing_deildir ORDER BY customer, name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(deildir, vec![("Sjúkra".to_string(), "Rekstur".to_string())]);

    // Idempotent: running migrate again must not duplicate the seed
    // or error on the UNIQUE(customer, name) constraint.
    migrate(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM billing_deildir", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);

    // A seeded deild the Owner deletes stays deleted on the next open.
    conn.execute("DELETE FROM billing_deildir", []).unwrap();
    migrate(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM billing_deildir", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn fresh_db_billing_folder_map_has_multi_tenant_column() {
    // Spec 005 T001.
    let conn = open_memory().unwrap();
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(billing_folder_map)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"multi_tenant".to_string()),
        "fresh db must have billing_folder_map.multi_tenant; got {cols:?}"
    );
}

#[test]
fn migrate_adds_multi_tenant_to_legacy_billing_folder_map_table_and_backfills_zero() {
    // Spec 005 T001. Simulate a pre-v13 billing_folder_map: no
    // multi_tenant column, insert a row, then run migrate() and assert
    // the column appears defaulting to 0 for the pre-existing row.
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE billing_folder_map (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            folder TEXT NOT NULL UNIQUE,
            customer TEXT,
            verkefni TEXT,
            billable INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO billing_folder_map (folder, customer) VALUES ('sjukra', 'Sjúkra')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();

    migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(billing_folder_map)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        cols.contains(&"multi_tenant".to_string()),
        "multi_tenant missing after migrate; got {cols:?}"
    );

    let multi_tenant: i64 = conn
        .query_row(
            "SELECT multi_tenant FROM billing_folder_map LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(multi_tenant, 0, "pre-existing rows must backfill to 0");
    assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
}

#[test]
fn summarize_counts_are_zero_on_fresh_db() {
    let conn = open_memory().unwrap();
    let s = summarize(&conn).unwrap();
    assert_eq!(s.schema_version, SCHEMA_VERSION);
    assert_eq!(s.events, 0);
    assert_eq!(s.blocks, 0);
    assert_eq!(s.sessions, 0);
    assert_eq!(s.jira_tickets, 0);
}
