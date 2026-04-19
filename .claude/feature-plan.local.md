# Feature Plan — LiteLLM estimator provider

**Branch:** `feature/litellm-estimator`
**Worktree:** `../code-worktrees/feature-litellm-estimator`
**Size:** medium
**Target release:** v0.7.0 (minor)

## Why

Today `estimate.rs` hard-invokes `claude -p` as a subprocess. Adding a LiteLLM-compatible HTTP path gives you a second provider you can point anywhere (local LiteLLM proxy, fly.io, hosted, etc.), opens the door to non-Anthropic models without changing worklog code, and keeps the existing `claude -p` workflow intact. Groundwork for the bigger "kill the subprocess" idea without forcing it today.

## Scope

**In**
- A new `LiteLLMInvoker` implementing the existing `ModelInvoker` trait in `estimate.rs`.
- Three new secret keys: `litellm_base_url`, `litellm_api_key`, `litellm_model`.
- A provider-selection env var `WORKLOG_ESTIMATOR_PROVIDER=claude_subprocess|litellm` with a `worklog_estimator_provider` secret fallback.
- `estimate_day()` internally routes to the right invoker via a new `resolve_provider()` helper. Public signature unchanged.
- `worklog setup` gains a new wizard step "estimator provider" that picks between the two options. For LiteLLM, it prompts for URL/key/model with a `/health` validation probe (best-effort, same "save anyway?" pattern as existing).
- `worklog doctor` reports the active provider + (for LiteLLM) `reachable: bool`.
- Back-compat: if no LiteLLM config is present AND no env var is set, behaviour is identical to today (`ClaudeSubprocess`).

**Out (explicit non-goals)**
- No auto-provisioned LiteLLM docker container (Q1=A = BYO endpoint).
- No removal/deprecation of `ClaudeSubprocess` (Q2=B = coexist).
- No first-class wizard support for OpenAI/Bedrock/Ollama/etc (Q3 = Anthropic only; other providers go through the plain LiteLLM flow with a custom URL/model).
- No parallelisation, no Batches API, no retry backoff — separate features.
- No UI changes in `web/`. Provider is invisible to the web layer.

## Files touched

| File | Change |
|---|---|
| `rust/crates/worklog-core/src/secrets.rs` | Add 4 keys to `KNOWN_KEYS` (`litellm_base_url`, `litellm_api_key`, `litellm_model`, `worklog_estimator_provider`); add `WORKLOG_LITELLM_*` + `WORKLOG_ESTIMATOR_PROVIDER` env mappings. |
| `rust/crates/worklog-core/src/estimate.rs` | Add `LiteLLMInvoker` struct + `impl ModelInvoker`; add `ProviderChoice` enum; add `resolve_provider()` factory; rewire `estimate_day` to go through factory. |
| `rust/crates/worklog-cli/src/wizard.rs` | Add `configure_estimator_provider()` step invoked from `run()` after `configure_daemon_service`. Extend `human_label` with 4 new keys. |
| `rust/crates/worklog-cli/src/cli.rs` | `cmd_doctor` adds "estimator" table block + JSON field. `cmd_day` estimating-label becomes provider-aware. `long_about` on `Cmd::Estimate`. |
| `README.md` | Short "Switching estimator provider" section under Storage/Config. |

Total: ~5 source files + README. Expected diff: **~500–700 lines** (incl. tests).

## Data model

Four new keychain keys (strings, all optional):

- `litellm_base_url` — e.g. `http://localhost:4000` or `https://litellm.fly.dev`. Trailing slash tolerated.
- `litellm_api_key` — bearer token the proxy expects. Empty string OK (local proxies often run unauthed).
- `litellm_model` — LiteLLM-format model id, e.g. `anthropic/claude-haiku-4-5`, `openai/gpt-4o-mini`, `ollama/llama3`. Default `DEFAULT_LITELLM_MODEL = "anthropic/claude-haiku-4-5"`.
- `worklog_estimator_provider` — `"claude_subprocess"` | `"litellm"`. Env `WORKLOG_ESTIMATOR_PROVIDER` wins over the secret.

