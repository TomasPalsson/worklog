//! Claude Code hook install / uninstall / status.
//!
//! Edits `~/.claude/settings.json` idempotently. We keep the exact handler
//! shape the Python installer used — `{"hooks": [{"type": "command",
//! "command": "<cmd>"}]}` — so installing from either CLI produces the
//! same file and a repeat install is a no-op.
//!
//! A single worklog handler is identified by substring `"worklog"` in the
//! command. That's loose on purpose: `worklog-hook`, `worklog hook-run`,
//! the legacy `worklog hook run`, and `worklog-rs` variants are all
//! recognised so re-running install doesn't leave duplicates behind.
//! Tests override `$CLAUDE_HOME` (and the hook command) so nothing here
//! ever touches your real settings file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

/// Events worklog listens for.
///
/// `PreToolUse` and `PostToolUse` are the heartbeats that keep a Claude
/// session visible to the block inference. Without them, a long
/// autonomous turn (Claude running tools for 20+ min between user
/// prompts) produces no events and the gap-timeout in `infer.rs` shreds
/// the session into dropped sub-MIN_BLOCK slivers.
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SubagentStop",
    "SessionEnd",
];

/// Override the Claude settings dir for tests and power users.
pub const ENV_CLAUDE_HOME: &str = "CLAUDE_HOME";

/// Cross-module test lock for `$CLAUDE_HOME`. Both `hook` and `skill` set
/// this env var during tests; without a shared lock their TestDir guards
/// race and one module's `remove_var` on drop can wipe another module's
/// setup mid-test. Public-but-test-only is the least invasive fix.
#[cfg(test)]
pub static CLAUDE_HOME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A captured view of the installation state, used by `status`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HookStatus {
    pub settings_path: PathBuf,
    pub installed: bool,
    pub events: Vec<String>,
    pub command: Option<String>,
}

/// Resolve `~/.claude/settings.json` (honouring `$CLAUDE_HOME` if set).
pub fn settings_path() -> Result<PathBuf> {
    let base = match std::env::var_os(ENV_CLAUDE_HOME) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = dirs::home_dir().context("no home directory available")?;
            home.join(".claude")
        }
    };
    Ok(base.join("settings.json"))
}

/// Default command string registered in the hook handler.
///
/// Preference order (most preferred first):
/// 1. `worklog-hook` — the old standalone Rust hook binary. Users on
///    Stage 0-era installs may still have it; if so, use it directly.
/// 2. `worklog-rs` — the Stage 1-4 binary name, kept for users who
///    haven't re-run the curl installer yet.
/// 3. `worklog` on PATH — the current pure-Rust binary.
///
/// All three flavours accept `hook-run` (hyphenated top-level command);
/// `worklog hook run` (two words) is also accepted via the back-compat
/// alias so an existing settings.json entry doesn't break after a
/// binary swap.
pub fn default_command() -> String {
    if let Some(p) = which::which_ok("worklog-hook") {
        return p.to_string_lossy().into_owned();
    }
    if let Some(p) = which::which_ok("worklog-rs") {
        return format!("{} hook-run", p.to_string_lossy());
    }
    if let Some(p) = which::which_ok("worklog") {
        return format!("{} hook-run", p.to_string_lossy());
    }
    "worklog hook-run".to_owned()
}

