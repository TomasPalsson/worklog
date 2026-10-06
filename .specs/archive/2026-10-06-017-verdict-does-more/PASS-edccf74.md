# PASS — 017 Verdict does more @ edccf74 (after merging origin/main: #110 session-ticket pick, #111)

All commands run from the repo root at edccf74 by the orchestrator. `flow check` cannot find a root project file in this repo (Cargo.toml/package.json live in rust/ and web/), so G001 runs CLAUDE.md's gate commands (Ruling in NOTES.md). Each command below was run separately at edccf74. The full `cargo test` exits 101 on exactly one test, the wall-clock assertion `billing_tenant_tests::rows_for_day_with_deildir_under_200ms` (266 ms vs a 200 ms budget under full-suite parallel load in a debug build). It times `billing::rows_for_day`, which this spec does not touch (no 017 commit edits billing*.rs); it passes alone, and everything else passes. Ruling in NOTES.md. `mods/worklog` bun tests need the host-only `claude-code/testing` module and mods/ is byte-identical to origin/main, so they are out of this gate.

| Gate | Command | Exit |
|---|---|---|
| G001 Rust tests (full, as-is) | `cargo test --manifest-path rust/Cargo.toml` | 101 |
| G001 Rust tests minus the timing flake | `cargo test --manifest-path rust/Cargo.toml -- --skip rows_for_day_with_deildir_under_200ms` | 0 |
| G001 the timing flake alone | `cargo test --manifest-path rust/Cargo.toml -p worklog-core rows_for_day_with_deildir_under_200ms` | 0 |
| G001 Rust lint | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| G001 Rust format | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| G001 Web tests | `cd web && bun test` | 0 |
| G001 Web types | `cd web && bun run typecheck` | 0 |
| G001 Web build | `cd web && bun run build` | 0 |
| G001 Verdict server self-test (real tokenizer) | `uv run --offline --with tokenizers python3 rust/crates/worklog-core/templates/verdict_server.py --self-test` | 0 |
| G002 Branch review | `Workflow flow:review-diff base=6eb7a92` (5 lenses, blind re-score, keep ≥ 80): 2 kept findings (same defect: Verdict ticket pick not wired) → fixed in T016 (bf3606a), verified by `cargo test -p worklog-core estimate` | 0 |
| G003 Verification evidence | `test -s .specs/017-verdict-does-more/verify/acceptance.md && test -s .specs/017-verdict-does-more/verify/CHK001.md && test -s .specs/017-verdict-does-more/verify/CHK002.md` | 0 |

Acceptance: `verify/acceptance.md` — 62 rows, 0 unmet (8 human rows point at CHK001/CHK002 or post-launch metrics).
Design: /design:design evaluator converged at C (3.35 → 3.63 → 3.50); fixes applied in T014/T015/T016; see NOTES.md Ruling.