All resolved via the existing `secrets::get()` which already handles keychain → file shim → `.env` fallback.

## HTTP shape for `LiteLLMInvoker`

```
POST {base_url}/v1/chat/completions
Authorization: Bearer {api_key}          (omitted if key empty)
Content-Type: application/json

{
  "model": "{litellm_model}",
  "messages": [
    {"role": "system",
     "content": "{SYSTEM_PROMPT}\n\nRespond ONLY with JSON matching this schema:\n{schema_json}"},
    {"role": "user", "content": "{user_message}"}
  ],
  "response_format": {"type": "json_object"},
  "temperature": 0,
  "max_tokens": 512
}
```

Response: `{"choices": [{"message": {"content": "<json string>"}}]}`.

`LiteLLMInvoker::invoke` extracts `choices[0].message.content` and hands it to the **existing `parse_response`** helper — it already tolerates envelope wrapping and prose-wrapped JSON.

Timeouts: 30s via `http::client()` pattern. No retry in this feature.

## Behaviour inventory (Given / When / Then)

| # | Given | When | Then | Verified by |
|---|---|---|---|---|
| B1 | No LiteLLM secrets, no env var | `estimate_day(…)` called | `resolve_provider` returns `ClaudeSubprocess`; behaviour identical to today | Phase 3 test |
| B2 | `WORKLOG_ESTIMATOR_PROVIDER=litellm`, `litellm_base_url` + `litellm_model` set | `resolve_provider()` | Returns `LiteLLM(LiteLLMInvoker)` configured with the secrets | Phase 3 test |
| B3 | `WORKLOG_ESTIMATOR_PROVIDER=litellm`, `litellm_base_url` missing | `resolve_provider()` | Returns a hard error mentioning `litellm_base_url` and pointing at `worklog setup` | Phase 3 test |
| B4 | `LiteLLMInvoker` configured, endpoint 200 with OpenAI envelope carrying `"{\"jira_issue\":\"PROJ-1\",…}"` | `invoke()` | Returns `Value` with `jira_issue = "PROJ-1"` | Phase 2 httpmock test |
| B5 | Endpoint returns 401 | `invoke()` | Returns `Err` containing "HTTP 401"; caller marks block `gap` | Phase 2 test |
| B6 | Endpoint content is prose-wrapped JSON: `"Here you go: {\"jira_issue\":…}"` | `invoke()` | Returns parsed Value (delegates to existing `parse_response`) | Phase 2 test |
| B7 | Endpoint delayed > client timeout | `invoke()` | Returns `Err` containing "timed out"/"timeout"; caller marks block `gap` | Phase 2 test (httpmock delay) |
| B8 | `litellm_api_key` is empty string | `invoke()` | Request sent WITHOUT `Authorization` header | Phase 2 test |
| B9 | Wizard non-interactive + no existing LiteLLM secrets | `run(WizardOptions{non_interactive:true, …})` | `configure_estimator_provider` is a no-op; wizard report includes `estimator: claude_subprocess` | Phase 4 test |
| B10 | Wizard interactive, user picks "LiteLLM", enters URL/key/model, probe succeeds | wizard run | Four secrets saved via `secrets::set`; `worklog_estimator_provider` = `"litellm"` | Phase 4 test (file-backed secrets) |
| B11 | Wizard probe fails (unreachable URL) | wizard run | Shows "probe failed" warning; `Confirm save anyway?` default=false (existing pattern) | Phase 4 test |
| B12 | `worklog doctor --json`, provider=litellm, reachable | CLI exec | JSON contains `"estimator": {"provider": "litellm", "base_url": "http://…", "reachable": true, "model": "anthropic/…"}` | Phase 5 integration test |
| B13 | `worklog doctor`, provider=claude_subprocess | CLI exec | Table row "estimator: claude_subprocess" + path or "not found" | Phase 5 test |