pub fn install(command: &str) -> Result<HookStatus> {
    let path = settings_path()?;
    let mut root = read_settings(&path)?;
    let hooks = root
        .entry("hooks".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks.as_object_mut().context("`hooks` must be an object")?;

    // Sweep worklog handlers out of EVERY hook key before re-adding them
    // to the current EVENTS set. Older worklog versions registered extra
    // events (notably PreToolUse / PostToolUse, which capture tool I/O —
    // i.e. source code — into the events table). The per-event loop below
    // only ever touches keys in EVENTS, so without this full sweep those
    // stale handlers would linger forever and keep logging tool content.
    let existing_keys: Vec<String> = hooks_obj.keys().cloned().collect();
    for k in existing_keys {
        if let Some(arr) = hooks_obj.get_mut(&k).and_then(Value::as_array_mut) {
            arr.retain(|h| !is_worklog_handler(h));
            if arr.is_empty() {
                hooks_obj.remove(&k);
            }
        }
    }

    for ev in EVENTS {
        let handlers = hooks_obj
            .entry((*ev).to_owned())
            .or_insert_with(|| Value::Array(vec![]));
        let arr = handlers
            .as_array_mut()
            .context("event handler list must be an array")?;
        // Drop any previous worklog handlers so we never leave duplicates.
        arr.retain(|h| !is_worklog_handler(h));
        arr.push(json!({
            "hooks": [
                { "type": "command", "command": command }
            ]
        }));
    }

    write_settings(&path, &Value::Object(root))?;
    Ok(HookStatus {
        settings_path: path,
        installed: true,
        events: EVENTS.iter().map(|s| (*s).to_owned()).collect(),
        command: Some(command.to_owned()),
    })
}

pub fn uninstall() -> Result<HookStatus> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(HookStatus {
            settings_path: path,
            installed: false,
            events: vec![],
            command: None,
        });
    }
    let mut root = read_settings(&path)?;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for k in keys {
            if let Some(arr) = hooks.get_mut(&k).and_then(Value::as_array_mut) {
                arr.retain(|h| !is_worklog_handler(h));
                if arr.is_empty() {
                    hooks.remove(&k);
                }
            }
        }
        // If the entire hooks map is now empty, drop it so we don't leave
        // stray keys behind.
        if hooks.is_empty() {
            root.remove("hooks");
        }
    }
    write_settings(&path, &Value::Object(root))?;
    Ok(HookStatus {
        settings_path: path,
        installed: false,
        events: vec![],
        command: None,
    })
}

pub fn status() -> Result<HookStatus> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(HookStatus {
            settings_path: path,
            installed: false,
            events: vec![],
            command: None,
        });
    }
    let root = read_settings(&path)?;
    let mut events = Vec::new();
    let mut command: Option<String> = None;
    if let Some(hooks) = root.get("hooks").and_then(Value::as_object) {
        for ev in EVENTS {
            let Some(arr) = hooks.get(*ev).and_then(Value::as_array) else {
                continue;
            };
            if let Some(cmd) = find_worklog_command(arr) {
                events.push((*ev).to_owned());
                command.get_or_insert(cmd);
            }
        }
    }
    Ok(HookStatus {
        settings_path: path,
        installed: !events.is_empty(),
        events,
        command,
    })
}

// ───────────────────────── private helpers ─────────────────────────

fn read_settings(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Map::new());
    }
    let parsed: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    match parsed {
        Value::Object(m) => Ok(m),
        _ => anyhow::bail!("{} must contain a JSON object at the root", path.display()),
    }
}

fn write_settings(path: &Path, v: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    // Atomic write: *.tmp → rename, with a .bak if the file was there.
    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = std::fs::copy(path, bak);
    }
    let tmp = path.with_extension("json.tmp");
    let pretty = serde_json::to_string_pretty(v)? + "\n";
    std::fs::write(&tmp, pretty).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

fn is_worklog_handler(handler: &Value) -> bool {
    let Some(list) = handler.get("hooks").and_then(Value::as_array) else {
        return false;
    };
    list.iter().any(|h| {
        h.get("command")
            .and_then(Value::as_str)
            .map(|c| c.contains("worklog"))
            .unwrap_or(false)
    })
}

fn find_worklog_command(arr: &[Value]) -> Option<String> {
    for handler in arr {
        let Some(list) = handler.get("hooks").and_then(Value::as_array) else {
            continue;
        };
        for h in list {
            if let Some(cmd) = h.get("command").and_then(Value::as_str) {
                if cmd.contains("worklog") {
                    return Some(cmd.to_owned());
                }
            }
        }
    }
    None
}

