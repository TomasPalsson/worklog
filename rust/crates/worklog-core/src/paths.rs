//! Filesystem layout for worklog data.
//!
//! Mirrors the Python runtime's XDG layout so the Rust binary and the Python
//! CLI read from the same SQLite file without any migration dance:
//!
//! * data — `~/.local/share/worklog/` (db + sockets + bin + releases + logs)
//! * config — `~/.config/worklog/` (env file, future config.toml)
//!
//! Override both with `$WORKLOG_HOME=<dir>` which collapses everything into a
//! single directory — used by tests and power users.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const ENV_HOME: &str = "WORKLOG_HOME";

/// Resolved filesystem paths. Cheap to clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// "Home" root used for reporting. Equals `data_dir` when XDG is in effect.
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub db: PathBuf,
    pub socket: PathBuf,
    pub config: PathBuf,
    pub env_file: PathBuf,
    pub bin_dir: PathBuf,
    pub releases: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    /// Resolve from `$WORKLOG_HOME` (if set) otherwise the XDG-style layout
    /// used by the Python runtime.
    pub fn resolve() -> Result<Self> {
        if let Some(v) = std::env::var_os(ENV_HOME) {
            if !v.is_empty() {
                return Ok(Self::from_root(PathBuf::from(v)));
            }
        }
        Self::xdg_default()
    }

    /// Collapse every path onto a single root. Used for `$WORKLOG_HOME` and
    /// tests. `data_dir`, `config_dir`, and `root` are all equal.
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            db: root.join("worklog.db"),
            socket: root.join("api.sock"),
            config: root.join("config.toml"),
            env_file: root.join(".env"),
            bin_dir: root.join("bin"),
            releases: root.join("releases"),
            log_dir: root.join("logs"),
            data_dir: root.clone(),
            config_dir: root.clone(),
            root,
        }
    }

    /// The XDG layout used by the Python runtime.
    fn xdg_default() -> Result<Self> {
        let home = dirs::home_dir().context("no home directory available")?;
        let data_dir = home.join(".local/share/worklog");
        let config_dir = home.join(".config/worklog");
        Ok(Self {
            db: data_dir.join("worklog.db"),
            socket: data_dir.join("api.sock"),
            config: config_dir.join("config.toml"),
            env_file: config_dir.join(".env"),
            bin_dir: data_dir.join("bin"),
            releases: data_dir.join("releases"),
            log_dir: data_dir.join("logs"),
            root: data_dir.clone(),
            data_dir,
            config_dir,
        })
    }

    /// Create every directory this layout needs. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.config_dir,
            &self.bin_dir,
            &self.releases,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// Does the database file exist on disk?
    pub fn db_exists(&self) -> bool {
        self.db.is_file()
    }
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
    fn xdg_default_splits_data_and_config() {
        std::env::remove_var(ENV_HOME);
        let p = Paths::resolve().unwrap();
        // The two should live in distinct trees.
        assert_ne!(p.data_dir, p.config_dir);
        assert!(p.db.ends_with(".local/share/worklog/worklog.db"));
        assert!(p.env_file.ends_with(".config/worklog/.env"));
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
