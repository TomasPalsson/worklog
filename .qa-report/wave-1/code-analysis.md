# Code analysis — feature/litellm-estimator

## Scope

Summary of the sonnet agents already run in Stage 4 (full reports at `.claude/quality/*.md`):

| Dimension | Scanner | Fixes applied | Residual |
|---|---|---|---|
| Security | auto-pr:security-scanner | Scheme validation; 1 MiB response cap | Low-severity "full envelope in `extract_message_content` error" tracked as follow-up |
| Performance | feature-dev:code-reviewer | Memoised `system_with_schema` in OnceLock; `--probe` gated | None |
| Type-safety | pr-review-toolkit:type-design-analyzer | Test uses public `configured_model()` | 3 cosmetic follow-ups (EstimatorReport → enum variants, `Option<String>` → `Result<(), String>`, `provider_label` match in cmd_day) |
| Accessibility | — (no UI) | N/A | N/A |

## Risk areas for Wave 2

The edge-case hunter should concentrate on these surfaces that earlier scanners did NOT stress-test:

1. **`read_trimmed_secret` vs keychain empty strings** (`estimate.rs:92-96`). Behaviour when `get()` returns `Some("   ")` vs `Some("")` vs `None`. Is any code path relying on `get()` being non-trimmed?
2. **Provider string casing** (`estimate.rs:156-160`). `"LITELLM"`, `" litellm "`, `"LiteLLM"` all go through `to_lowercase()` — verify by construction, but also verify there's no `if v == "litellm"` comparison that would miss the casing fold somewhere else.
3. **`--model` passthrough** (`estimate.rs:324-330`). What if caller passes whitespace-only model like `"   "`? Currently `is_empty()` would be false and we'd send the junk model to the proxy.
4. **Response envelope edge cases**. `choices: []` (empty array). `choices[0]` lacking `message`. `choices[0].message.content` being null / non-string / an object. `extract_message_content` handles missing pointer but what about wrong-type values?
5. **Base URL edge cases**. `http://localhost:4000/` (trailing slash — handled). `http:///foo` (malformed). `http://localhost:4000/v1` (user accidentally includes the path prefix — would we POST to `…/v1/v1/chat/completions`?).
6. **Wizard re-run behavior**. User runs `worklog setup`, picks LiteLLM, saves. Re-runs setup, picks subprocess: does `worklog_estimator_provider` secret get deleted cleanly? (Checked `configure_estimator_provider`: yes, `secrets::delete` is called on the subprocess arm.)
7. **Concurrent `worklog estimate` calls.** The `LiteLLMInvoker` holds a `Client` which is thread-safe per reqwest docs, but `OnceLock<String>` has interior mutability — does `get_or_init` race correctly? (It's `Sync` per the `OnceLock` contract.)
8. **Secret ordering in `secrets.rs:15-30`.** The new keys were added at the end of KNOWN_KEYS — does any test or UI walk that order and break on the additions? (Integration test `secret_list_shows_state` only checks for `jira_email` substring, safe.)

## Known NOT TESTED dimensions

- **TTY-driven wizard flow** — cargo can't drive dialoguer prompts hermetically.
- **Real LLM round-trip** — covered by the live-proxy user verification U3–U6.
- **Non-TLS HTTPS (self-signed)** — `http::client()` uses rustls; user-provided self-signed LiteLLM proxies over HTTPS would fail. Documented behaviour: use HTTP for local, HTTPS for trusted endpoints.
