# PASS — 023 topic atlas @ 0a22a6a

Base 9f27bd4. Last code commit 45a5eb6 (T043); 0a22a6a touches only this feature directory.

| Gate | Command | Exit |
|------|---------|------|
| G001 fmt | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| G001 clippy | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| G001 rust tests | `cargo test --manifest-path rust/Cargo.toml` (2086 + 78 + 53 + 7 + 2 passed, 0 failed, 1 ignored bench) | 0 |
| G001 web tests | `cd web && bun test` (1766 pass, 0 fail) | 0 |
| G001 build | `cd web && bun run build` | 0 |
| G001 typecheck | `cd web && bun run typecheck` | 0 |
| G002 branch review | `flow:review-diff` wf_b3c64452-e95 over 9f27bd4..HEAD: 1 verified (≥80) + 1 advisory fixed in T039; follow-up adversary over 00bbb3f..393af30 → percentages fixed in T042, namer limit in T043 | 0 |
| G003 evidence | `test -s .specs/023-topic-atlas/verify/acceptance.md && test -s .specs/023-topic-atlas/verify/CHK001.md && test -s .specs/023-topic-atlas/verify/claim-check.md` | 0 |
