//! Tests for `claude_subprocess::command_spec` — the args/env `claude -p`
//! gets, asserted without spawning a process.

use super::*;

#[test]
fn default_has_no_thinking_env() {
    let (args, envs) = command_spec("sys", "{}", "model-x", None);
    assert!(args.contains(&"--model".to_string()));
    assert!(args.contains(&"model-x".to_string()));
    assert!(!envs.iter().any(|(k, _)| k == "MAX_THINKING_TOKENS"));
    assert!(envs
        .iter()
        .any(|(k, v)| k == crate::hook_run::SUPPRESS_ENV && v == "1"));
}

fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
    args.windows(2).any(|w| w[0] == flag && w[1] == value)
}

#[test]
fn every_call_runs_without_tools_plugins_or_user_settings() {
    // Without these a text call loads every user hook/plugin/MCP server and
    // may wander off using tools — minutes instead of seconds.
    for thinking in [None, Some(8000)] {
        let (args, _) = command_spec("sys", "{}", "model-x", thinking);
        assert!(has_pair(&args, "--tools", ""), "{args:?}");
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert!(has_pair(&args, "--setting-sources", "project"));
    }
}

#[test]
fn thinking_call_turns_thinking_on_explicitly() {
    // User settings are skipped, so thinking must be requested here.
    let (args, _) = command_spec("sys", "{}", "model-x", Some(8000));
    assert!(has_pair(
        &args,
        "--settings",
        r#"{"alwaysThinkingEnabled":true}"#
    ));
    assert!(has_pair(&args, "--effort", "high"));
    let (plain, _) = command_spec("sys", "{}", "model-x", None);
    assert!(!plain.contains(&"--effort".to_string()));
}

#[test]
fn with_thinking_sets_max_thinking_tokens() {
    let (_, envs) = command_spec("sys", "{}", "model-x", Some(8000));
    assert!(envs
        .iter()
        .any(|(k, v)| k == "MAX_THINKING_TOKENS" && v == "8000"));
}

#[test]
fn default_invoker_timeout_is_unchanged() {
    let inv = ClaudeSubprocess::default();
    assert_eq!(inv.timeout_secs(), DEFAULT_TIMEOUT_SECS);
}

#[test]
fn with_thinking_uses_the_longer_timeout() {
    let inv = ClaudeSubprocess::with_thinking(8000);
    assert_eq!(inv.timeout_secs(), THINKING_TIMEOUT_SECS);
    assert_eq!(inv.thinking_tokens, Some(8000));
}