## TDD phase triplets

### Phase 1 — secrets + env plumbing (XS)

**Red**
- `secrets.rs` tests asserting `KNOWN_KEYS` contains the 4 new keys.
- `secrets.rs` test asserting `env_var_for("litellm_base_url") == Some("WORKLOG_LITELLM_BASE_URL")` + likewise for the other three.
- Run `cargo test secrets::` → new tests FAIL. Commit `test(secrets): RED — new LiteLLM + provider keys`.

**Green**
- Extend `KNOWN_KEYS` array + `env_var_for` match with the four new keys.
- Tests → green. Commit `feat(secrets): GREEN — register LiteLLM + provider keys`.

**Refactor**
- Apply `clean-code` skill to `secrets.rs`. No behaviour change. Commit `refactor(secrets): clean-code pass` only if something actually changed.

### Phase 2 — `LiteLLMInvoker` (S)

**Red**
- New test functions inside existing `#[cfg(test)] mod tests` in `estimate.rs` covering B4, B5, B6, B7, B8. All use `httpmock` (already a dev-dep — see `tempo.rs` tests).
- Tests reference a `LiteLLMInvoker { base_url, api_key, model, client }` struct that doesn't exist yet.
- Compile fails. Commit `test(estimate): RED — LiteLLMInvoker HTTP contract`.

**Green**
- Add `LiteLLMInvoker` struct with `pub fn new(base_url, api_key, model) -> Result<Self>` (trims trailing `/`, builds a `reqwest::blocking::Client` with 30s timeout).
- `impl ModelInvoker for LiteLLMInvoker { fn invoke(&self, system, user, schema, model) -> Result<Value> }`.
  - If the caller-passed `model` is non-empty, prefer it over `self.model` (lets `--model` override per-call).
  - Body uses `response_format: {"type": "json_object"}`, `temperature: 0`, `max_tokens: 512`.
  - On non-2xx, `bail!("HTTP {status}: {body_preview_500_chars}")`.
  - On 200, extracts `choices[0].message.content: String` and calls the existing `parse_response` to get a `Value`.
- Tests → green. Commit `feat(estimate): GREEN — LiteLLMInvoker implementation`.

**Refactor**
- Apply `clean-code`. Likely extractions: `fn build_request_body(…)`, `fn extract_content(v: &Value) -> Result<String>`.
- Ensure `parse_response` is reused, not duplicated.
- `cargo clippy` clean. Commit `refactor(estimate): clean-code pass — extract helpers`.

### Phase 3 — provider factory + `resolve_provider` (XS)

**Red**
- Tests covering B1, B2, B3.
- Return shape: `enum ProviderChoice { ClaudeSubprocess, LiteLLM(LiteLLMInvoker) }` (decided up-front so tests can pattern-match).
- Env var mutation serialized with a test-local `Mutex` (same pattern as `secrets.rs`).
- Compile fails. Commit `test(estimate): RED — provider resolution`.

**Green**
- Add the enum.
- `pub fn resolve_provider() -> Result<ProviderChoice>` reads env + secrets and builds the right variant; returns actionable errors on missing config.
- Rewire `estimate_day` to call `resolve_provider()?`, then `match` to dispatch via `estimate_day_with` with the concrete invoker.
- Tests → green. Commit `feat(estimate): GREEN — provider factory`.

**Refactor**
- `clean-code` pass. If `estimate.rs` grows above ~950 lines, split LiteLLM + provider into `estimator/litellm.rs` submodule. Decision deferred to the actual Refactor step.

### Phase 4 — wizard estimator step (S)

**Red**
- Test for `configure_estimator_provider(opts)` that, with `non_interactive=true`, is a no-op.
- Test that, when the secrets are pre-populated with `litellm_*`, the wizard report indicates `estimator: litellm`.
- Test for `human_label` returning labels for the 4 new keys.
- Compile fails. Commit `test(wizard): RED — estimator provider step`.

