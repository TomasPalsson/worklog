# Type-safety review — feature/litellm-estimator

## Scores

| Dimension | Score |
|---|---|
| Encapsulation | 4 / 5 |
| Invariant expression | 3 / 5 |
| Invariant usefulness | 4 / 5 |
| Invariant enforcement | 4 / 5 |

## Findings

**CLEAN:**
- `ProviderChoice` enum over `Box<dyn ModelInvoker>` is the right call — compiler-proven exhaustive dispatch at every callsite (`estimate_day`, `estimator_report`).
- Hand-written `Debug` impl redacts `base_url` / `api_key` from tracing + panic messages.
- `api_key` empty-string invariant ("empty means omit `Authorization`") is enforced at a single co-located guard.
- `resolve_provider` returns `Result<ProviderChoice>` with actionable error messages for both missing `base_url` and unknown provider strings.
- No `.unwrap()` / `.expect()` in production paths touching `LiteLLMInvoker` or `ProviderChoice`.
- `read_trimmed_secret` is the right abstraction — enforces a consistent "blank equals absent" rule across keyring + `.env` backends.

**OPEN (NOTED as follow-ups — not blocking):**

| # | Issue | Disposition |
|---|---|---|
| T1 | `EstimatorReport` uses `provider: &'static str` + `Option<>` fields rather than an enum-with-variants | **NOTED** — restructuring would rewrite the `worklog doctor --json` schema (a public surface via the `--json` contract). Documented for a future schema revision. |
| T2 | `probe_litellm` returns `Option<String>` with inverted-None convention (`None = ok`) | **NOTED** — cosmetic; would churn two callsites. Document for future cleanup. |
| T3 | Test at `resolve_provider_uses_default_litellm_model_when_model_secret_missing` calls private `inv.resolve_model("")` | **FIX APPLIED** — rewritten to use public `configured_model()`. |
| T4 | `provider_label` match in `cmd_day` groups `Err` with `ClaudeSubprocess` under one label | **WON'T FIX** — display-only. `cmd_doctor` is the surface that differentiates; `cmd_day` just labels the spinner. |

## Verdict

**PASS** — Core type design is sound: enum exhaustiveness, construction-time invariant enforcement, actionable error paths, no sloppy unwraps. One hidden-to-public-method fix applied; the two cosmetic follow-ups are documented for a later pass and don't affect correctness.
