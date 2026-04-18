//! Open the SQLite DB, apply pragmas, ensure schema, expose small helpers.
//!
//! The schema is shared with Python via `include_str!` — one source of truth.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

const SCHEMA: &str = include_str!("../../../src/worklog/schema.sql");

const PRAGMAS: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 1000;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
";

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    conn.execute_batch(PRAGMAS)?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}
