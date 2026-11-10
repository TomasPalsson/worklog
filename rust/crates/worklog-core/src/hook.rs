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

/// Override the Codex config dir for tests and power users.
pub const ENV_CODEX_HOME: &str = "CODEX_HOME";

/// Resolve `~/.codex/hooks.json` (honouring `$CODEX_HOME` if set).
pub fn codex_hooks_path() -> Result<PathBuf> {
    let base = match std::env::var_os(ENV_CODEX_HOME) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => dirs::home_dir()
            .context("no home directory available")?
            .join(".codex"),
    };
    Ok(base.join("hooks.json"))
}

/// Default command string registered in the hook handler: the absolute
/// path of the running binary plus ` hook-run`, so the hook keeps working
/// when `worklog` isn't on the PATH Claude Code / Codex run it with.
/// Paths with shell-special characters are single-quoted (both agents run
/// the command through `sh -c`). Falls back to the bare `worklog hook-run`
/// if the current executable can't be resolved.
pub fn default_command() -> String {
    match std::env::current_exe() {
        Ok(exe) => {
            let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
            format!("{} hook-run", sh_quote(&exe.to_string_lossy()))
        }
        Err(_) => "worklog hook-run".to_owned(),
    }
}

/// Default command for the Codex CLI: the recorder tagged `--source codex`.
pub fn default_codex_command() -> String {
    format!("{} --source codex", default_command())
}

/// Quote `s` for `sh` unless it only has safe characters.
fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._+-".contains(c))
    {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Derive the `session-hint` invocation from the recorder's `hook-run`
/// command, keeping the same binary/back-compat alias.
// ponytail: falls back to appending " session-hint" when neither suffix
// matches (the bare legacy `worklog-hook` binary); upgrade if that binary
// ever needs to run session-hint too.
fn session_hint_command(command: &str) -> String {
    if let Some(prefix) = command.strip_suffix("hook-run") {
        return format!("{prefix}session-hint");
    }
    if let Some(prefix) = command.strip_suffix("hook run") {
        return format!("{prefix}session-hint");
    }
    format!("{command} session-hint")
}

pub fn install(command: &str) -> Result<HookStatus> {
    install_at(settings_path()?, command, true)
}

pub fn uninstall() -> Result<HookStatus> {
    uninstall_at(settings_path()?)
}

pub fn status() -> Result<HookStatus> {
    status_at(settings_path()?)
}

/// Register the recorder in `$CODEX_HOME/hooks.json` (no session-hint).
pub fn install_codex(command: &str) -> Result<HookStatus> {
    install_at(codex_hooks_path()?, command, false)
}

pub fn uninstall_codex() -> Result<HookStatus> {
    uninstall_at(codex_hooks_path()?)
}

pub fn status_codex() -> Result<HookStatus> {
    status_at(codex_hooks_path()?)
}

fn install_at(path: PathBuf, command: &str, session_hint: bool) -> Result<HookStatus> {
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
        if session_hint && *ev == "SessionStart" {
            arr.push(json!({
                "hooks": [
                    { "type": "command", "command": session_hint_command(command) }
                ]
            }));
        }
    }

    write_settings(&path, &Value::Object(root))?;
    Ok(HookStatus {
        settings_path: path,
        installed: true,
        events: EVENTS.iter().map(|s| (*s).to_owned()).collect(),
        command: Some(command.to_owned()),
    })
}

