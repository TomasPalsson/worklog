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