**Green**
- Add `fn configure_estimator_provider(theme, notes, opts) -> Result<Option<String>>` to `wizard.rs`:
  - Section header "estimator provider".
  - `Select` between `claude -p (subprocess)` and `litellm (HTTP proxy)` with default = whatever's currently configured or `claude -p`.
  - LiteLLM picked: prompt URL (required), api_key (password, empty OK), model (default `anthropic/claude-haiku-4-5`); run `probe_litellm(url)` best-effort; "save anyway?" on failure.
  - Save the 3 secrets + `worklog_estimator_provider = "litellm"` via `secrets::set`.
  - Subprocess picked: delete any prior `worklog_estimator_provider` secret so env takes over cleanly.
- Extend `human_label` with the 4 new keys.
- Hook into `wizard::run` after `configure_daemon_service`.
- Tests → green. Commit `feat(wizard): GREEN — estimator provider picker`.

**Refactor**
- `clean-code`. Probe logic moved to `fn probe_litellm(base_url: &str) -> Option<String>` returning a validation error string on failure, `None` on success. Mirrors shape of `validate_github_token`.

### Phase 5 — CLI doctor + help (XS)

**Red**
- `tests/cli.rs`: `worklog doctor --json` output includes an `"estimator"` object with `"provider"` and (conditionally) `"base_url"` / `"reachable"`.
- `estimate` subcommand has a `long_about` whose text contains `WORKLOG_ESTIMATOR_PROVIDER`.
- Commit `test(cli): RED — doctor estimator block + estimate long_about`.

**Green**
- Extend `cmd_doctor` JSON payload + table. Reachability probe uses the same `probe_litellm` helper (lifted to a small shared module if needed).
- Add `long_about` to `Cmd::Estimate { … }` explaining `WORKLOG_ESTIMATOR_PROVIDER`, `--model` override, fallback behaviour.
- `cmd_day`'s hard-coded `"running claude -p over unestimated blocks"` string becomes provider-aware.
- Commit `feat(cli): GREEN — doctor + long_about cover estimator`.

**Refactor**
- `clean-code`. If `probe_litellm` is shared between wizard and doctor, it lives in `worklog-core` (e.g. `core::estimate::probe_litellm`) so both callers pull the same impl.

## TDD Plan Validation Checklist

- [x] Each phase has exactly R / G / R commits
- [x] No phase writes implementation in Red, no phase writes tests in Green
- [x] Every behavior in the Behavior Inventory maps to at least one explicit test
- [x] No `toBeDefined()`-style green-bar tests — every test asserts a concrete value or error substring
- [x] `clean-code` pass scheduled in every Refactor
- [x] Inline gates come BEFORE quality agents, before runtime verification, before user verification, before QA, before PR
- [x] No phase exceeds 8 task rows
- [x] Every file touched is named in "Files touched"

## Inline gates (Stage 3 — after all phases done)

Single command:
```bash
cd rust && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```
If any fails: fix inline, re-run. No web changes expected — skip `cd web && …` unless a phase accidentally pulled one in.

## Quality pipeline (Stage 4 — 4 parallel sonnet agents)

1. **Security Scanner** — focus: secret handling for `litellm_api_key` (must not land in logs / error messages / wizard echo); HTTP request TLS behaviour; no credentials in request bodies; prompt-injection risk in `system`/`user` strings concatenated from events.
2. **Performance Analyzer** — focus: per-block HTTP roundtrip; no accidental sync-on-async; no regression in the sequential `for block in blocks` loop.
3. **Accessibility Auditor** — N/A (no UI). Agent reports "no UI touched" verdict PASS.
4. **Type-Safety Checker** — focus: `Box<dyn ModelInvoker>` vs enum choice; Result chains; feature doesn't widen `anyhow::Error` where a typed error would aid callers.

