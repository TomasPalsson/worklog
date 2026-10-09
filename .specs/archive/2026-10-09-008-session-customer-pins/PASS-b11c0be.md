# PASS — b11c0be

Feature: 008-session-customer-pins · Base: ce1306f (after syncing onto origin/main 0949211) · Run: 2026-09-28

Code under `rust/` and `web/` is byte-identical to the previous pass (b913c80, before the rebase);
all gates were re-run on this tree anyway.

## G001 — project gates (`flow check --fix` can't find the Rust crate at repo root; ran the project's own gates from CLAUDE.md)
| Command | Exit |
|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| `cargo test --manifest-path rust/Cargo.toml` (1150 tests: 1074 core lib, 43 cli, 24 cli lib, 7 daemon, 2 env; 0 failed) | 0 |
| `bash scripts/verify-inference.sh` (real-data copy, all 9 assertions) | 0 |

## G002 — branch review
Five lenses (correctness, gaming, security, cross-file, slop) over the branch code diff,
blind 0–100 re-score, ≥80 kept. Kept and fixed: pinned origin never firing on real sessions (100),
stale pins never purged (80), purge without cutoff (88), block/lane disagreement (96, fixed over
three rounds), per-block day reload (measured), agreeing prompt blocking "pinned" (found live).
Dropped findings are recorded as `Ruling:` lines in NOTES.md.

## G003 — verification evidence
`verify/`: CHK001.md (+ day view, block detail, re-run day view screenshots, hint texts, re-run
script), NFR-measurements.txt (session-hint p95 16 ms ≤ 300 ms; start text 540/599 chars ≤ 600).

## G004 — web gates
| Command | Exit |
|---|---|
| `cd web && bun test` (324 pass) | 0 |
| `cd web && bun run typecheck` | 0 |
| `cd web && bun run build` | 0 |
