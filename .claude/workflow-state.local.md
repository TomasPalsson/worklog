---
type: feature
branch: feature/delete-python
description: Port remaining Python to Rust, delete the Python package, ship via signed curl installer
size: large
worktree: false
current_stage: stage-1.3-plan-draft
---

## Detected commands

RUST_TEST_CMD: `cargo test --manifest-path rust/Cargo.toml`
RUST_LINT_CMD: `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings`
RUST_FMT_CMD: `cargo fmt --manifest-path rust/Cargo.toml --all -- --check`
WEB_TEST_CMD: `cd web && bun test`
WEB_TYPECHECK_CMD: `cd web && bun run typecheck`
WEB_BUILD_CMD: `cd web && bun run build`
PY_TEST_CMD: `uv run pytest` (deleted in phase 5)

## Progress

- [x] Stage 0: setup + clarification gate
- [ ] Stage 1.3: plan approval HARD GATE
- [ ] Phase 1: Gcal collector in Rust (RED → GREEN → REFACTOR)
- [ ] Phase 2: `worklog day` in Rust
- [ ] Phase 3: GHA release + real signing key
- [ ] Phase 4: install.sh
- [ ] Phase 5: Delete Python
- [ ] Phase 6: Docs + CLAUDE.md rewrite
- [ ] Stage 3: inline gates
- [ ] Stage 4: quality pipeline (4 parallel sonnet agents)
- [ ] Stage 5: runtime verification
- [ ] Stage 6: user verification HARD GATE
- [ ] Stage 7: QA swarm
- [ ] Stage 8: PR (draft → ready)
