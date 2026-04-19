---
type: feature
branch: feat/v0.5-events-submenu-polish
worktree: null
size: large
---

# Workflow State

## Metadata
- Type: feature
- Branch: feat/v0.5-events-submenu-polish
- Worktree: none (in-place)
- Description: v0.6 — events submenu + ticket-flow fix + design polish + upgrade daemon-restart
- Size: large
- Max iterations: 100

## Detected commands
- TEST_CMD: `cargo test --manifest-path rust/Cargo.toml` + `cd web && bun test`
- LINT_CMD: `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` + `cd web && bun run lint`
- FORMAT_CMD: `cargo fmt --manifest-path rust/Cargo.toml --all`
- TYPECHECK_CMD: `cd web && bun run typecheck`
- BUILD_CMD: `cd web && bun run build`

## Progress
- [x] Plan drafted → `.claude/feature-plan.local.md`
- [ ] Phase 1: daemon read endpoints — RED / GREEN / REFACTOR
- [ ] Phase 2: web reader migration — RED / GREEN / REFACTOR
- [ ] Phase 3: events submenu UI — RED / GREEN / REFACTOR
- [ ] Phase 4: design polish — RED / GREEN / REFACTOR (includes /design evaluator subagent)
- [ ] Phase 5: upgrade daemon-restart — RED / GREEN / REFACTOR
- [ ] Phase 6: Inline gates
- [ ] Phase 7: Quality pipeline
- [ ] Phase 8: Browser verification (Claude-in-Chrome)
- [ ] Phase 9: User verification (HARD GATE)
- [ ] Phase 10: QA pass
- [ ] Phase 11: PR + release (target v0.6.0)