Gate: all four VERDICT=PASS before Stage 5.

## Runtime verification (Stage 5 — NON-SKIPPABLE)

Non-UI feature → runtime probe, NOT browser verification.

**Setup**
```bash
# Option A: docker
docker run -d --name litellm-test -p 4000:4000 \
  -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  ghcr.io/berriai/litellm-database:main-stable \
  --model anthropic/claude-haiku-4-5

# Option B: pipx / uv
pipx run litellm --model anthropic/claude-haiku-4-5 --port 4000
```

**Runtime verification steps**

| # | Command | Expect |
|---|---|---|
| R1 | `worklog secret set litellm_base_url http://localhost:4000`<br>`worklog secret set litellm_model anthropic/claude-haiku-4-5`<br>`worklog secret set worklog_estimator_provider litellm` | succeeds |
| R2 | `worklog doctor --json` | JSON has `"estimator": {"provider": "litellm", "reachable": true, …}` |
| R3 | Seed one un-estimated block in a test DB. `worklog estimate --day <today>` | Exit 0; stats show `estimated >= 1`; block row has `estimated_by='claude_p'` (column value unchanged in this feature) |
| R4 | Stop docker. `worklog estimate --day <today>` on a fresh block | Exit 0 with `failed >= 1`; block marked `gap`; stderr has a helpful HTTP error |
| R5 | `WORKLOG_ESTIMATOR_PROVIDER=claude_subprocess worklog estimate --day <today>` | Falls back to `ClaudeSubprocess` (requires `claude` binary available) |
| R6 | `worklog secret set litellm_api_key invalid` + restart proxy with auth | `worklog estimate` → block `gap`, error mentions HTTP 401 |
| R7 | `worklog setup` interactive | Shows the new "estimator provider" step with two choices; picking litellm prompts the 3 fields + runs the probe |

Results → `.claude/verification/runtime-results.md`.

## User verification (Stage 6 — HARD GATE)

Present to user:
- `worklog doctor` table with the new estimator block.
- Artefacts from R1–R7.
- Quality gate table.

Explicit acceptance question: **"Verify R1–R7 above. Respond 'approved' to proceed to QA + PR, or describe any issues."**

## QA pass (Stage 7)

Invoke `/qa` on `feature/litellm-estimator`. Gate: PASS or PASS WITH NOTES (fix any ship-blocker).

## PR (Stage 8)

Title: `feat(estimate): add LiteLLM provider alongside claude -p subprocess`

Body sections: Why, What, TDD evidence (R/G/R commits), Quality gate table, How to test (R1–R7), release-please footer (this is a `feat:` → v0.7.0 minor bump).

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| LiteLLM endpoint returns a provider-specific error shape we don't expect | `parse_response` already tolerates envelopes + prose wrapping; if we still can't parse, we bail → block marked `gap` with reason surfaced in warn log. No hard crash. |
| Prompt-injection via event content leading Claude to emit arbitrary JSON | Existing `validate_ticket` guard drops hallucinated keys. Injection via description field would land verbatim in Tempo — this is the current behaviour regardless of provider and out of scope here. |
| User accidentally POSTs their secrets to a rogue base_url | Wizard probe surfaces connection, but not auth correctness; `long_about` on `estimate` recommends local-only / trusted endpoints. |
| First-time-setup order: the new step runs AFTER daemon-service, so a user who cancels halfway has no estimator configured but still lands on a working default (`claude_subprocess`). | Intended — back-compat is the primary requirement. |
| Tests that mutate `std::env` bleed across parallel test runs | Serialize with a test-local `Mutex` (same pattern as `secrets.rs`). |

## Rollback

- Revert the whole branch → no data migration needed; all new secrets are optional.
- Users who already saved LiteLLM secrets are unaffected post-revert (secrets remain in their keychain as harmless orphans).
