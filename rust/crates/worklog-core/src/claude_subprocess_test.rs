//! Tests for `claude_subprocess::command_spec` — the args/env `claude -p`
//! gets, asserted without spawning a process — plus `bounded_concurrent_invoke`'s
//! rolling-window fan-out, against a fake invoker.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// SLICE T11 round 5 (r5-rolling): a fake invoker whose per-call sleep
/// varies, `users[i]` carrying `i` itself so `invoke` can look its delay
/// up. Chunks of 4 would pay `max(400,50,50,50) + max(50,50,50,50)` =
/// ~450ms (the first chunk waits out its one slow call before the
/// second chunk's fast ones even start); a rolling window instead lets
/// the three other workers drain the remaining fast items while the
/// slow one is still running, finishing in ~400ms — bounded by the slow
/// call alone, not by chunk boundaries.
struct RollingWindowInvoker {
    sleeps_ms: Vec<u64>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl ModelInvoker for RollingWindowInvoker {
    fn invoke(&self, _system: &str, user: &str, _schema: &Value, _model: &str) -> Result<Value> {
        let idx: usize = user.parse().expect("test user is an index");
        let now_in_flight = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight
            .fetch_max(now_in_flight, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(self.sleeps_ms[idx]));
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(Value::String(user.to_string()))
    }

    fn invoke_many(
        &self,
        system: &str,
        users: &[String],
        schema: &Value,
        model: &str,
    ) -> Vec<Result<Value>> {
        bounded_concurrent_invoke(self, system, users, schema, model)
    }
}

#[test]
fn bounded_concurrent_invoke_rolls_past_a_slow_call_in_order_and_bounded() {
    let sleeps_ms = vec![400, 50, 50, 50, 50, 50, 50, 50];
    let users: Vec<String> = (0..sleeps_ms.len()).map(|i| i.to_string()).collect();
    let invoker = RollingWindowInvoker {
        sleeps_ms,
        in_flight: AtomicUsize::new(0),
        max_in_flight: AtomicUsize::new(0),
    };

    let started = Instant::now();
    let results = bounded_concurrent_invoke(&invoker, "sys", &users, &Value::Null, "model-x");
    let elapsed = started.elapsed();

    for (i, result) in results.iter().enumerate() {
        assert_eq!(
            result.as_ref().unwrap().as_str().unwrap(),
            users[i],
            "result {i} was not zipped back onto its own input"
        );
    }
    assert!(
        elapsed < std::time::Duration::from_millis(440),
        "rolling window took {elapsed:?}, expected ~400ms (bounded by the one slow \
         call), well under chunked's ~450ms (a slow call's chunk stalls the next chunk)"
    );
    assert!(
        invoker.max_in_flight.load(Ordering::SeqCst) <= 4,
        "more than MAX_CONCURRENT_INVOKES calls were in flight at once"
    );
}
