# Performance review — feature/litellm-estimator

## Checked

- Per-call allocation cost in `LiteLLMInvoker::build_request_body`.
- HTTP client reuse across invocations.
- Response decoding memory footprint.
- Sequential block loop: LiteLLM vs subprocess per-block.
- Doctor probe cost on every `worklog doctor` invocation.
- Wizard probe cost.
- Test suite wall-clock.

## Findings

| # | Issue | Severity | Disposition |
|---|---|---|---|
| 1 | `build_request_body` re-serialises the static schema + `format!`s `system_with_schema` on every block | Important | **FIX APPLIED** — compute `system_with_schema` once in `LiteLLMInvoker::new` and store as `String` on the struct. `invoke` now borrows `&self.system_with_schema` instead of rebuilding it per block. |
| 2 | HTTP client reuse | CLEAN | `Client` is held on the struct and reused across `invoke` calls. |
| 3 | `resp.json()` unbounded | — | Addressed in security.md finding #4 (same fix caps memory). |
| 4 | Sequential block loop: LiteLLM vs subprocess per-block | CLEAN | LiteLLM is strictly no worse — eliminates ~10ms process spawn overhead and the poll loop. Both paths are dominated by LLM model latency. |
| 5 | `worklog doctor` makes unconditional 3s-timeout HTTP GET when LiteLLM is active | Important | **FIX APPLIED** — probe is now gated behind `--probe` flag / `WORKLOG_DOCTOR_PROBE=1`. Default: report `reachable: null`. Prevents 3s UX regression for scripted callers. |
| 6 | Wizard probe | CLEAN | One-shot interactive call — not in any loop. |
| 7 | Test perf — `litellm_invoker_bails_on_timeout` | CLEAN | 200ms client timeout vs 500ms httpmock delay finishes deterministically fast. |

## Verdict

**PASS** — Two important findings (per-block schema hoist, doctor probe gating) are applied in this branch. No correctness bugs, no memory hazards after fixes, no regression vs. the subprocess path.
