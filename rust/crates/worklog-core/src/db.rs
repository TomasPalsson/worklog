//! Database connection + migration runner.
//!
//! The schema lives in `sql/schema.sql` and is embedded with `include_str!`.
//! Every `CREATE` in the schema is idempotent, so `migrate()` can be called on
//! every boot. This mirrors the Rust claude-code hook and the Python runtime
//! so the three stay bit-compatible during Stage 1.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Embedded schema — compiled into the binary.
pub const SCHEMA_SQL: &str = include_str!("../sql/schema.sql");

/// Monotonic integer version of the schema, bumped by future migrations.
/// Stored in `PRAGMA user_version` so we can detect stale dbs without adding
/// a dedicated table.
pub const SCHEMA_VERSION: i32 = 3;

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
    conn.execute_batch(SCHEMA_SQL)
        .context("applying schema.sql")?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("stamping user_version")?;
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
mod tests {
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
    fn summarize_counts_are_zero_on_fresh_db() {
        let conn = open_memory().unwrap();
        let s = summarize(&conn).unwrap();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.events, 0);
        assert_eq!(s.blocks, 0);
        assert_eq!(s.sessions, 0);
        assert_eq!(s.jira_tickets, 0);
    }

    #[test]
    fn embedded_schema_matches_python_copy() {
        let python_schema = include_str!("../../../../src/worklog/schema.sql");
        assert_eq!(
            SCHEMA_SQL, python_schema,
            "rust sql/schema.sql has drifted from src/worklog/schema.sql — keep them identical during Stage 1"
        );
    }
}
