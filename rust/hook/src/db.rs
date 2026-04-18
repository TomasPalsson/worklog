//! Open the SQLite DB, apply pragmas, ensure schema, expose small helpers.
//!
//! The schema is shared with Python via `include_str!` — one source of truth.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

const SCHEMA: &str = include_str!("../../../src/worklog/schema.sql");

// busy_timeout MUST be set before anything that can block — otherwise a
// concurrent writer hits SQLITE_BUSY immediately instead of waiting.
const PRAGMAS_EARLY: &str = "PRAGMA busy_timeout = 5000;";
const PRAGMAS_SESSION: &str = "
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
";

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    conn.execute_batch(PRAGMAS_EARLY)?;

    // journal_mode is a persistent DB property; only switch to WAL if not
    // already — a concurrent write lock during the switch is what causes
    // SQLITE_BUSY under rapid-fire hook invocations.
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap_or_default();
    if !mode.eq_ignore_ascii_case("wal") {
        let _: String = conn
            .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
            .unwrap_or_default();
    }
    conn.execute_batch(PRAGMAS_SESSION)?;
    // Fast path: if schema is already present, skip the write-lock burst caused
    // by concurrent `CREATE TABLE/INDEX IF NOT EXISTS` on every invocation.
    let applied: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if applied == 0 {
        conn.execute_batch(SCHEMA)?;
    }
    Ok(conn)
}
