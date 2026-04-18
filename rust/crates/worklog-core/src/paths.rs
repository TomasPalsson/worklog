//! Filesystem layout for worklog data.
//!
//! Single source of truth for where the db, socket, config, binary cache, and
//! release cache live. Everything is rooted at `$WORKLOG_HOME` (for tests and
//! power users) or `~/.worklog` by default.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const ENV_HOME: &str = "WORKLOG_HOME";

/// Resolved filesystem paths. Cheap to clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub socket: PathBuf,
    pub config: PathBuf,
    pub bin_dir: PathBuf,
    pub releases: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    /// Resolve from `$WORKLOG_HOME` or `~/.worklog`. Does not touch the filesystem.
    pub fn resolve() -> Result<Self> {
        let root = match std::env::var_os(ENV_HOME) {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => default_root()?,
        };
        Ok(Self::from_root(root))
    }

    /// Build a `Paths` from an explicit root directory. Primarily for tests.
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            db: root.join("worklog.db"),
            socket: root.join("api.sock"),
            config: root.join("config.toml"),
            bin_dir: root.join("bin"),
            releases: root.join("releases"),
            log_dir: root.join("logs"),
            root,
        }
    }

    /// Create every directory this layout needs. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        for dir in [&self.root, &self.bin_dir, &self.releases, &self.log_dir] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// Does the database file exist on disk?
    pub fn db_exists(&self) -> bool {
        self.db.is_file()
    }
}

fn default_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory available")?;
    Ok(home.join(".worklog"))
}

/// Helper used by the CLI: resolve and ensure in one shot.
pub fn ensure_resolved() -> Result<Paths> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    Ok(paths)
}

/// Display a path with `~` substitution when it lives under the home dir.
pub fn short_display(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = p.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    p.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn from_root_assigns_expected_subpaths() {
        let p = Paths::from_root("/tmp/fake");
        assert_eq!(p.db, PathBuf::from("/tmp/fake/worklog.db"));
        assert_eq!(p.socket, PathBuf::from("/tmp/fake/api.sock"));
        assert_eq!(p.config, PathBuf::from("/tmp/fake/config.toml"));
        assert_eq!(p.bin_dir, PathBuf::from("/tmp/fake/bin"));
    }

    #[test]
    fn ensure_creates_all_dirs_and_is_idempotent() {
        let tmp = tempdir().unwrap();
        let p = Paths::from_root(tmp.path());
        p.ensure().unwrap();
        assert!(p.root.is_dir());
        assert!(p.bin_dir.is_dir());
        assert!(p.releases.is_dir());
        assert!(p.log_dir.is_dir());
        // Second call must not error.
        p.ensure().unwrap();
    }

    #[test]
    fn resolve_honors_env_override() {
        let tmp = tempdir().unwrap();
        // SAFETY: tests are single-threaded via `--test-threads=1` is not
        // guaranteed, so we scope the env mutation tightly.
        std::env::set_var(ENV_HOME, tmp.path());
        let p = Paths::resolve().unwrap();
        assert_eq!(p.root, tmp.path());
        std::env::remove_var(ENV_HOME);
    }

    #[test]
    fn db_exists_reports_correctly() {
        let tmp = tempdir().unwrap();
        let p = Paths::from_root(tmp.path());
        p.ensure().unwrap();
        assert!(!p.db_exists());
        std::fs::write(&p.db, b"").unwrap();
        assert!(p.db_exists());
    }

    #[test]
    fn short_display_substitutes_home() {
        if let Some(home) = dirs::home_dir() {
            let inside = home.join(".worklog/worklog.db");
            let rendered = short_display(&inside);
            assert!(rendered.starts_with("~/"), "got {rendered}");
        }
    }
}
