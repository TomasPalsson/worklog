//! Personal/work classification for blocks.
//!
//! At infer time we ask [`PersonalConfig::classify`] whether a block's
//! dominant `project_path` should be treated as personal. Personal blocks
//! render at reduced opacity in the review UI, skip the `claude -p`
//! estimator round-trip, and are excluded from Tempo sync.
//!
//! Config lives at `~/.config/worklog/personal.toml`:
//!
//! ```toml
//! work     = ["~/Desktop/Work/**"]      # explicit work paths
//! personal = ["~/Desktop/Work/sandbox"] # explicit personal paths (override default)
//! ```
//!
//! Resolution order, first match wins:
//!
//! 1. `project_path` is `None` → **work** (gcal / github / jira blocks
//!    have no cwd but represent billable signals)
//! 2. matches a `work` pattern → **work**
//! 3. matches a `personal` pattern → **personal**
//! 4. default: under `~/Desktop/Work/**` → **work**, else **personal**
//!
//! Patterns support a tiny glob dialect: a literal prefix optionally
//! followed by `/**` or `/*`. `~` expands to the user's home. This is
//! enough for the path shapes a real config will hold and avoids pulling
//! in the `glob` crate.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// On-disk shape of `personal.toml`. Public so `worklog tag` can read,
/// edit, and rewrite the file.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub work: Vec<String>,
    #[serde(default)]
    pub personal: Vec<String>,
}

/// Loaded classification rules. Cheap to clone; load once per infer pass.
#[derive(Debug, Clone)]
pub struct PersonalConfig {
    work_patterns: Vec<String>,
    personal_patterns: Vec<String>,
    /// Built-in default — anything under this prefix counts as work even
    /// without any config file.
    default_work_prefix: Option<String>,
}

impl PersonalConfig {
    /// Load from the standard config location. On missing file, parse
    /// errors, or any IO problem we log a warning and fall back to the
    /// built-in default. Inference never panics on user config.
    pub fn load() -> Self {
        match crate::paths::Paths::resolve() {
            Ok(paths) => Self::load_from(&paths.config_dir.join("personal.toml")),
            Err(_) => Self::default_only(),
        }
    }

    /// Load from a specific path. Test seam.
    pub fn load_from(path: &std::path::Path) -> Self {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Self::default_only();
            }
            Err(e) => {
                warn!(?path, error = %e, "reading personal.toml — using defaults");
                return Self::default_only();
            }
        };
        let file: ConfigFile = match toml::from_str(&raw) {
            Ok(f) => f,
            Err(e) => {
                warn!(?path, error = %e, "parsing personal.toml — using defaults");
                return Self::default_only();
            }
        };
        Self {
            work_patterns: file.work,
            personal_patterns: file.personal,
            default_work_prefix: default_work_prefix(),
        }
    }

    /// No config file present — the built-in rule does the work.
    fn default_only() -> Self {
        Self {
            work_patterns: Vec::new(),
            personal_patterns: Vec::new(),
            default_work_prefix: default_work_prefix(),
        }
    }

    /// Classify a block by its dominant project_path.
    ///
    /// `None` means the block has no cwd attached (gcal, github, jira),
    /// which we treat as billable — calendar meetings and PR reviews
    /// should ride into Tempo with the rest of the work.
    pub fn classify(&self, project_path: Option<&str>) -> bool {
        let Some(path) = project_path else {
            return false;
        };
        let path = expand_home(path);

        for pat in &self.work_patterns {
            if matches(pat, &path) {
                return false;
            }
        }
        for pat in &self.personal_patterns {
            if matches(pat, &path) {
                return true;
            }
        }
        match &self.default_work_prefix {
            Some(prefix) => !path_starts_with(&path, prefix),
            None => false,
        }
    }
}

/// `~/Desktop/Work` (expanded). Returned as a directory prefix the
/// classifier compares against with proper path-segment boundaries.
fn default_work_prefix() -> Option<String> {
    dirs::home_dir().map(|home| {
        let mut p = home;
        p.push("Desktop/Work");
        p.to_string_lossy().into_owned()
    })
}

