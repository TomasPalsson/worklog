# Edge Cases — Wave 2

## 1. Goldilocks: `litellm_base_url = "http://"` (scheme present, no host)

**Scenario:** User saves `http://` as the base URL via `worklog secret set litellm_base_url http://`.

**Current behaviour:** `LiteLLMInvoker::new` (`estimate.rs:321`) trims the trailing slash, leaving `"http://"`. The scheme check at `estimate.rs:322` passes because it starts with `"http://"`. Construction succeeds. `endpoint()` returns `"http:///v1/chat/completions"`. The HTTP client will fail with a connect error at invoke time, which is caught and converted to a `gap` block — no panic, no data loss. The wizard's `probe_litellm` (`estimate.rs:154`) calls `format!("{}/health", "http://")` → `"http:///health"`, which also fails gracefully.

**Severity:** nice-to-fix

**Suggested fix:** Add a host-presence check in `LiteLLMInvoker::new`: reject URLs where `reqwest::Url::parse(&base_url)?.host().is_none()`. Same check in `probe_litellm`.

---

## 2. Goldilocks: `litellm_base_url` includes `/v1` path prefix

**Scenario:** User sets `litellm_base_url = "http://localhost:4000/v1"`.

**Current behaviour:** `LiteLLMInvoker::new` trims the trailing slash (none here). `endpoint()` returns `"http://localhost:4000/v1/v1/chat/completions"` (`estimate.rs:350`). Every inference call hits the doubled path. LiteLLM returns 404; `estimate.rs:388-393` bails with `"HTTP 404 from LiteLLM proxy"`. Every block becomes `gap`. Silent failure — no warning that the URL was misconfigured.

**Severity:** should-fix

**Suggested fix:** In `LiteLLMInvoker::new`, strip any trailing `/v1` (and `/v1/`) from `base_url` before storing it, with a `warn!` that the path was normalised.

---

## 3. Goldilocks: `litellm_model = "   "` (all whitespace)

**Scenario:** User sets `litellm_model` to whitespace only.

**Current behaviour:** `read_trimmed_secret("litellm_model")` (`estimate.rs:123-124`) trims and filters empty — the whitespace-only value becomes `None`, so `DEFAULT_LITELLM_MODEL` is used. Correct.

**Severity:** by-design (already handled)

---

## 4. Goldilocks: `--model "   "` (whitespace-only caller model)

**Scenario:** Caller invokes `worklog estimate --model "   "`.

**Current behaviour:** clap receives `"   "` as the `model` argument (not the default, since the user provided a value). This is passed as-is to `estimate_day` → `estimate_day_with` → `invoker.invoke`. For `LiteLLMInvoker`, `resolve_model("   ")` (`estimate.rs:362-368`) checks `caller.is_empty()` — `"   ".is_empty()` is `false` — so `"   "` is used as the model name. The request body contains `"model": "   "`. LiteLLM will return a 400 or 404; the block becomes `gap`.

For `ClaudeSubprocess`, `"   "` is passed verbatim as `--model   `, which claude will likely reject with a non-zero exit, again becoming `gap`.

**Severity:** should-fix

**Suggested fix:** In `resolve_model`, change `if caller.is_empty()` to `if caller.trim().is_empty()` (`estimate.rs:363`).

---

## 5. Casing / whitespace on `WORKLOG_ESTIMATOR_PROVIDER`

**Scenario:** `WORKLOG_ESTIMATOR_PROVIDER=LITELLM`, `= LiteLLM `, `=litellm\n`.

**Current behaviour:** `read_provider_selector` (`estimate.rs:101-108`): trims whitespace, then calls `to_lowercase()`. `"LITELLM"` → `"litellm"`, `" LiteLLM "` → `"litellm"`, `"litellm\n"` → `"litellm"` (newline stripped by `trim()`). All map correctly to `Some("litellm")` and route to `build_litellm_from_secrets`. Fully handled.

**Severity:** by-design (already handled)

---

## 6. Response envelope: `{"choices": []}` (empty array)

**Scenario:** Proxy returns a well-formed 200 with an empty `choices` array.

**Current behaviour:** `extract_message_content` (`estimate.rs:460-467`) calls `.pointer("/choices/0/message/content")` — with an empty array, `/choices/0` returns `None`. `and_then(Value::as_str)` short-circuits. `ok_or_else` fires with `"LiteLLM response missing choices[0].message.content"`. `invoke` returns `Err`, the block becomes `gap`. Correct — no panic.

**Severity:** by-design (already handled)

---

## 7. Response envelope: `content` is `null`, integer, or empty string

**Scenario:** Proxy returns `{"choices":[{"message":{"content":null}}]}`, or `content: 42`, or `content: ""`.

