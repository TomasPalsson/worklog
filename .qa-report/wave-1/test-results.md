# Test results — feature/litellm-estimator

## Commands run

```
cargo test    --manifest-path rust/Cargo.toml
cargo clippy  --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt     --manifest-path rust/Cargo.toml --all -- --check
cd web && bun test && bun run typecheck
```

## Cargo test (Rust)

**287 passed / 0 failed / 0 ignored** across 8 test suites (worklog-core lib, worklog-cli lib, worklog-cli tests/cli, worklog-cli tests/daemon, …).

14 net new tests on this branch:

| Test | Pins behaviour |
|---|---|
| `secrets::tests::known_keys_include_litellm_provider_settings` | 4 new KNOWN_KEYS entries |
| `secrets::tests::env_var_mapping_for_litellm_keys` | WORKLOG_LITELLM_* + WORKLOG_ESTIMATOR_PROVIDER env names |
| `estimate::tests::litellm_invoker_returns_parsed_reply_on_200` | OpenAI envelope → parsed Value (B4) |
| `estimate::tests::litellm_invoker_bails_on_401` | HTTP 401 → actionable Err (B5) |
| `estimate::tests::litellm_invoker_handles_prose_wrapped_json_content` | prose-wrapped JSON → parse_response (B6) |
| `estimate::tests::litellm_invoker_bails_on_timeout` | 200ms client vs 500ms proxy delay (B7) |
| `estimate::tests::litellm_invoker_omits_authorization_header_when_key_empty` | empty api_key → no Auth header (B8) |
| `estimate::tests::litellm_invoker_uses_caller_model_when_provided` | caller `--model` wins |
| `estimate::tests::resolve_provider_defaults_to_claude_subprocess_when_nothing_set` | back-compat (B1) |
| `estimate::tests::resolve_provider_picks_litellm_when_env_says_so_and_url_present` | env + secrets → LiteLLM (B2) |
| `estimate::tests::resolve_provider_errors_when_litellm_selected_but_url_missing` | actionable error (B3) |
| `estimate::tests::resolve_provider_reads_secret_when_env_unset` | keychain fallback |
| `estimate::tests::resolve_provider_uses_default_litellm_model_when_model_secret_missing` | DEFAULT_LITELLM_MODEL fallback |
| `estimate::tests::resolve_provider_errors_on_unknown_provider_string` | typo → loud error |
| `wizard::tests::human_label_covers_all_litellm_and_provider_keys` | wizard header coverage |
| `wizard::tests::probe_litellm_returns_none_on_healthy_proxy` | /health 200 → None |
| `wizard::tests::probe_litellm_returns_err_string_when_unreachable` | refused port → Some(err) |
| `wizard::tests::non_interactive_wizard_does_not_write_litellm_secrets` | CI-safe no-op |
| `cli` integration `doctor_json_reports_estimator_provider_block` | subprocess block |
| `cli` integration `doctor_json_reports_litellm_provider_when_selected` | litellm block with `--probe` |
| `cli` integration `doctor_json_skips_probe_by_default` | regression pin on the UX fix |
| `cli` integration `estimate_help_documents_provider_env_var` | long_about present |

## Clippy

`cargo clippy --all-targets --all-features -- -D warnings` → **0 warnings.**

## Rustfmt

`cargo fmt --all -- --check` → **clean** after applied formatter.

## Web

- `bun test` → 41 passed / 0 failed / 0 skipped.
- `bun run typecheck` → 0 errors.

## Coverage gaps

- **B9** (non-interactive wizard is a no-op on new step) — covered by `non_interactive_wizard_does_not_write_litellm_secrets`.
- **B10** (interactive wizard picks LiteLLM + saves secrets) — NOT covered by cargo tests (dialoguer requires TTY). Covered manually by **U2** in runtime-results.md.
- **B11** (wizard probe failure → "save anyway?") — NOT covered by cargo tests (dialoguer). User verification **U2** variant.
- **B12/B13** (doctor JSON/table surfaces) — fully covered by `doctor_json_reports_*` + the autonomous A5–A7 runtime checks.

No failing tests, no ignored tests, no skipped tests. `#[should_panic]` count unchanged vs main.
