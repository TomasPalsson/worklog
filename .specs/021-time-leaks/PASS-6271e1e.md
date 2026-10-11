# PASS — 021 time leaks @ 6271e1e

All commands run from the repo root by the orchestrator on 2026-10-11. `flow check --since 5d765ac`
exits 1 with "No supported project file found" (Cargo.toml and package.json live in rust/ and web/),
so G001 runs CLAUDE.md's gate commands instead — the same ruling as spec 017 (see NOTES.md).
Rust gates ran at 7571274; the only later commit (6271e1e) touches web/ only, and the web gates
were re-run after it.

| Gate | Command | Exit |
|------|---------|------|
| G001 Rust tests | `cargo test --manifest-path rust/Cargo.toml` (110 + 53 + 7 + 2029 + 2 passed) | 0 |
| G001 Rust lint | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| G001 Rust format | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| G001 Web tests | `cd web && bun test` (1581 pass) | 0 |
| G001 Web types | `cd web && bun run typecheck` | 0 |
| G001 Web build | `cd web && bun run build` | 0 |
| G001 Mod tests | `claude plugin test mods/worklog` (147 pass) | 0 |
| G002 Branch review | `review-diff` workflow over `review-package 5d765ac HEAD`, 6 lenses, blind re-score, keep ≥ 80: 0 kept; two sub-80 findings fixed anyway in 6271e1e (Ruling in NOTES.md) | 0 |
| G003 Evidence | `test -s .specs/021-time-leaks/verify/acceptance.md && test -s .specs/021-time-leaks/verify/CHK001.md && test -s .specs/021-time-leaks/verify/CHK002.md` | 0 |