**Current behaviour (`null`, `42`):** `.pointer("/choices/0/message/content")` returns `Some(Value::Null)` / `Some(Value::Number)`. `Value::as_str()` returns `None` for both. `ok_or_else` fires → `Err` → `gap`. No panic.

**Current behaviour (`""`):** `as_str()` returns `Some("")`. `extract_message_content` returns `Ok("")`. `parse_response("")` (`estimate.rs:747`): `raw.trim()` is `""`. `serde_json::from_str::<Value>("")` fails. The regex `(?s)\{.*\}` finds no match. Falls through to `anyhow::bail!("no JSON object in response")` → `Err` → `gap`. No panic.

**Severity:** by-design (already handled)

---

## 8. `OnceLock` across calls with different `system`/`schema`

**Scenario:** Two separate `LiteLLMInvoker::invoke` calls on the same instance pass different `system` or `schema` arguments.

**Current behaviour:** `system_with_schema: OnceLock<String>` (`estimate.rs:295`) is initialised on the first call and reused on every subsequent call (`estimate.rs:427-432`). If a caller passes a different `system` or `schema` in a later call to the same invoker, the cached (first-call) value silently wins.

Within a single `estimate_day` run this is not a bug — `SYSTEM_PROMPT` and `response_schema()` are constants (`estimate.rs:497`). The concern arises only if a caller reuses the same `LiteLLMInvoker` for a structurally different task (e.g. a future second estimator type or a test that shares an invoker across subtly different invocations).

The current call site creates the invoker fresh in `resolve_provider` → `estimate_day` per process, so no cross-run state pollution occurs. However the trait `ModelInvoker::invoke` signature documents `system`, `schema` as per-call inputs — the memoisation is an invisible contract violation if the invoker is ever reused outside its current scope.

**Severity:** should-fix (latent correctness hazard as API surface grows)

**Suggested fix:** Document the memoisation constraint as a `# Panics` / `# Contract` doc comment on `LiteLLMInvoker` and on `build_request_body`: "system and schema must be identical across all calls to the same instance." Alternatively, assert in `build_request_body` that `get_or_init` returned the same value as what would be produced from the new inputs (debug-only).

---

## 9. State pollution: LiteLLM → subprocess re-run

**Scenario:** User runs `worklog setup`, picks LiteLLM (saves `worklog_estimator_provider=litellm`), then re-runs setup and picks subprocess (item 0).

**Current behaviour:** `configure_estimator_provider` arm `0` (`wizard.rs:261-270`) calls `secrets::delete("worklog_estimator_provider")`. The `litellm_base_url`, `litellm_api_key`, and `litellm_model` secrets are **not** deleted. They remain in the keychain as zombie state. On the next `worklog estimate`, `resolve_provider` reads no provider secret and defaults to `ClaudeSubprocess` — correct. The LiteLLM secrets are inert.

**Severity:** nice-to-fix (zombie secrets in keychain, harmless functionally)

**Suggested fix:** In arm `0` of `configure_estimator_provider`, also call `secrets::delete` for `litellm_base_url`, `litellm_api_key`, `litellm_model` — or add a note to the wizard's output that those secrets remain and can be cleared with `worklog secret rm`.

---

## 10. State pollution: LiteLLM URL set, provider secret absent

**Scenario:** User runs `worklog secret set litellm_base_url http://localhost:4000` but never sets `WORKLOG_ESTIMATOR_PROVIDER` or `worklog_estimator_provider`.

**Current behaviour:** `read_provider_selector` returns `None` (env unset, secret unset). `resolve_provider` matches `None` → `ProviderChoice::ClaudeSubprocess` (`estimate.rs:166`). LiteLLM is not used. Correct back-compat behaviour. The orphan `litellm_base_url` secret is inert.

`worklog doctor` will show `litellm_base_url: set` in the secrets table but `estimator: claude_subprocess` — potentially confusing but not incorrect.

**Severity:** nice-to-fix

**Suggested fix:** In `estimator_report`, if `provider == "claude_subprocess"` and `litellm_base_url` is set in the keychain, add a `reason` hint: "`litellm_base_url` is set but `worklog_estimator_provider` is not — set it to `litellm` to activate the proxy."

---

## 11. Env vs. keychain priority undocumented for conflicting values

**Scenario:** Shell has `WORKLOG_ESTIMATOR_PROVIDER=litellm` but keychain has `worklog_estimator_provider=claude_subprocess`.

**Current behaviour:** `read_provider_selector` (`estimate.rs:101-108`) returns the env value immediately if non-empty, without consulting the keychain. So env wins. This is correct and the code comment says "Env wins over secret" (`estimate.rs:98-99`). However the `--help` text for `Estimate` (`cli.rs:148-150`) says "The env var wins" — it is documented.

**Severity:** by-design

---

## 12. `doctor` with `WORKLOG_ESTIMATOR_PROVIDER=litellm` but no URL

