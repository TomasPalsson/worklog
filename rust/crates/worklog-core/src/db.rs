//! Database connection + migration runner.
//!
//! The schema lives in `sql/schema.sql` and is embedded with `include_str!`.
//! Every `CREATE` in the schema is idempotent, so `migrate()` can be called
//! on every boot.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Embedded schema — compiled into the binary.
pub const SCHEMA_SQL: &str = include_str!("../sql/schema.sql");

/// Monotonic integer version of the schema, bumped by future migrations.
/// Stored in `PRAGMA user_version` so we can detect stale dbs without adding
/// a dedicated table.
pub const SCHEMA_VERSION: i32 = 14;

/// Open a connection at `path`, enable WAL + FK, and run migrations.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating db parent {}", parent.display()))?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("opening sqlite at {}", path.display()))?;
    configure(&conn)?;
    migrate(&conn)?;
    // Give the billing registry a starting point on first use so the user
    // corrects entries instead of typing them all. Guarded internally, so
    // it only fires when both registry tables are empty and never
    // overwrites an edit.
    //
    // Deliberately here and NOT in `migrate`: `open_memory` shares
    // `migrate`, and a test database arriving with ten fabricated customer
    // names is a trap — a billing test asserting "no customer resolved"
    // could match a seeded alias by accident. Real databases seed; test
    // databases stay empty.
    crate::billing_registry::seed_if_empty(&conn).context("seeding billing registry")?;
    crate::billing_registry::seed_tenant_roots_if_empty(&conn).context("seeding tenant roots")?;
    Ok(conn)
}

/// Open an in-memory connection for tests.
pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("opening in-memory sqlite")?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    // journal_mode uses a pragma that returns a row; use query_row.
    let _: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .context("enabling WAL")?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA synchronous  = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA temp_store   = MEMORY;",
    )
    .context("applying pragmas")?;
    Ok(())
}

/// Apply `SCHEMA_SQL` and stamp `user_version`. Idempotent.
pub fn migrate(conn: &Connection) -> Result<()> {
    let from_version = current_version(conn)?;
    conn.execute_batch(SCHEMA_SQL)
        .context("applying schema.sql")?;
    // SCHEMA_SQL's `CREATE TABLE IF NOT EXISTS blocks` is a no-op when the
    // table already exists, so a DB from v3 won't auto-pick up the new
    // is_personal column. Do an idempotent ALTER for upgraded users.
    ensure_blocks_is_personal(conn).context("ensuring blocks.is_personal")?;
    ensure_blocks_dirty(conn).context("ensuring blocks.dirty")?;
    ensure_blocks_exported_at(conn).context("ensuring blocks.exported_at")?;
    ensure_jira_tickets_issue_id(conn).context("ensuring jira_tickets.issue_id")?;
    ensure_jira_tickets_external(conn).context("ensuring jira_tickets.external")?;
    ensure_events_routing_columns(conn).context("ensuring events routing columns")?;
    ensure_billing_folder_map_multi_tenant(conn)
        .context("ensuring billing_folder_map.multi_tenant")?;
    ensure_block_customer_shares_rows_json(conn)
        .context("ensuring block_customer_shares.rows_json")?;
    if from_version < 14 {
        seed_deildir_from_folder_pins(conn).context("seeding billing_deildir from folder pins")?;
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("stamping user_version")?;
    Ok(())
}

fn ensure_blocks_is_personal(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(blocks)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "is_personal");
    if !has {
        conn.execute(
            "ALTER TABLE blocks ADD COLUMN is_personal INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("ALTER TABLE blocks ADD is_personal")?;
    }
    Ok(())
}

fn ensure_blocks_dirty(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(blocks)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "dirty");
    if !has {
        conn.execute(
            "ALTER TABLE blocks ADD COLUMN dirty INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("ALTER TABLE blocks ADD dirty")?;
    }
    Ok(())
}

fn ensure_blocks_exported_at(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(blocks)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "exported_at");
    if !has {
        conn.execute("ALTER TABLE blocks ADD COLUMN exported_at TEXT", [])
            .context("ALTER TABLE blocks ADD exported_at")?;
    }
    Ok(())
}

