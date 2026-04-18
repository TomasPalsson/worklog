//! Resolve DB path from env var or default.

use std::path::PathBuf;

pub struct Paths {
    pub db: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        let db = std::env::var_os("WORKLOG_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share/worklog/worklog.db"));

        Self { db }
    }
}