**Scenario:** Env set to `litellm`, `litellm_base_url` secret absent.

**Current behaviour:** `estimator_report(probe)` calls `est::resolve_provider()` → `build_litellm_from_secrets()` → `Err("estimator provider litellm selected, but litellm_base_url is not set…")`. The `Err(e)` arm (`cli.rs:655-665`) sets `provider: "unconfigured"`, `reason: Some(format!("{e:#}"))`. Human output hits the `None if estimator.provider == "unconfigured"` branch (`cli.rs:728-730`) and prints the error string. Correct and actionable.

**Severity:** by-design

---

## 13. `doctor` with unknown provider string

**Scenario:** `WORKLOG_ESTIMATOR_PROVIDER=banana`.

**Current behaviour:** `resolve_provider` bails with `"unknown estimator provider banana"` (`estimate.rs:170-175`). `estimator_report` catches it as `Err` → `provider: "unconfigured"` with `reason`. Correct.

**Severity:** by-design

---

## 14. Back-compat: existing install with only `anthropic_api_key`

**Scenario:** User has an old install. `anthropic_api_key` is set; no LiteLLM secrets, no provider env.

**Current behaviour:** `read_provider_selector` → `None`. `resolve_provider` → `ProviderChoice::ClaudeSubprocess`. `estimate_day_with` spawns `claude -p --model <model>`. The `anthropic_api_key` is consumed by the `claude` binary directly (not by worklog at all). Unchanged from pre-feature behaviour. Full back-compat.

**Severity:** by-design (confirmed safe)

---

## 15. `doctor` base_url recovery from `endpoint()`

**Scenario:** `estimator_report` reconstructs `base_url` from `inv.endpoint()` by stripping the suffix `/v1/chat/completions` (`cli.rs:635`). If user stored a URL ending in `/v1` (finding #2), `endpoint()` returns `…/v1/v1/chat/completions`. The strip removes `/v1/chat/completions`, leaving `…/v1`. `probe_litellm` is then called with `…/v1` and probes `…/v1/health` — a path the real proxy probably doesn't serve, giving a false "unreachable" even when the proxy is up.

**Severity:** should-fix (same root cause as finding #2; fix #2 and this disappears)

---

## 16. Wizard: `capture_litellm_config` saves untrimmed model

**Scenario:** User types model with leading/trailing whitespace in the `litellm_model` prompt.

**Current behaviour:** `secrets::set("litellm_model", model.trim())` (`wizard.rs:334`) — `model` is the raw dialoguer return value, which is trimmed before storing. Correct.

**Severity:** by-design (already handled)

---

## 17. Wizard: Cancel mid-flow (Ctrl+C during `Password` prompt)

**Scenario:** User starts the LiteLLM config step, fills in the URL, then Ctrl+C on the `api_key` prompt.

**Current behaviour:** `Password::interact()` returns `Err` with `io::ErrorKind::Interrupted`. `capture_litellm_config` propagates the error up through `configure_estimator_provider` → `wizard::run` → `cmd_setup`. No secrets have been written yet (the three `secrets::set` calls are after the probe at `wizard.rs:332-334`). State is consistent: no partial secrets written.

**Severity:** by-design (already handled — writes are atomic post-validation)

---

## 18. `cmd_day` provider_label double-resolves provider

**Scenario:** `cmd_day` calls `estimate::resolve_provider()` once for the label at `cli.rs:1319-1322` and then `estimate::estimate_day` calls it again internally at `estimate.rs:181`. If the environment changes between the two calls (unlikely but possible in tests), the displayed label could disagree with the actual provider used.

**Current behaviour:** Two independent calls to `resolve_provider` — the label could theoretically display `"litellm"` while `estimate_day` routes through `ClaudeSubprocess` if a secret disappears between the calls. In production this is vanishingly unlikely. In automated tests it is the kind of TOCTOU that causes flaky results.

**Severity:** nice-to-fix

**Suggested fix:** Have `estimate_day` return the resolved `ProviderChoice` alongside `EstimateStats`, or expose `resolve_provider` as a result `cmd_day` passes down. Alternatively, resolve once and thread the value.

---

## Verdict

**PASS WITH NOTES**

No ship-blockers. Two `should-fix` issues exist:

- **Finding #2** (`/v1` double-path): Silent all-blocks-gap failure when user includes `/v1` in the URL. Easy to detect and strip at construction.
- **Finding #4** (whitespace `--model`): `resolve_model` should use `trim().is_empty()` instead of `is_empty()`.
- **Finding #8** (`OnceLock` contract): Latent correctness hazard if the invoker is ever reused across heterogeneous calls; should be documented or asserted.
- **Finding #15** (`doctor` base_url recovery): Dependent on #2; fixing #2 resolves this.

Remaining findings are either by-design, already handled, or nice-to-fix quality items.
