# PASS — 021 time leaks @ fb7801b (origin/main merged in)

origin/main (#157, #159, #160) merged into the branch. One conflict, in `web/app/[day]/page.tsx`:
main added a `noiseHosts(day)` fetch inside the old swallow-all block that this branch replaced with
`sectionLoader` (FR-13). Resolved by loading it as one more named section,
`load("Noise hosts", noiseHosts(day), { hosts: [] })`, so a failure is named in the day notice.
All commands run on the merged tree by the orchestrator on 2026-10-11.

| Gate | Command | Exit |
|------|---------|------|
| G001 Rust tests | `cargo test --manifest-path rust/Cargo.toml` (132 + 53 + 7 + 2134 + 2 passed) | 0 |
| G001 Rust lint | `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | 0 |
| G001 Rust format | `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | 0 |
| G001 Web tests | `cd web && bun test` (1617 pass) | 0 |
| G001 Web types | `cd web && bun run typecheck` | 0 |
| G001 Web build | `cd web && bun run build` | 0 |
| G001 Mod tests | `claude plugin test mods/worklog` (150 pass) | 0 |
| G002 Branch review | see PASS-42edb2d.md; the merge adds only main's reviewed PRs plus the one-line conflict resolution above | 0 |
| G003 Evidence | `test -s .specs/021-time-leaks/verify/acceptance.md && test -s .specs/021-time-leaks/verify/CHK001.md && test -s .specs/021-time-leaks/verify/CHK002.md` | 0 |
