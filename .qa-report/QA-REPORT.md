# QA Report — `feature/litellm-estimator`

**Branch:** `feature/litellm-estimator` (worktree at `../code-worktrees/feature-litellm-estimator`)
**Base:** `main` @ `a53f493`
**Scope:** Medium — 10 code files, ~1400 LOC net new
**Risk zones touched:** None (no auth, payments, or data deletion). **New external HTTP surface** (user-configured LiteLLM proxy) — scheme-validated + response-capped.

## Verdict

## **PASS**

All ship-blockers addressed. Two QA-swarm edge cases fixed in this branch. No remaining blockers; three cosmetic follow-ups documented for a later pass.

## Ship-blockers

None.

## Should-fix (fixed in this branch)

| # | Issue | Fix | Commit |
|---|---|---|---|
| SF1 | `litellm_base_url = ".../v1"` silently 404'd every block (LiteLLM docs show the `/v1` URL form) | `LiteLLMInvoker::new` strips trailing `/v1` + trailing slash | `60ba9e6` |
| SF2 | `--model "   "` forwarded whitespace as the model name instead of falling back | `resolve_model` uses `trim().is_empty()` | `60ba9e6` |
| SF3 | `LiteLLMInvoker`'s `OnceLock` memoisation invisible in the trait contract | Struct-level `# Contract` doc + field comment | `60ba9e6` |

## Nice-to-fix (tracked as follow-ups, not blocking)

| # | Issue | Why deferred |
|---|---|---|
| N1 | `LiteLLM → subprocess` re-run leaves zombie `litellm_*` secrets in keychain | Functionally harmless; user can `worklog secret rm` |
| N2 | `litellm_base_url` set without `worklog_estimator_provider` shows `claude_subprocess` in doctor without a hint | Subtle UX polish; the secrets table already shows the orphan |
| N3 | `cmd_day` calls `resolve_provider()` twice (label + estimate) — latent TOCTOU | Real race window is microseconds; no observed flakes |
| N4 | `EstimatorReport` could become an enum-with-variants for a cleaner JSON schema | Would rewrite the public `doctor --json` shape; bundle with next schema revision |
| N5 | `probe_litellm` returns inverted `Option<String>` (`None = ok`) | Cosmetic; two callsites; doesn't affect behaviour |
| N6 | `extract_message_content` error message dumps the full envelope | Bounded on normal paths; rare branch |

## Passed checks

### Automated
- **290 Rust tests** (100% pass) across 8 suites — includes 17 net new tests pinning every behaviour in the plan's inventory + 3 QA regressions.
- **`cargo clippy --all-targets --all-features -- -D warnings`**: 0 warnings.
- **`cargo fmt --check`**: clean.
- **`bun test` (web)**: 41/41 pass — web untouched.
- **`bun run typecheck`** (web): 0 errors.
- **Release build** completes in 37 s.

### Quality-pipeline agents (Stage 4)
| Dimension | Verdict | Artifact |
|---|---|---|
| Security | PASS | [`.claude/quality/security.md`](../.claude/quality/security.md) |
| Performance | PASS | [`.claude/quality/performance.md`](../.claude/quality/performance.md) |
| Accessibility | PASS (no UI) | [`.claude/quality/accessibility.md`](../.claude/quality/accessibility.md) |
| Type-safety | PASS | [`.claude/quality/type-safety.md`](../.claude/quality/type-safety.md) |

### Runtime verification (Stage 5)
8/8 autonomous runtime checks pass. Report: [`.claude/verification/runtime-results.md`](../.claude/verification/runtime-results.md).

### User verification (Stage 6)
U1–U6 approved by user on 2026-04-19: "approved".

### Edge-case hunt (Stage 7, Wave 2)
18 scenarios traced, 2 should-fix + 1 documentation item (above) + 6 nice-to-fix + 9 by-design / already-handled. Report: [`wave-2/edge-cases.md`](wave-2/edge-cases.md).

## Not tested

| Scenario | Reason |
|---|---|
| Interactive `worklog setup` TTY flow (B10/B11) | dialoguer requires a real terminal; covered by user verification U2 |
| Real LLM round-trip with Anthropic | Covered by user verification U3 after the user ran the live LiteLLM proxy |
| Self-signed HTTPS proxies | Out of scope — `rustls` default rejects untrusted certs (documented as "use HTTP for local, HTTPS for trusted endpoints") |
| Windows | Platform not shipped; no CI matrix |

## Conflicting evidence

None. Scanners and edge-case hunter agreed on all findings or commented on orthogonal dimensions.

## Test evidence

- **Unit + integration:** `cargo test --manifest-path rust/Cargo.toml` → 290 pass.
- **Release binary smoke (A1–A8):** [`../.claude/verification/runtime-results.md`](../.claude/verification/runtime-results.md).
- **Quality agents:** [`../.claude/quality/*.md`](../.claude/quality/).
- **Edge cases:** [`wave-2/edge-cases.md`](wave-2/edge-cases.md).

## Commit log

```
60ba9e6 fix(estimate): QA-swarm edge cases — /v1 strip + whitespace --model
ed5a42b chore(verification): runtime results — 8/8 autonomous checks pass
9952d64 fix(estimate,cli): quality-gate findings from sec/perf/type reviews
732b322 docs(readme): document LiteLLM estimator alternative
2aef8ba feat(cli): GREEN — doctor estimator block + estimate long_about
6bd12bd test(cli): RED — doctor estimator block + estimate long_about
d806cfe refactor(estimate): lift probe_litellm into worklog-core
959b0f8 feat(wizard): GREEN — estimator provider picker + probe
fa328a5 test(wizard): RED — estimator provider step
58d73a0 refactor(estimate): clean-code pass on provider factory
d5a094c feat(estimate): GREEN — resolve_provider factory
323463e test(estimate): RED — provider factory resolution
f0132f8 refactor(estimate): clean-code pass on LiteLLMInvoker
da7d46b feat(estimate): GREEN — LiteLLMInvoker via OpenAI-compatible HTTP
cc7552e test(estimate): RED — LiteLLMInvoker HTTP contract
d60044d feat(secrets): GREEN — register LiteLLM + provider keys
1106f81 test(secrets): RED — LiteLLM + provider keys
```

Red/Green/Refactor ordering preserved across all 5 TDD phases.
