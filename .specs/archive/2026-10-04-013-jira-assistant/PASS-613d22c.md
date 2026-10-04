# Gates — 013 Jira assistant @ 613d22c (2026-10-04)

| Gate | Command | Exit | Result |
|---|---|---|---|
| fmt | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 | clean |
| clippy | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 | clean |
| rust tests | `cargo test --manifest-path rust/Cargo.toml` | 0 | 1465 passed, 0 failed |
| web tests | `cd web && bun test` | 1 | 787 pass, 1 fail — `TaskWorkLog` "log time > submits, refetches and closes"; fails identically on base e85e7da (weekend-date test bug, not this branch) |
| web typecheck | `cd web && bun run typecheck` | 0 | clean |
| web build | `cd web && bun run build` | 0 | built (no dev server running) |
| branch review | `flow:review-diff` e85e7da..24e4b2c, lenses correctness/security/spec, blind re-score ≥80 | — | 0 of 12 kept; the 72 fixed in 613d22c |

`flow check --fix` exits with "No supported project file found" (Cargo.toml lives in rust/, package.json in web/), so the project's own commands above were run instead.
