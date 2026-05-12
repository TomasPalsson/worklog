//! Claude Code skill install / uninstall / status.
//!
//! Writes the bundled `worklog` skill (`SKILL.md` + reference files) to
//! `~/.claude/skills/worklog/` so Claude Code can load it on every session.
//! Files are baked into the binary via `include_str!` at compile time —
//! a `worklog upgrade` followed by `worklog skill install` is the only
//! way to refresh the on-disk copy. We honour `$CLAUDE_HOME` for tests
//! and power users (same env var as the hook module).
//!
//! Install is idempotent and side-effect-only-on-this-skill: it touches
//! `~/.claude/skills/worklog/**` and nothing else. Other skills in the
//! same directory are not enumerated or modified.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::hook::ENV_CLAUDE_HOME;

/// Skill identifier — also the directory name under `~/.claude/skills/`.
pub const SKILL_NAME: &str = "worklog";

/// Bundled skill payload — `(relative_path_under_skill_dir, contents)`.
///
/// `include_str!` paths are relative to this source file
/// (`rust/crates/worklog-core/src/`). The repo layout puts the canonical
/// skill at `<repo>/skills/worklog/`, four directories up.
const BUNDLED_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../../skills/worklog/SKILL.md"),
    ),
    (
        "references/api-reference.md",
        include_str!("../../../../skills/worklog/references/api-reference.md"),
    ),
    (
        "references/recipes.md",
        include_str!("../../../../skills/worklog/references/recipes.md"),
    ),
    (
        "references/state-machine.md",
        include_str!("../../../../skills/worklog/references/state-machine.md"),
    ),
    (
        "references/troubleshooting.md",
        include_str!("../../../../skills/worklog/references/troubleshooting.md"),
    ),
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillStatus {
    pub skill_dir: PathBuf,
    pub installed: bool,
    /// Whether the on-disk SKILL.md matches the bundled one byte-for-byte.
    /// `false` when the user (or a different worklog version) has edited
    /// the file — install would overwrite. None when not installed.
    pub up_to_date: Option<bool>,
    pub files: Vec<String>,
    pub bundled_version: String,
}

/// Resolve `~/.claude/skills/worklog/` (honouring `$CLAUDE_HOME` if set).
pub fn skill_dir() -> Result<PathBuf> {
    let base = match std::env::var_os(ENV_CLAUDE_HOME) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = dirs::home_dir().context("no home directory available")?;
            home.join(".claude")
        }
    };
    Ok(base.join("skills").join(SKILL_NAME))
}

pub fn install() -> Result<SkillStatus> {
    let dir = skill_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let mut written = Vec::with_capacity(BUNDLED_FILES.len());
    for (rel, body) in BUNDLED_FILES {
        let dest = dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("mkdir {}", parent.display()))?;
        }
        atomic_write(&dest, body)?;
        written.push(rel.to_string());
    }
    Ok(SkillStatus {
        skill_dir: dir,
        installed: true,
        up_to_date: Some(true),
        files: written,
        bundled_version: crate::VERSION.to_string(),
    })
}

pub fn uninstall() -> Result<SkillStatus> {
    let dir = skill_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("removing skill dir {}", dir.display()))?;
    }
    Ok(SkillStatus {
        skill_dir: dir,
        installed: false,
        up_to_date: None,
        files: vec![],
        bundled_version: crate::VERSION.to_string(),
    })
}

pub fn status() -> Result<SkillStatus> {
    let dir = skill_dir()?;
    let skill_md = dir.join("SKILL.md");
    if !skill_md.exists() {
        return Ok(SkillStatus {
            skill_dir: dir,
            installed: false,
            up_to_date: None,
            files: vec![],
            bundled_version: crate::VERSION.to_string(),
        });
    }
    let mut present = Vec::with_capacity(BUNDLED_FILES.len());
    let mut all_match = true;
    for (rel, body) in BUNDLED_FILES {
        let path = dir.join(rel);
        if !path.exists() {
            all_match = false;
            continue;
        }
        present.push(rel.to_string());
        match std::fs::read_to_string(&path) {
            Ok(disk) if disk == *body => {}
            _ => all_match = false,
        }
    }
    Ok(SkillStatus {
        skill_dir: dir,
        installed: true,
        up_to_date: Some(all_match),
        files: present,
        bundled_version: crate::VERSION.to_string(),
    })
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Shared lock with the `hook` module — both flip `$CLAUDE_HOME` in
    // their tests and without a cross-module lock they race on the
    // TestDir-drop `remove_var`, wiping each other's setup mid-test.
    use crate::hook::CLAUDE_HOME_TEST_LOCK as ENV_LOCK;

    struct TestDir {
        _tmp: tempfile::TempDir,
    }

    fn enter(tmp: tempfile::TempDir) -> (std::sync::MutexGuard<'static, ()>, TestDir) {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_CLAUDE_HOME, tmp.path());
        (g, TestDir { _tmp: tmp })
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            std::env::remove_var(ENV_CLAUDE_HOME);
        }
    }

    #[test]
    fn install_writes_skill_md_and_references() {
        let (_g, _t) = enter(tempdir().unwrap());
        let r = install().unwrap();
        assert!(r.installed);
        let dir = skill_dir().unwrap();
        assert!(dir.join("SKILL.md").exists(), "SKILL.md missing");
        assert!(dir.join("references/recipes.md").exists());
        assert!(dir.join("references/api-reference.md").exists());
        assert!(dir.join("references/state-machine.md").exists());
        assert!(dir.join("references/troubleshooting.md").exists());
        assert_eq!(r.files.len(), BUNDLED_FILES.len());
    }

    #[test]
    fn install_is_idempotent() {
        let (_g, _t) = enter(tempdir().unwrap());
        install().unwrap();
        install().unwrap();
        let s = status().unwrap();
        assert!(s.installed);
        assert_eq!(s.up_to_date, Some(true));
    }

    #[test]
    fn status_detects_user_edits_as_out_of_date() {
        let (_g, _t) = enter(tempdir().unwrap());
        install().unwrap();
        let skill_md = skill_dir().unwrap().join("SKILL.md");
        std::fs::write(&skill_md, "# user edit\n").unwrap();
        let s = status().unwrap();
        assert!(s.installed);
        assert_eq!(s.up_to_date, Some(false));
    }

    #[test]
    fn status_before_install_reports_uninstalled() {
        let (_g, _t) = enter(tempdir().unwrap());
        let s = status().unwrap();
        assert!(!s.installed);
        assert!(s.up_to_date.is_none());
    }

    #[test]
    fn uninstall_removes_everything_and_is_safe_to_repeat() {
        let (_g, _t) = enter(tempdir().unwrap());
        install().unwrap();
        let dir = skill_dir().unwrap();
        assert!(dir.exists());
        uninstall().unwrap();
        assert!(!dir.exists());
        // Second call must not error.
        uninstall().unwrap();
    }

    #[test]
    fn bundled_skill_md_has_required_frontmatter_keys() {
        let body = BUNDLED_FILES
            .iter()
            .find(|(p, _)| *p == "SKILL.md")
            .map(|(_, b)| *b)
            .expect("SKILL.md must be in BUNDLED_FILES");
        assert!(
            body.starts_with("---"),
            "SKILL.md must start with YAML frontmatter"
        );
        assert!(body.contains("name: worklog"), "missing name");
        assert!(body.contains("description:"), "missing description");
    }
}