/// A tiny `which` shim so the core crate doesn't grow a dep for one call.
mod which {
    use std::path::PathBuf;
    pub fn which_ok(bin: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Every hook test mutates `$CLAUDE_HOME` which is process-global, so
    // we serialise them with a shared cross-module lock — the `skill`
    // module flips the same env var and its TestDir guard would otherwise
    // race against ours and wipe each other's setup on drop.
    use super::CLAUDE_HOME_TEST_LOCK as ENV_LOCK;

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
    fn install_creates_handlers_for_every_event() {
        let (_g, _t) = enter(tempdir().unwrap());
        let cmd = "/usr/bin/worklog hook run";
        install(cmd).unwrap();

        let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let hooks = root.get("hooks").unwrap().as_object().unwrap();
        for ev in EVENTS {
            let arr = hooks.get(*ev).unwrap().as_array().unwrap();
            assert_eq!(arr.len(), 1, "event {ev} must have 1 handler, got {arr:?}");
            let handler = arr[0].get("hooks").unwrap().as_array().unwrap();
            assert_eq!(handler[0]["command"].as_str().unwrap(), cmd);
        }
    }

    #[test]
    fn install_is_idempotent() {
        let (_g, _t) = enter(tempdir().unwrap());
        let cmd = "worklog hook run";
        install(cmd).unwrap();
        install(cmd).unwrap();
        let s = status().unwrap();
        assert!(s.installed);
        assert_eq!(s.events.len(), EVENTS.len());

        let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let hooks = root.get("hooks").unwrap().as_object().unwrap();
        for ev in EVENTS {
            assert_eq!(hooks.get(*ev).unwrap().as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn install_sweeps_worklog_handlers_off_legacy_event_keys() {
        // Regression: an older worklog registered PreToolUse / PostToolUse
        // (which capture tool I/O — source code — into the events table).
        // `install` only manages EVENTS, so those stale handlers used to
        // linger forever. A fresh install must sweep them out.
        let (_g, _t) = enter(tempdir().unwrap());
        let settings = json!({
            "hooks": {
                "PreToolUse": [
                    { "hooks": [{ "type": "command", "command": "worklog hook-run" }] }
                ],
                "PostToolUse": [
                    { "hooks": [{ "type": "command", "command": "/x/worklog hook-run" }] },
                    { "hooks": [{ "type": "command", "command": "other-tool" }] }
                ]
            }
        });
        std::fs::write(
            settings_path().unwrap(),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        install("worklog hook-run").unwrap();

        let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let hooks = root.get("hooks").unwrap().as_object().unwrap();
        // PreToolUse had only a worklog handler → key removed entirely.
        assert!(
            hooks.get("PreToolUse").is_none(),
            "stale PreToolUse worklog handler must be swept"
        );
        // PostToolUse keeps the unrelated handler, loses the worklog one.
        let post = hooks.get("PostToolUse").unwrap().as_array().unwrap();
        assert_eq!(post.len(), 1, "only the non-worklog handler should remain");
        assert!(!is_worklog_handler(&post[0]));
        // …and the intended events are still installed.
        for ev in EVENTS {
            assert_eq!(hooks.get(*ev).unwrap().as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn install_replaces_an_earlier_python_handler() {
        let (_g, _t) = enter(tempdir().unwrap());
        let settings = json!({
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "/old/worklog hook run" }] }
                ]
            }
        });
        let path = settings_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();

        install("/new/worklog hook run").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(
            stop[0]["hooks"][0]["command"].as_str().unwrap(),
            "/new/worklog hook run"
        );
    }

    #[test]
    fn uninstall_removes_only_worklog_handlers() {
        let (_g, _t) = enter(tempdir().unwrap());
        let settings = json!({
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "worklog hook run" }] },
                    { "hooks": [{ "type": "command", "command": "/opt/other-tool/notify" }] }
                ]
            }
        });
        let path = settings_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();

        uninstall().unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(
            stop[0]["hooks"][0]["command"].as_str().unwrap(),
            "/opt/other-tool/notify"
        );
    }

    #[test]
    fn status_on_missing_file_reports_uninstalled() {
        let (_g, _t) = enter(tempdir().unwrap());
        let s = status().unwrap();
        assert!(!s.installed);
        assert!(s.events.is_empty());
    }

    #[test]
    fn uninstall_on_missing_file_is_a_noop() {
        let (_g, _t) = enter(tempdir().unwrap());
        let s = uninstall().unwrap();
        assert!(!s.installed);
    }
}
