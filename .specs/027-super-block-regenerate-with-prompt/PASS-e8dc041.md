# PASS — 027 super block regenerate with prompt @ e8dc041

Gates run 2026-10-16 from the repo root with CLAUDE.md's own commands (`flow check` finds no root
project file in this monorepo). Exit = `<cmd> > log 2>&1; echo $?`, read from the log.
Rust gates ran at e8dc041; web gates ran at 073d075 (e8dc041 only reformats one Rust test file).

| Gate | Command | Exit | Executed |
|---|---|---|---|
| rust test | `cargo test --manifest-path rust/Cargo.toml` | 0 | 2522 |
| rust lint | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 | n/a |
| rust format | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 | n/a |
| web test | `cd web && bun test` | 0 | 2092 |
| web typecheck | `cd web && bun run typecheck` | 0 | n/a |
| web lint | `cd web && bun run lint` | 0 | n/a |
| web build | `cd web && bun run build` | 0 | n/a |
| gate-relaxation scan | `slop-check --base dc9a952 --no-tools` (no NS-09/NS-13; `[skip ci]` grep: 0 hits) | 0 | n/a |
| branch review | `flow:review-diff` dc9a952..c5a0329, findings triaged by hand (receipt-check misfired), 3 fixed in d4773b8 | 0 | n/a |
| acceptance | `verify/acceptance.md` — 16 FR rows exit 0, CHK001 human; `verify/claim-check.md` 14 confirmed, 3 inconclusive (FR-07/FR-08 rows re-pointed and re-run) | 0 | n/a |

Not checked: branch review covered dc9a952..c5a0329 only — the gate fix d4773b8, the test-only tasks T008/T009
(d8969ce, 073d075) and the rustfmt commit e8dc041 were re-gated but not re-reviewed by a fresh reviewer;
web lint keeps 3 pre-existing react-hooks warnings in ChangeNotices.tsx and TaskCard.tsx (files untouched).
