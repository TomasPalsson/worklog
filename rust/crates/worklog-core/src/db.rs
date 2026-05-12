//! Database connection + migration runner.
//!
//! The schema lives in `sql/schema.sql` and is embedded with `include_str!`.
//! Every `CREATE` in the schema is idempotent, so `migrate()` can be called
//! on every boot.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Embedded schema — compiled into the binary.
pub const SCHEMA_SQL: &str = include_str!("../sql/schema.sql");

/// Monotonic integer version of the schema, bumped by future migrations.
/// Stored in `PRAGMA user_version` so we can detect stale dbs without adding
/// a dedicated table.
pub const SCHEMA_VERSION: i32 = 5;

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
    // SCHEMA_SQL's `CREATE TABLE IF NOT EXISTS blocks` is a no-op when the
    // table already exists, so a DB from v3 won't auto-pick up the new
    // is_personal column. Do an idempotent ALTER for upgraded users.
    ensure_blocks_is_personal(conn).context("ensuring blocks.is_personal")?;
    ensure_jira_tickets_issue_id(conn).context("ensuring jira_tickets.issue_id")?;
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
    fn summarize_counts_are_zero_on_fresh_db() {
        let conn = open_memory().unwrap();
        let s = summarize(&conn).unwrap();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.events, 0);
        assert_eq!(s.blocks, 0);
        assert_eq!(s.sessions, 0);
        assert_eq!(s.jira_tickets, 0);
    }
}