/// Tiny prefix-glob matcher. Supported shapes:
/// - `literal/path`            — exact match
/// - `literal/path/*`          — match one path segment beneath
/// - `literal/path/**`         — match any depth beneath
/// - leading `~/...`           — home expansion
///
/// Anything more exotic is treated as a literal prefix.
fn matches(pattern: &str, path: &str) -> bool {
    let pat = expand_home(pattern);
    if let Some(prefix) = pat.strip_suffix("/**") {
        return path_starts_with(path, prefix);
    }
    if let Some(prefix) = pat.strip_suffix("/*") {
        // exactly one extra segment, no further slashes
        if !path_starts_with(path, prefix) {
            return false;
        }
        let rest = &path[prefix.len()..];
        let rest = rest.trim_start_matches('/');
        return !rest.is_empty() && !rest.contains('/');
    }
    path == pat || path_starts_with(path, &pat)
}

/// Path-aware `starts_with` — matches at segment boundaries so that
/// `/foo/bar` doesn't match the prefix `/foo/ba`.
fn path_starts_with(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    let prefix = prefix.trim_end_matches('/');
    path.starts_with(prefix) && path.as_bytes().get(prefix.len()).copied() == Some(b'/')
}

/// Standard config location: `<config_dir>/personal.toml`. Returns
/// `None` if `Paths::resolve` fails (no home dir).
pub fn config_path() -> Option<PathBuf> {
    crate::paths::Paths::resolve()
        .ok()
        .map(|p| p.config_dir.join("personal.toml"))
}

/// Read the on-disk config (or default if missing). For the CLI's
/// `worklog tag list` and `worklog tag personal/work` commands.
pub fn read_file(path: &std::path::Path) -> ConfigFile {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => ConfigFile::default(),
    }
}

