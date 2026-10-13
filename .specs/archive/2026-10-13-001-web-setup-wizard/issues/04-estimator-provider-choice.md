# 04 — Owner can choose the estimator provider (subprocess `claude -p` or LiteLLM)

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner picks LiteLLM, enters base-url/key/model, and (optionally) the `/health` probe runs with a save-anyway fallback; picking subprocess instead resets the estimator to default by clearing only `worklog_estimator_provider`.

## Tag rationale
- **AFK** → No human decision required. Provider options, the three LiteLLM keys, the default model, and the optional `/health` probe behaviour are all defined (FR-006 + §4.1.1). Reuses slice 03's credential save path.

## Depends on
- Slice 03 — Owner can enter, live-validate, and save any credential (LiteLLM keys reuse the credential save path; provider persists via a secret key)

## Spec trace
- Journey: Journey 1 (happy path step 5, estimator)
- Requirements: FR-006, §4.1.1 (`litellm_base_url` / `litellm_api_key` / `litellm_model` / `worklog_estimator_provider` rows), Non-goal "resetting the estimator to the subprocess default clears only `worklog_estimator_provider`", NFR-E2
- Seam: estimator config — provider toggle persisted via existing secret key; LiteLLM trio captured through slice 03's save path with an optional `/health` probe.

## Acceptance criteria  (verbatim from the spec — do not invent)
- [ ] **FR-006 (subprocess)** — Given the estimator step, When the Owner selects subprocess `claude -p`, Then the persisted provider override (`worklog_estimator_provider`) is cleared.
- [ ] **FR-006 (LiteLLM)** — Given the estimator step, When the Owner selects LiteLLM, Then `litellm_base_url` / `litellm_api_key` / `litellm_model` are captured (model defaults to `DEFAULT_LITELLM_MODEL`) and the provider is set to litellm.
- [ ] **§4.1.1 + NFR-E2** — Given LiteLLM is chosen, When the optional `GET <base_url>/health` probe fails, Then the page warns with "save anyway?" rather than hard-blocking.

## Out of scope
- General credential capture/validation mechanics (slice 03).
