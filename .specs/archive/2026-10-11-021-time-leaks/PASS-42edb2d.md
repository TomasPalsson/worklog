# PASS — 021 time leaks @ 42edb2d (after Phase 6 + 7 follow-up fixes)

All commands run from the repo root by the orchestrator on 2026-10-11 at 42edb2d.
`flow check` still finds no root project file, so G001 runs CLAUDE.md's gate commands
(same ruling as spec 017 and PASS-6271e1e).

| Gate | Command | Exit |
|------|---------|------|
| G001 Rust tests | `cargo test --manifest-path rust/Cargo.toml` (132 + 53 + 7 + 2068 + 2 passed) | 0 |
| G001 Rust lint | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| G001 Rust format | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| G001 Web tests | `cd web && bun test` (1596 pass) | 0 |
| G001 Web types | `cd web && bun run typecheck` | 0 |
| G001 Web build | `cd web && bun run build` (first run exit 1 with no error line; two re-runs exit 0, see NOTES) | 0 |
| G001 Mod tests | `claude plugin test mods/worklog` (150 pass) | 0 |
| G002 Branch review | `review-diff` over `5d765ac..e9531b9`, 6 lenses, receipts checked, blind re-score, keep ≥ 80: 0 kept; 6 sub-80 findings + 1 Seen fixed anyway in e4747a8 … 42edb2d (Ruling in NOTES.md) | 0 |
| G003 Evidence | `test -s .specs/021-time-leaks/verify/acceptance.md && test -s .specs/021-time-leaks/verify/CHK001.md && test -s .specs/021-time-leaks/verify/CHK002.md` | 0 |