fn uninstall_at(path: PathBuf) -> Result<HookStatus> {
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

fn status_at(path: PathBuf) -> Result<HookStatus> {
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

pub(crate) fn read_settings(path: &Path) -> Result<Map<String, Value>> {
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

pub(crate) fn write_settings(path: &Path, v: &Value) -> Result<()> {
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

/// A worklog hook command: the program (everything before the first
/// `hook-run` / `session-hint` / `hook` word, or the whole command for the
/// bare `worklog-hook` binary) has a basename containing `worklog`. A
/// bare substring match would also eat foreign hooks that merely live under
/// a `worklog/` directory.
fn is_worklog_command(cmd: &str) -> bool {
    let words: Vec<&str> = cmd.split_whitespace().collect();
    let end = words
        .iter()
        .position(|w| matches!(*w, "hook-run" | "session-hint" | "hook"))
        .unwrap_or(words.len());
    let prog = words[..end].join(" ");
    let prog = prog.trim_matches('\'');
    prog.rsplit('/')
        .next()
        .is_some_and(|b| b.contains("worklog"))
}

fn is_worklog_handler(handler: &Value) -> bool {
    let Some(list) = handler.get("hooks").and_then(Value::as_array) else {
        return false;
    };
    list.iter().any(|h| {
        h.get("command")
            .and_then(Value::as_str)
            .map(is_worklog_command)
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
                if is_worklog_command(cmd) {
                    return Some(cmd.to_owned());
                }
            }
        }
    }
    None
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
        _codex: tempfile::TempDir,
    }

    fn enter(tmp: tempfile::TempDir) -> (std::sync::MutexGuard<'static, ()>, TestDir) {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_CLAUDE_HOME, tmp.path());
        let codex = tempdir().unwrap();
        std::env::set_var(ENV_CODEX_HOME, codex.path());
        (
            g,
            TestDir {
                _tmp: tmp,
                _codex: codex,
            },
        )
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            std::env::remove_var(ENV_CLAUDE_HOME);
            std::env::remove_var(ENV_CODEX_HOME);
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
            let expected = if *ev == "SessionStart" { 2 } else { 1 };
            assert_eq!(
                arr.len(),
                expected,
                "event {ev} must have {expected} handler(s), got {arr:?}"
            );
            let handler = arr[0].get("hooks").unwrap().as_array().unwrap();
            assert_eq!(handler[0]["command"].as_str().unwrap(), cmd);
        }
    }

    #[test]
    fn install_keeps_recorder_and_start_hint() {
        let (_g, _t) = enter(tempdir().unwrap());
        let cmd = "/usr/bin/worklog hook-run";
        install(cmd).unwrap();
        install(cmd).unwrap(); // idempotent: re-running must not duplicate

        let raw = std::fs::read_to_string(settings_path().unwrap()).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();
        let hooks = root.get("hooks").unwrap().as_object().unwrap();

        // SessionStart carries both the recorder and the start-hint command.
        let start = hooks.get("SessionStart").unwrap().as_array().unwrap();
        assert_eq!(
            start.len(),
            2,
            "SessionStart must have exactly 2 handlers, got {start:?}"
        );
        let commands: Vec<&str> = start
            .iter()
            .map(|h| h["hooks"][0]["command"].as_str().unwrap())
            .collect();
        assert_eq!(
            commands,
            vec!["/usr/bin/worklog hook-run", "/usr/bin/worklog session-hint"]
        );

        // Every other event keeps only the recorder.
        for ev in EVENTS {
            if *ev == "SessionStart" {
                continue;
            }
            let arr = hooks.get(*ev).unwrap().as_array().unwrap();
            assert_eq!(arr.len(), 1, "event {ev} must have 1 handler, got {arr:?}");
            assert_eq!(arr[0]["hooks"][0]["command"].as_str().unwrap(), cmd);
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
            let expected = if *ev == "SessionStart" { 2 } else { 1 };
            assert_eq!(hooks.get(*ev).unwrap().as_array().unwrap().len(), expected);
        }
    }

    #[test]
    fn install_sweeps_worklog_handlers_off_legacy_event_keys() {
        // Regression: an older worklog may have registered handlers on event
        // keys that are NOT in the current EVENTS set. `install`'s per-event
        // loop only manages EVENTS, so without the up-front full sweep those
        // stale worklog handlers would linger forever on the retired keys.
        //
        // Note: PreToolUse / PostToolUse are now first-class EVENTS heartbeats
        // (see the EVENTS doc comment — they keep long autonomous turns
        // visible to inference), so they are re-registered on install and are
        // NOT valid "legacy" examples. Use a genuinely-retired event key.
        let (_g, _t) = enter(tempdir().unwrap());
        let settings = json!({
            "hooks": {
                // Real Claude Code events that worklog does not register.
                "Notification": [
                    { "hooks": [{ "type": "command", "command": "worklog hook-run" }] }
                ],
                "PreCompact": [
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
        // Notification had only a worklog handler → key removed entirely.
        assert!(
            hooks.get("Notification").is_none(),
            "stale Notification worklog handler must be swept"
        );
        // PreCompact keeps the unrelated handler, loses the worklog one.
        let pre = hooks.get("PreCompact").unwrap().as_array().unwrap();
        assert_eq!(pre.len(), 1, "only the non-worklog handler should remain");
        assert!(!is_worklog_handler(&pre[0]));
        // …and the intended events are still installed.
        for ev in EVENTS {
            let expected = if *ev == "SessionStart" { 2 } else { 1 };
            assert_eq!(hooks.get(*ev).unwrap().as_array().unwrap().len(), expected);
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

    // ── Claude + Codex shared behaviour ──

    struct Target {
        codex: bool,
        path: fn() -> Result<PathBuf>,
        install: fn(&str) -> Result<HookStatus>,
        uninstall: fn() -> Result<HookStatus>,
        status: fn() -> Result<HookStatus>,
        suffix: &'static str,
    }

    const TARGETS: [Target; 2] = [
        Target {
            codex: false,
            path: settings_path,
            install,
            uninstall,
            status,
            suffix: "hook-run",
        },
        Target {
            codex: true,
            path: codex_hooks_path,
            install: install_codex,
            uninstall: uninstall_codex,
            status: status_codex,
            suffix: "hook-run --source codex",
        },
    ];

    fn read(t: &Target) -> Value {
        serde_json::from_str(&std::fs::read_to_string((t.path)().unwrap()).unwrap()).unwrap()
    }

    fn seed(t: &Target, v: &Value) {
        let p = (t.path)().unwrap();
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    }

    fn cmds(root: &Value, ev: &str) -> Vec<String> {
        root["hooks"][ev]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["hooks"][0]["command"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn install_twice_leaves_one_recorder_per_event() {
        let (_g, _t) = enter(tempdir().unwrap());
        for t in &TARGETS {
            let cmd = format!("/x/worklog {}", t.suffix);
            (t.install)(&cmd).unwrap();
            (t.install)(&cmd).unwrap();
            let root = read(t);
            for ev in EVENTS {
                let c = cmds(&root, ev);
                if !t.codex && *ev == "SessionStart" {
                    assert_eq!(c, vec![cmd.clone(), "/x/worklog session-hint".to_owned()]);
                } else {
                    assert_eq!(c, vec![cmd.clone()], "{ev} codex={}", t.codex);
                }
            }
        }
    }

    #[test]
    fn foreign_entries_survive_install_and_uninstall() {
        let (_g, _t) = enter(tempdir().unwrap());
        for t in &TARGETS {
            let seeded = json!({
                "theme": "dark",
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Bash", "hooks": [{ "type": "command", "command": "/opt/other" }] }
                    ],
                    "Notification": [
                        { "hooks": [{ "type": "command", "command": "/opt/notify" }] }
                    ]
                }
            });
            seed(t, &seeded);
            (t.install)(&format!("/x/worklog {}", t.suffix)).unwrap();
            let root = read(t);
            assert_eq!(root["theme"], seeded["theme"]);
            assert_eq!(
                root["hooks"]["Notification"],
                seeded["hooks"]["Notification"]
            );
            assert_eq!(
                root["hooks"]["PreToolUse"][0],
                seeded["hooks"]["PreToolUse"][0]
            );
            assert_eq!(root["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
            (t.uninstall)().unwrap();
            assert_eq!(read(t), seeded, "codex={}", t.codex);
        }
    }

    #[test]
    fn foreign_hook_under_a_worklog_directory_is_not_ours() {
        let (_g, _t) = enter(tempdir().unwrap());
        for t in &TARGETS {
            let seeded = json!({
                "hooks": { "Stop": [
                    { "hooks": [{ "type": "command", "command": "/Users/me/worklog/scripts/audit.sh" }] }
                ] }
            });
            seed(t, &seeded);
            (t.install)(&format!("/x/worklog {}", t.suffix)).unwrap();
            assert_eq!(read(t)["hooks"]["Stop"][0], seeded["hooks"]["Stop"][0]);
            (t.uninstall)().unwrap();
            assert_eq!(read(t), seeded, "codex={}", t.codex);
        }
        assert!(is_worklog_command("'/My Apps/worklog' hook-run"));
        assert!(is_worklog_command("worklog-hook"));
        assert!(is_worklog_command("worklog hook run"));
    }

    #[test]
    fn bare_and_absolute_entries_are_recognised_removed_and_replaced() {
        let (_g, _t) = enter(tempdir().unwrap());
        for t in &TARGETS {
            for old in [
                format!("worklog {}", t.suffix),
                format!("/opt/x/bin/worklog {}", t.suffix),
                format!("'/a b/worklog' {}", t.suffix),
                format!(
                    "[ -n \"$WORKLOG_DISABLE\" ] || \"$HOME/.local/bin/worklog\" {}",
                    t.suffix
                ),
            ] {
                let mut hooks = Map::new();
                for ev in EVENTS {
                    hooks.insert(
                        (*ev).to_owned(),
                        json!([{ "hooks": [{ "type": "command", "command": old }] }]),
                    );
                }
                seed(t, &json!({ "hooks": hooks }));
                let s = (t.status)().unwrap();
                assert!(s.installed, "{old}");
                assert_eq!(s.events.len(), EVENTS.len());

                (t.install)("/new/worklog hook-run").unwrap();
                let root = read(t);
                for ev in EVENTS {
                    let c = cmds(&root, ev);
                    assert_eq!(c[0], "/new/worklog hook-run", "{old}");
                    let n = if !t.codex && *ev == "SessionStart" {
                        2
                    } else {
                        1
                    };
                    assert_eq!(c.len(), n, "{old} {ev}");
                }

                seed(
                    t,
                    &json!({ "hooks": { "Stop": [
                        { "hooks": [{ "type": "command", "command": old }] }
                    ] } }),
                );
                (t.uninstall)().unwrap();
                assert_eq!(read(t), json!({}), "{old}");
                assert!(!(t.status)().unwrap().installed);
            }
        }
    }

    #[test]
    fn codex_install_has_no_session_hint_and_tags_source() {
        let (_g, _t) = enter(tempdir().unwrap());
        install_codex("/x/worklog hook-run --source codex").unwrap();
        let root = read(&TARGETS[1]);
        for ev in EVENTS {
            let c = cmds(&root, ev);
            assert_eq!(c.len(), 1);
            assert!(c[0].ends_with("hook-run --source codex"));
        }
        assert!(!std::fs::read_to_string(codex_hooks_path().unwrap())
            .unwrap()
            .contains("session-hint"));
        assert!(!settings_path().unwrap().exists());
        install("/x/worklog hook-run").unwrap();
        assert!(!codex_hooks_path()
            .unwrap()
            .with_extension("json.bak")
            .exists());
    }

    #[test]
    fn malformed_json_errors_and_is_left_untouched() {
        let (_g, _t) = enter(tempdir().unwrap());
        for t in &TARGETS {
            let p = (t.path)().unwrap();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "{ not json").unwrap();
            assert!((t.install)("worklog hook-run").is_err());
            assert!((t.uninstall)().is_err());
            assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");
        }
    }

    #[test]
    fn codex_backup_is_named_after_the_file() {
        let (_g, _t) = enter(tempdir().unwrap());
        install_codex("worklog hook-run --source codex").unwrap();
        install_codex("worklog hook-run --source codex").unwrap();
        assert!(codex_hooks_path()
            .unwrap()
            .with_file_name("hooks.json.bak")
            .exists());
    }

    #[test]
    fn codex_hooks_path_honours_codex_home() {
        let (_g, t) = enter(tempdir().unwrap());
        assert_eq!(
            codex_hooks_path().unwrap(),
            t._codex.path().join("hooks.json")
        );
    }

    #[test]
    fn default_command_is_absolute() {
        let c = default_command();
        assert!(c.starts_with('/') || c.starts_with("'/"), "{c}");
        assert!(c.ends_with(" hook-run"), "{c}");
        assert!(default_codex_command().ends_with(" hook-run --source codex"));
    }

    #[test]
    fn sh_quote_plain_space_and_quote() {
        assert_eq!(sh_quote("/usr/local/bin/worklog"), "/usr/local/bin/worklog");
        assert_eq!(sh_quote("/a b/worklog"), "'/a b/worklog'");
        assert_eq!(sh_quote("/a'b/worklog"), "'/a'\\''b/worklog'");
    }

    #[test]
    fn session_hint_derives_from_absolute_and_quoted_paths() {
        assert_eq!(
            session_hint_command("/usr/bin/worklog hook-run"),
            "/usr/bin/worklog session-hint"
        );
        assert_eq!(
            session_hint_command("'/a b/worklog' hook-run"),
            "'/a b/worklog' session-hint"
        );
    }
}