fn ensure_jira_tickets_issue_id(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(jira_tickets)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "issue_id");
    if !has {
        conn.execute("ALTER TABLE jira_tickets ADD COLUMN issue_id TEXT", [])
            .context("ALTER TABLE jira_tickets ADD issue_id")?;
    }
    Ok(())
}

fn ensure_jira_tickets_external(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(jira_tickets)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "external");
    if !has {
        conn.execute(
            "ALTER TABLE jira_tickets ADD COLUMN external INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("ALTER TABLE jira_tickets ADD external")?;
    }
    Ok(())
}

fn ensure_events_routing_columns(conn: &Connection) -> Result<()> {
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(events)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !cols.iter().any(|c| c == "container") {
        conn.execute("ALTER TABLE events ADD COLUMN container TEXT", [])
            .context("ALTER TABLE events ADD container")?;
    }
    if !cols.iter().any(|c| c == "label_origin") {
        conn.execute("ALTER TABLE events ADD COLUMN label_origin TEXT", [])
            .context("ALTER TABLE events ADD label_origin")?;
    }
    if !cols.iter().any(|c| c == "label_confidence") {
        conn.execute("ALTER TABLE events ADD COLUMN label_confidence REAL", [])
            .context("ALTER TABLE events ADD label_confidence")?;
    }
    Ok(())
}

fn ensure_billing_folder_map_multi_tenant(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(billing_folder_map)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "multi_tenant");
    if !has {
        conn.execute(
            "ALTER TABLE billing_folder_map ADD COLUMN multi_tenant INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("ALTER TABLE billing_folder_map ADD multi_tenant")?;
    }
    Ok(())
}

fn ensure_block_customer_shares_rows_json(conn: &Connection) -> Result<()> {
    let has: bool = conn
        .prepare("PRAGMA table_info(block_customer_shares)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|c| c == "rows_json");
    if !has {
        conn.execute(
            "ALTER TABLE block_customer_shares ADD COLUMN rows_json TEXT",
            [],
        )
        .context("ALTER TABLE block_customer_shares ADD rows_json")?;
    }
    Ok(())
}

/// FR-14: a folder already pinned to a customer and a Verkefni becomes
/// that customer's deild on upgrade, so the Owner starts with something
/// instead of an empty list. Runs only on the upgrade to v14, so a seeded
/// deild the Owner later removes is not re-added on the next open.
fn seed_deildir_from_folder_pins(conn: &Connection) -> Result<()> {
    let pins: Vec<(String, String)> = conn
        .prepare(
            "SELECT customer, verkefni FROM billing_folder_map
             WHERE customer IS NOT NULL AND verkefni IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (customer, verkefni) in pins {
        conn.execute(
            "INSERT INTO billing_deildir (customer, name) VALUES (?1, ?2)
             ON CONFLICT(customer, name) DO NOTHING",
            params![customer, verkefni],
        )
        .context("seed billing_deildir from folder pin")?;
    }
    Ok(())
}

/// Read the current `user_version`. Useful for `worklog doctor`.
pub fn current_version(conn: &Connection) -> Result<i32> {
    let v: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .context("reading user_version")?;
    Ok(v)
}

/// A one-line health summary of a db. Used by `worklog doctor`.
#[derive(Debug, serde::Serialize)]
pub struct DbSummary {
    pub schema_version: i32,
    pub events: i64,
    pub blocks: i64,
    pub sessions: i64,
    pub jira_tickets: i64,
}

pub fn summarize(conn: &Connection) -> Result<DbSummary> {
    let schema_version = current_version(conn)?;
    let events = count(conn, "events")?;
    let blocks = count(conn, "blocks")?;
    let sessions = count(conn, "sessions")?;
    let jira_tickets = count(conn, "jira_tickets")?;
    Ok(DbSummary {
        schema_version,
        events,
        blocks,
        sessions,
        jira_tickets,
    })
}

fn count(conn: &Connection, table: &str) -> Result<i64> {
    // `table` is a compile-time constant passed by callers in this module.
    // rusqlite does not allow binding identifiers, so we format carefully.
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let n: i64 = conn
        .query_row(&sql, [], |r| r.get(0))
        .with_context(|| format!("counting {table}"))?;
    Ok(n)
}

#[cfg(test)]
#[path = "db_test.rs"]
mod tests;
