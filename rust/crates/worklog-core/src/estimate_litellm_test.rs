//! `LiteLLMInvoker` request-shape tests split out of `estimate.rs`.

use super::*;

/// Probed 2026-09-29: gpt-6-luna reasons inside `max_tokens`; at 512 a
/// full day's clues spent it all on reasoning → empty content →
/// "no JSON object in response". Leave room for reasoning + answer.
#[test]
fn litellm_request_leaves_room_for_reasoning() {
    let inv = LiteLLMInvoker::new("http://localhost:4000", "k", "gpt-6-luna").unwrap();
    let body = inv
        .build_request_body("SYS", "user", &response_schema(), "")
        .unwrap();
    assert!(body["max_tokens"].as_u64().unwrap() >= 4096, "{body}");
}
