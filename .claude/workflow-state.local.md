---
type: feature
branch: feat/v0.4-daemon-polish-purge
worktree: null
size: medium
---

# Workflow State

## Metadata
- Type: feature
- Branch: feat/v0.4-daemon-polish-purge
- Worktree: none (in-place)
- Description: v0.4 bundle — daemon auto-install, CLI polish, rolling purge, richer hook capture
- Size: medium
- Max iterations: 50

## Detected commands
- TEST_CMD: `cargo test --manifest-path rust/Cargo.toml`
- LINT_CMD: `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings`
- FORMAT_CMD: `cargo fmt --manifest-path rust/Cargo.toml --all`
- TYPECHECK_CMD: (folded into `cargo test`)
- BUILD_CMD: `cargo build --manifest-path rust/Cargo.toml`

## Progress
- [x] Plan drafted → `.claude/feature-plan.local.md`
- [ ] Phase 1: hook prompt capture — RED / GREEN / REFACTOR
- [ ] Phase 2: data purge — RED / GREEN / REFACTOR
- [ ] Phase 3: daemon install — RED / GREEN / REFACTOR
- [ ] Phase 4: daemon auto-start + wizard — RED / GREEN / REFACTOR
- [ ] Phase 5: CLI polish — RED / GREEN / REFACTOR
- [ ] Phase 6: web dark mode — RED / GREEN / REFACTOR
- [ ] Phase 7: Inline gates
- [ ] Phase 8: Quality pipeline
- [ ] Phase 9: Runtime verification (CLI + browser)
- [ ] Phase 10: User verification (HARD GATE)
- [ ] Phase 11: QA pass
- [ ] Phase 12: PR creation (v0.4.0)

## Notes
- Skipping UI showcase gate (CLI feature, no UI changes to web/).
- Skipping browser verification — runtime verification in a terminal replaces it.
- User's memory: "Default to autonomous execution on worklog — ship agreed plans phase-by-phase without mid-execution permission checks". Once plan is approved, blast through phases.
