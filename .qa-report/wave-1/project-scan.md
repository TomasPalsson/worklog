# Project scan — feature/litellm-estimator

## Tech stack
- **Primary:** Rust workspace (`rust/crates/worklog-core`, `worklog-cli`) + cargo
- **Secondary:** Next.js 15 + Bun (`web/`) — **untouched** by this branch
- **Tests:** inline `#[cfg(test)] mod tests` in every module + `tests/cli.rs` integration tests (assert_cmd + predicates)
- **Mock HTTP:** `httpmock = "0.7"` (already in worklog-core dev-deps; added to worklog-cli dev-deps in this branch)
- **Secrets:** `keyring` crate + `.env` fallback + `WORKLOG_SECRETS_FILE` file shim for tests

## App URL / browser
- **N/A.** This is a CLI + daemon feature. No HTTP-facing UI changes. `web/` node_modules not even required for the QA target.

## Auth requirements
- **N/A for this branch.** User-supplied `litellm_api_key` is the only new credential; it's written to the OS keychain via the same path as `anthropic_api_key` / `github_token` / etc.

## Changed files (via `git diff --stat main...HEAD`)

| File | Lines | Category |
|---|---|---|
| `rust/crates/worklog-core/src/secrets.rs` | +59/-0 | 4 new KNOWN_KEYS + env mappings + tests |
| `rust/crates/worklog-core/src/estimate.rs` | +793/-0 | LiteLLMInvoker + ProviderChoice + resolve_provider + probe_litellm + tests |
| `rust/crates/worklog-cli/src/wizard.rs` | +220/+0 | configure_estimator_provider step + 4 new human_labels + 4 tests |
| `rust/crates/worklog-cli/src/cli.rs` | +140/-3 | doctor `--probe` + estimator JSON/table block + estimate long_about |
| `rust/crates/worklog-cli/tests/cli.rs` | +110/+0 | 4 new CLI integration tests |
| `rust/crates/worklog-cli/Cargo.toml` | +4 | httpmock dev-dep |
| `rust/Cargo.lock` | +1 | deps |
| `README.md` | +24 | "Switching estimator provider" section |
| `.claude/*.md` | — | planning/quality/verification artifacts (not code) |

**10 code files changed, ~1300 net new lines, zero web/ changes.**

## Changed endpoints / commands

- `worklog doctor` — new `--probe` flag; JSON + table gain an `estimator` block.
- `worklog estimate --help` — new `long_about` documenting provider selection.
- `worklog setup` — new "estimator provider" wizard step between daemon-auto-start and the final done screen.
- `worklog secret set <key>` — accepts 4 new keys by name.
- `worklog day` — estimator spinner label becomes provider-aware.

No new subcommands. No removed subcommands. Back-compat: every existing install sees no behavioural change unless it sets one of the new env vars / secrets.

## Changed runtime surfaces

- **Outbound HTTP:** `LiteLLMInvoker` POSTs to `{user_base_url}/v1/chat/completions`. Scheme restricted to http/https at construction. 30s timeout via shared `http::client()`. Response capped at 1 MiB.
- **Wizard probe:** `probe_litellm` GETs `{user_base_url}/health` with 3s timeout, only interactively.
- **Doctor probe:** same `probe_litellm`, ONLY when `--probe` or `WORKLOG_DOCTOR_PROBE=1` is set. Default off.

## Databases

No schema changes. `db::SCHEMA_VERSION` still 3.
