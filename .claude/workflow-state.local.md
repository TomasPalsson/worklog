---
type: feature
branch: feature/litellm-estimator
worktree: /Users/tomas/Desktop/Projects/code-worktrees/feature-litellm-estimator
size: medium
---

# Workflow State

## Metadata
- Type: feature
- Branch: `feature/litellm-estimator`
- Worktree: `/Users/tomas/Desktop/Projects/code-worktrees/feature-litellm-estimator`
- Description: Add LiteLLM-proxy support as a new estimator provider alongside the existing `claude -p` subprocess. BYO endpoint (Q1=A), coexist with claude -p (Q2=B), Anthropic first-class in wizard (Q3).
- Size: medium (4–8 files touched)
- Max iterations: 50

## Detected commands
- TEST_CMD: `cargo test --manifest-path rust/Cargo.toml` (+ `cd web && bun test` — no web changes expected)
- LINT_CMD: `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings`
- FORMAT_CMD: `cargo fmt --manifest-path rust/Cargo.toml --all`
- TYPECHECK_CMD: `cd web && bun run typecheck` (no web changes expected but run anyway)
- BUILD_CMD: `cd web && bun run build` (skip unless web touched)

## Progress
- [x] Plan drafted → `.claude/feature-plan.local.md`
- [ ] Plan approved by user (HARD GATE)
- [ ] Phase 1: secrets + env plumbing (R/G/R)
- [ ] Phase 2: LiteLLMInvoker (R/G/R)
- [ ] Phase 3: provider factory + resolve_invoker (R/G/R)
- [ ] Phase 4: wizard estimator step (R/G/R)
- [ ] Phase 5: CLI doctor + help text (R/G/R)
- [ ] Stage 3: Inline gates (cargo fmt/clippy/test)
- [ ] Stage 4: Quality pipeline (4 parallel agents)
- [ ] Stage 5: Runtime verification (LiteLLM docker live probe)
- [ ] Stage 6: User verification (HARD GATE)
- [ ] Stage 7: QA pass
- [ ] Stage 8: PR creation
