---
type: feature
branch: feature/rust-hook-and-time-inference
description: Rust hook binary + claude-p based time inference + SessionStart/Stop pairing
size: medium
worktree: false
current_stage: stage-1.2-research
---

## Detected commands
TEST_CMD: `uv run pytest -q`
LINT_CMD: `uv run ruff check src`
TYPECHECK_CMD: `uv run mypy src`
FORMAT_CMD: `uv run ruff format`
RUST_TEST_CMD: `cargo test --manifest-path rust/hook/Cargo.toml`
RUST_LINT_CMD: `cargo clippy --manifest-path rust/hook/Cargo.toml -- -D warnings`

## Progress
- [x] Stage 0: branch created
- [ ] Stage 1.2: research swarm
- [ ] Stage 1.3: plan
- [ ] Stage 1.5: user approval HARD GATE
- [ ] Stage 2: TDD execution
- [ ] Stage 3: inline gates
- [ ] Stage 4: quality pipeline
- [ ] Stage 5: verification
- [ ] Stage 6: user verification HARD GATE
- [ ] Stage 7: QA pass
- [ ] Stage 8: merge summary