/// Write the config back to disk, creating the parent directory if
/// missing.
pub fn write_file(path: &std::path::Path, file: &ConfigFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let body = toml::to_string_pretty(file).context("serialising personal.toml")?;
    std::fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Summary of a reclassify pass — what changed and what didn't.
#[derive(Debug, Default, Serialize)]
pub struct ReclassifyStats {
    pub total: u32,
    pub changed_to_personal: u32,
    pub changed_to_work: u32,
    pub unchanged: u32,
}

/// Walk blocks (optionally filtered to one day), recompute each
/// block's dominant project_path from its events, and update the
/// `is_personal` column accordingly. Does NOT re-cluster.
pub fn reclassify_blocks(conn: &Connection, day_filter: Option<&str>) -> Result<ReclassifyStats> {
    let cfg = PersonalConfig::load();
    let ids: Vec<i64> = match day_filter {
        Some(d) => {
            let mut stmt =
                conn.prepare("SELECT id FROM blocks WHERE day = ?1 ORDER BY started_at")?;
            let ids = stmt
                .query_map([d], |r| r.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ids
        }
        None => {
            let mut stmt = conn.prepare("SELECT id FROM blocks ORDER BY day, started_at")?;
            let ids = stmt
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ids
        }
    };

    let mut stats = ReclassifyStats {
        total: ids.len() as u32,
        ..Default::default()
    };
    for id in ids {
        let project = dominant_project_path_for_block(conn, id)?;
        let new_personal = cfg.classify(project.as_deref());
        let current: i64 =
            conn.query_row("SELECT is_personal FROM blocks WHERE id = ?1", [id], |r| {
                r.get(0)
            })?;
        let was_personal = current != 0;
        if was_personal == new_personal {
            stats.unchanged += 1;
            continue;
        }
        conn.execute(
            "UPDATE blocks SET is_personal = ?1 WHERE id = ?2",
            rusqlite::params![if new_personal { 1 } else { 0 }, id],
        )?;
        if new_personal {
            stats.changed_to_personal += 1;
        } else {
            stats.changed_to_work += 1;
        }
    }
    Ok(stats)
}

fn dominant_project_path_for_block(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.project_path
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1 AND e.project_path IS NOT NULL",
    )?;
    let paths: Vec<String> = stmt
        .query_map([block_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for p in paths {
        *counts.entry(p).or_insert(0) += 1;
    }
    Ok(counts.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p))
}

fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            let mut full: PathBuf = home;
            full.push(rest);
            return full.to_string_lossy().into_owned();
        }
    }
    p.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn home() -> String {
        dirs::home_dir().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn classify_default_treats_work_subdir_as_work() {
        let cfg = PersonalConfig::default_only();
        let path = format!("{}/Desktop/Work/sjukra", home());
        assert!(!cfg.classify(Some(&path)));
    }

    #[test]
    fn classify_default_treats_other_paths_as_personal() {
        let cfg = PersonalConfig::default_only();
        let path = format!("{}/Desktop/Projects/worklog", home());
        assert!(cfg.classify(Some(&path)));
    }

    #[test]
    fn classify_no_project_path_is_work() {
        let cfg = PersonalConfig::default_only();
        assert!(!cfg.classify(None));
    }

    #[test]
    fn classify_work_pattern_overrides_personal_default() {
        let cfg = PersonalConfig {
            work_patterns: vec!["~/Desktop/Projects/clientX/**".into()],
            personal_patterns: vec![],
            default_work_prefix: default_work_prefix(),
        };
        let path = format!("{}/Desktop/Projects/clientX/frontend", home());
        assert!(!cfg.classify(Some(&path)));
    }

    #[test]
    fn classify_personal_pattern_overrides_work_default() {
        let cfg = PersonalConfig {
            work_patterns: vec![],
            personal_patterns: vec!["~/Desktop/Work/playground/**".into()],
            default_work_prefix: default_work_prefix(),
        };
        let path = format!("{}/Desktop/Work/playground/sketch", home());
        assert!(cfg.classify(Some(&path)));
    }

    #[test]
    fn classify_missing_config_uses_default() {
        let cfg = PersonalConfig::load_from(std::path::Path::new(
            "/tmp/definitely-does-not-exist-xyz.toml",
        ));
        let path = format!("{}/Desktop/Projects/foo", home());
        assert!(cfg.classify(Some(&path)));
        let work = format!("{}/Desktop/Work/foo", home());
        assert!(!cfg.classify(Some(&work)));
    }

    #[test]
    fn classify_malformed_config_falls_back_to_default() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "this is not toml = [[[").unwrap();
        let cfg = PersonalConfig::load_from(tmp.path());
        // Default rule still applies.
        let path = format!("{}/Desktop/Projects/foo", home());
        assert!(cfg.classify(Some(&path)));
    }

    #[test]
    fn classify_loads_from_toml_round_trip() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(
            tmp,
            "work = [\"~/contract/**\"]\npersonal = [\"~/Desktop/Work/scratch\"]\n"
        )
        .unwrap();
        let cfg = PersonalConfig::load_from(tmp.path());
        let contract = format!("{}/contract/repo", home());
        assert!(!cfg.classify(Some(&contract)), "explicit work wins");
        let scratch = format!("{}/Desktop/Work/scratch", home());
        assert!(
            cfg.classify(Some(&scratch)),
            "explicit personal overrides default-work"
        );
    }

    #[test]
    fn matches_handles_glob_suffixes() {
        assert!(matches("/foo/bar/**", "/foo/bar/baz/qux"));
        assert!(matches("/foo/bar/**", "/foo/bar"));
        assert!(matches("/foo/bar/*", "/foo/bar/baz"));
        assert!(!matches("/foo/bar/*", "/foo/bar/baz/qux"));
        assert!(!matches("/foo/bar/**", "/foo/baz"));
    }

    #[test]
    fn path_starts_with_respects_segment_boundaries() {
        assert!(path_starts_with("/foo/bar", "/foo"));
        assert!(path_starts_with("/foo", "/foo"));
        assert!(!path_starts_with("/foobar", "/foo"));
    }
}
