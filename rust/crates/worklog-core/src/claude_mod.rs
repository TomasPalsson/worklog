//! The Claude Code mod, embedded in the binary and enabled through
//! `env.CLAUDE_CODE_PLUGIN_DIRS` in `~/.claude/settings.json`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

use crate::hook::{read_settings, settings_path, write_settings};

pub const PLUGIN_DIRS_KEY: &str = "CLAUDE_CODE_PLUGIN_DIRS";

const MOD_DIR_NAME: &str = "claude-mod";

const FILES: &[(&str, &str)] = &[
    (
        ".claude-plugin/plugin.json",
        include_str!("../../../../mods/worklog/.claude-plugin/plugin.json"),
    ),
    (
        "hooks/hooks.json",
        include_str!("../../../../mods/worklog/hooks/hooks.json"),
    ),
    (
        "hooks/register.tsx",
        include_str!("../../../../mods/worklog/hooks/register.tsx"),
    ),
    (
        "hooks/contract.ts",
        include_str!("../../../../mods/worklog/hooks/contract.ts"),
    ),
    (
        "hooks/lib.ts",
        include_str!("../../../../mods/worklog/hooks/lib.ts"),
    ),
    (
        "hooks/status.ts",
        include_str!("../../../../mods/worklog/hooks/status.ts"),
    ),
    (
        "hooks/ticket.ts",
        include_str!("../../../../mods/worklog/hooks/ticket.ts"),
    ),
    (
        "hooks/command.tsx",
        include_str!("../../../../mods/worklog/hooks/command.tsx"),
    ),
    (
        "types/index.d.ts",
        include_str!("../../../../mods/worklog/types/index.d.ts"),
    ),
];

fn mod_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MOD_DIR_NAME)
}

fn split_dirs(value: &str) -> Vec<&str> {
    value.split(':').filter(|d| !d.is_empty()).collect()
}

fn current_dirs(root: &Map<String, Value>) -> String {
    root.get("env")
        .and_then(|env| env.get(PLUGIN_DIRS_KEY))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn set_dirs(root: &mut Map<String, Value>, dirs: &[&str]) -> Result<()> {
    let env = root
        .entry("env".to_owned())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("`env` must be an object")?;
    if dirs.is_empty() {
        env.remove(PLUGIN_DIRS_KEY);
    } else {
        env.insert(PLUGIN_DIRS_KEY.to_owned(), Value::String(dirs.join(":")));
    }
    Ok(())
}

pub fn install(data_dir: &Path) -> Result<PathBuf> {
    let dir = mod_dir(data_dir);
    for (relative, contents) in FILES {
        let target = dir.join(relative);
        std::fs::create_dir_all(target.parent().context("mod file has no parent")?)
            .with_context(|| format!("mkdir for {}", target.display()))?;
        std::fs::write(&target, contents)
            .with_context(|| format!("writing {}", target.display()))?;
    }
    let path = settings_path()?;
    let mut root = read_settings(&path)?;
    let existing = current_dirs(&root);
    let mut dirs = split_dirs(&existing);
    let dir_text = dir.to_string_lossy();
    if !dirs.contains(&dir_text.as_ref()) {
        dirs.push(&dir_text);
        set_dirs(&mut root, &dirs)?;
        write_settings(&path, &Value::Object(root))?;
    }
    Ok(dir)
}

pub fn uninstall(data_dir: &Path) -> Result<()> {
    let dir = mod_dir(data_dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    let path = settings_path()?;
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_settings(&path)?;
    let existing = current_dirs(&root);
    let dir_text = dir.to_string_lossy();
    let dirs: Vec<&str> = split_dirs(&existing)
        .into_iter()
        .filter(|d| *d != dir_text)
        .collect();
    if dirs.join(":") != existing {
        set_dirs(&mut root, &dirs)?;
        write_settings(&path, &Value::Object(root))?;
    }
    Ok(())
}

pub fn is_installed(data_dir: &Path) -> bool {
    let Ok(path) = settings_path() else {
        return false;
    };
    let Ok(root) = read_settings(&path) else {
        return false;
    };
    let dir = mod_dir(data_dir);
    split_dirs(&current_dirs(&root)).contains(&dir.to_string_lossy().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::{CLAUDE_HOME_TEST_LOCK, ENV_CLAUDE_HOME};
    use serde_json::json;
    use tempfile::tempdir;

    fn read_dirs() -> Option<String> {
        let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        root["env"][PLUGIN_DIRS_KEY].as_str().map(str::to_owned)
    }

    fn with_home<R>(settings: Option<Value>, body: impl FnOnce(&Path) -> R) -> R {
        let _guard = CLAUDE_HOME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let data = tempdir().unwrap();
        std::env::set_var(ENV_CLAUDE_HOME, home.path());
        if let Some(value) = settings {
            std::fs::write(settings_path().unwrap(), value.to_string()).unwrap();
        }
        let result = body(data.path());
        std::env::remove_var(ENV_CLAUDE_HOME);
        result
    }

    #[test]
    fn install_writes_files_and_registers_once() {
        with_home(None, |data| {
            let dir = install(data).unwrap();
            assert_eq!(dir, data.join("claude-mod"));
            assert!(dir.join(".claude-plugin/plugin.json").is_file());
            assert!(dir.join("hooks/register.tsx").is_file());
            assert!(!dir.join("hooks/lib.test.ts").exists());
            install(data).unwrap();
            assert_eq!(read_dirs().unwrap(), dir.to_string_lossy());
            assert!(is_installed(data));
        });
    }

    #[test]
    fn install_appends_to_existing_value() {
        with_home(
            Some(json!({"env": {PLUGIN_DIRS_KEY: "/other/plugin", "KEEP": "1"}})),
            |data| {
                let dir = install(data).unwrap();
                assert_eq!(
                    read_dirs().unwrap(),
                    format!("/other/plugin:{}", dir.display())
                );
                let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
                let root: Value = serde_json::from_str(&raw).unwrap();
                assert_eq!(root["env"]["KEEP"], "1");
            },
        );
    }

    #[test]
    fn uninstall_removes_only_own_path() {
        with_home(
            Some(json!({"env": {PLUGIN_DIRS_KEY: "/other/plugin"}})),
            |data| {
                let dir = install(data).unwrap();
                uninstall(data).unwrap();
                assert_eq!(read_dirs().unwrap(), "/other/plugin");
                assert!(!dir.exists());
                assert!(!is_installed(data));
            },
        );
    }

    #[test]
    fn uninstall_drops_key_when_empty() {
        with_home(None, |data| {
            install(data).unwrap();
            uninstall(data).unwrap();
            assert_eq!(read_dirs(), None);
        });
    }

    #[test]
    fn uninstall_without_settings_is_a_noop() {
        with_home(None, |data| {
            uninstall(data).unwrap();
            assert!(!is_installed(data));
        });
    }
}
