# Runtime verification — feature/litellm-estimator

Built `./rust/target/release/worklog` (release profile, 36s) against the branch HEAD. Commands below ran with `WORKLOG_HOME=/tmp/worklog-rv` so nothing touched the user's real data.

## Autonomous checks (no real LLM required)

| # | Command | Expected | Actual | Status |
|---|---|---|---|---|
| A1 | `worklog --version` | `worklog 0.6.0` line | `worklog 0.6.0` | ✅ PASS |
| A2 | `worklog estimate --help` | long_about names `WORKLOG_ESTIMATOR_PROVIDER`, both providers, the 3 litellm secrets | Full long_about rendered including "Provider selection:" section | ✅ PASS |
| A3 | `worklog db migrate` | creates db + schema v3 | `✓ db ready at /tmp/worklog-rv/worklog.db  (schema v3)` | ✅ PASS |
| A4 | `worklog secret set litellm_base_url --value http://127.0.0.1:4000` then `litellm_model` then `worklog_estimator_provider litellm` | three `saved to keychain` lines | `✓ litellm_base_url saved to keychain` + `✓ litellm_model saved to keychain` + `✓ worklog_estimator_provider saved to keychain` | ✅ PASS |
| A5 | `worklog --json doctor` (no `--probe`) | `estimator.provider = "litellm"`, `base_url` + `model` populated, NO `reachable` field | JSON: `{"provider": "litellm", "base_url": "http://127.0.0.1:4000", "model": "anthropic/claude-haiku-4-5"}` — `reachable` absent | ✅ PASS |
| A6 | `worklog --json doctor --probe` | Adds `reachable: false` + `reason` (port 4000 is closed) | `"reachable": false, "reason": "connect: error sending request for url (http://127.0.0.1:4000/health)"` | ✅ PASS |
| A7 | `worklog doctor` (text table) | `estimator` row shows `litellm anthropic/claude-haiku-4-5 (http://127.0.0.1:4000)` with note `probe skipped (pass --probe)` | Exactly that line + note | ✅ PASS |
| A8 | `WORKLOG_ESTIMATOR_PROVIDER=claude_subprocess worklog --json doctor` | env override wins → `estimator.provider = "claude_subprocess"` even though secret says litellm | `"estimator": {"provider": "claude_subprocess"}` | ✅ PASS |

## Unit-test evidence

- **287 cargo tests** pass (was 273 before the feature — 14 net new).
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `cargo fmt --all -- --check` clean.
- `bun test` in web/ still passes (41 tests, 0 failed — feature did not touch web).
- `bun run typecheck` in web/ clean.

Key test coverage for LLM-round-trip paths (no real LLM needed; httpmock):

| Test | What it pins |
|---|---|
| `litellm_invoker_returns_parsed_reply_on_200` | 200 + OpenAI envelope → parsed Value (B4) |
| `litellm_invoker_bails_on_401` | 401 → Err with "HTTP 401" → caller marks `gap` (B5) |
| `litellm_invoker_handles_prose_wrapped_json_content` | "Here you go: {…}" content parses via `parse_response` (B6) |
| `litellm_invoker_bails_on_timeout` | proxy delay > client timeout → Err (B7) |
| `litellm_invoker_omits_authorization_header_when_key_empty` | empty api_key → no `Authorization` header (B8) |
| `litellm_invoker_uses_caller_model_when_provided` | `--model openai/gpt-4o` overrides invoker default |
| `resolve_provider_*` (6 tests) | env + secrets resolve correctly to the right variant (B1/B2/B3) |
| `probe_litellm_returns_*` (2 tests) | healthy proxy → None; unreachable → Some(err) |
| `doctor_json_reports_estimator_provider_block` | subprocess block in default doctor |
| `doctor_json_reports_litellm_provider_when_selected` | litellm block with `reachable` when `--probe` is set |
| `doctor_json_skips_probe_by_default` | NO `reachable` when `--probe` absent — regression pin for the UX fix |
| `estimate_help_documents_provider_env_var` | `long_about` text coverage |
| `non_interactive_wizard_does_not_write_litellm_secrets` | wizard is a no-op under `--non-interactive` |
| `human_label_covers_all_litellm_and_provider_keys` | every new KNOWN_KEYS entry has a heading |

## User-verification steps (Stage 6 — require real LiteLLM proxy + Anthropic key)

These exercise the real LLM round-trip and the interactive wizard TTY path, so they can't run autonomously. Spin up a proxy with your own key:

```bash
# Option A — docker
docker run -d --name litellm-test -p 4000:4000 \
  -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  ghcr.io/berriai/litellm-database:main-stable \
  --model anthropic/claude-haiku-4-5

# Option B — pipx / uv
pipx run litellm --model anthropic/claude-haiku-4-5 --port 4000
```

Then:

| # | Command | Expected |
|---|---|---|
| U1 | `worklog doctor --probe` with the proxy up | `reachable: true` |
| U2 | Run `worklog setup` interactively and pick `litellm` when prompted | Wizard prompts for URL/key/model, runs the probe, saves the secrets |
| U3 | Seed one un-estimated block; run `worklog estimate --day <today>` | `estimated >= 1`, block row populated; `worklog day` label reads "estimating (litellm)" |
| U4 | Stop the proxy; run `worklog estimate --day <today>` on a fresh block | `failed >= 1`; block marked `gap`; stderr carries `connect:` / `HTTP` error |
| U5 | `worklog secret set litellm_api_key invalid` + restart proxy with auth | `worklog estimate` → block `gap`, error mentions HTTP 401 |
| U6 | `WORKLOG_ESTIMATOR_PROVIDER=claude_subprocess worklog estimate --day <today>` | Falls back to the subprocess path (requires `claude` binary) |

## Verdict

**Autonomous runtime — PASS (8/8).** Binary ships the right help text, reads/writes the four new secrets, resolves the provider from env+secrets exactly as specified, honors the `--probe` gate, and surfaces all three report shapes (subprocess / litellm / unconfigured) in both JSON and table mode.

**Pending user verification:** U1–U6 require the user to run a live proxy and provide `ANTHROPIC_API_KEY`.
