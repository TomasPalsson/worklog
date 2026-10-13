# Issue index — Browser-Based Setup Wizard

Slices of [spec](./spec.md) · build plan in [plan.md](./plan.md) · 8 slices, all `AFK`.
Build one at a time in a fresh session (start with #01, the walking skeleton).
Verified by an adversarial swarm before creation: coverage critic **pass** (every FR, MUST-AC, and journey mapped; no orphan deps); 5 issue bodies refined post-creation per skeptic findings.

| # | Title | Tag | depends-on | GitHub |
|---|-------|-----|------------|--------|
| 01 | `worklog setup` opens a daemon-served page; `--cli` keeps the TTY wizard | AFK | — | [#24](https://github.com/TomasPalsson/worklog/issues/24) |
| 02 | Setup page shows bootstrap (home + DB) + preflight diagnostics | AFK | 01 | [#25](https://github.com/TomasPalsson/worklog/issues/25) |
| 03 | Owner enters, validates & saves any credential (+ save-anyway, re-run state) | AFK | 01 | [#26](https://github.com/TomasPalsson/worklog/issues/26) |
| 04 | Owner chooses estimator provider (subprocess / LiteLLM) | AFK | 03 | [#27](https://github.com/TomasPalsson/worklog/issues/27) |
| 05 | Owner sets/disables the scheduled-collection interval | AFK | 01 | [#28](https://github.com/TomasPalsson/worklog/issues/28) |
| 06 | Owner installs/refreshes the Claude Code hook | AFK | 01 | [#29](https://github.com/TomasPalsson/worklog/issues/29) |
| 07 | Owner installs/refreshes the bundled skill (byte-diff detection) | AFK | 01 | [#30](https://github.com/TomasPalsson/worklog/issues/30) |
| 08 | Owner installs daemon auto-start (non-blocking failure + retry) | AFK | 01 | [#31](https://github.com/TomasPalsson/worklog/issues/31) |

## Dependency graph

```
01 ──┬── 02
     ├── 03 ── 04
     ├── 05
     ├── 06
     ├── 07
     └── 08
```
