# Security review — feature/litellm-estimator

## Checked

- Secret leakage for `litellm_api_key` across `estimate.rs`, `wizard.rs`, `cli.rs`.
- HTTP request construction + TLS policy.
- Prompt injection surface (new vs pre-existing).
- Response handling and memory footprint.
- SSRF surface via user-provided `base_url`.
- Secrets-file backend shape.
- Test-environment mutation.

## Findings

| # | Surface | Severity | Disposition |
|---|---|---|---|
| 1 | `api_key` in tracing / println | None | CLEAN — `bearer_auth` only; never interpolated into errors. |
| 1b | Raw proxy response in `extract_message_content` error message | Low | NOTED — already bounded by the 500-char cap on normal error paths; the envelope-missing branch dumps full JSON but is rare. Monitor. |
| 2 | SSRF via unvalidated `base_url` in `LiteLLMInvoker::new` / `probe_litellm` | Medium | **FIX APPLIED** — reject non-http(s) schemes in the constructor. Blocking RFC1918 / link-local is punted: the user explicitly points the tool at their own proxy (often `localhost:4000`), so a hard block would break the happy path. |
| 3 | Prompt injection (new surface) | None | CLEAN — same event content was already fed to `claude -p`; LiteLLM adds no new injection vector. |
| 4 | Unbounded `.json()` response decode at `estimate.rs:357` | Low-Medium | **FIX APPLIED** — replaced with `.bytes()` → 1 MiB cap → `serde_json::from_slice`. Prevents OOM from a hostile proxy. |
| 5 | Probe SSRF — same root as #2 | Medium | **FIX APPLIED** via scheme check in `probe_litellm`. |
| 6 | Secrets file backend | None | CLEAN — three new keys use the existing path, no structural change. |
| 7 | Test env mutation / cross-test leakage | None | CLEAN — `PROVIDER_ENV_LOCK` + `WIZARD_LOCK` serialise; `clear_provider_state()` is symmetric. |

## Verdict

**PASS** — Two important findings (body-size cap + scheme validation) are applied in this branch. The low-severity "raw response in error message" finding is bounded in the common path by `bounded_body_preview`; the envelope-missing branch is rare enough to track as a follow-up rather than block merge. RFC1918/link-local blocking is intentionally NOT enforced because the user's most common configuration is `http://localhost:4000`.
