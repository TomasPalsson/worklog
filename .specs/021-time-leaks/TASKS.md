Approved: 2026-10-11 by user
# Tasks — Time leaks
Spec: spec.md · Design: design.md · Base: 5d765ac · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given an ignored block, when the week is aggregated, then it counts as ignored, not personal | T001 | aggregate test |
| B2 (P0) | Given work overlapping an ignored block, when aggregated, then work + personal + ignored − overlap = total | T001 | aggregate overlap test |
| B3 (P0) | Given one ticket's send errors, when auto-send runs, then its error is stored and the next ticket is sent | T003 | auto_send test |
| B4 (P0) | Given Tempo skips a line, when auto-send runs, then it is stored as not sent with the reason | T003 | auto_send skip test |
| B5 (P0) | Given a block, when ignored with a reason, then the reason is stored, returned and survives a rebuild | T010, T011, T012 | block_service, infer, daemon tests |
| B6 (P0) | Given ticketed blocks lacking Tempo or export in the cycle, when loose ends load, then each is listed with what is missing | T014 | loose_ends test |
| B7 (P1) | Given a probe error, when status runs, then it says "check failed" | T006 | status test |
| B8 (P1) | Given a collector run, when it ends, then its last success or error is recorded and shown | T007, T008 | collect_health test |
| B9 (P1) | Given 2 pages of GitHub results, when collecting, then both pages are stored | T004 | github paging test |

## Phase 0 — Contract
Goal: the shared shapes exist and compile on both sides.
Independent test: `cargo check --manifest-path rust/Cargo.toml && cd web && bun run typecheck`
- [x] T000 Contract files from design §1 (orchestrator) — files: rust/crates/worklog-core/src/time_leaks_contract.rs, rust/crates/worklog-core/src/lib.rs, web/lib/time_leaks_contract.ts — verify: `cargo check --manifest-path rust/Cargo.toml && cd web && bun run typecheck` — done: 2618eed

## Phase 1 — Honest numbers and loud failures
Goal: totals add up and every failure says what failed, the same day.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core auto_send && cargo test --manifest-path rust/Cargo.toml -p worklog-cli aggregate`
- [x] T001 CLI ignored bucket in day/week tables and JSON (FR-01, FR-02, B1, B2) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli aggregate` — after: T000 — done: bda16bd
- [x] T002 [P] Reclassify leaves ignored blocks alone (FR-03) — files: rust/crates/worklog-core/src/personal.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core personal` — after: T000 — done: 3509480
- [x] T003 [P] Auto-send stores per-ticket errors, skipped ≠ sent, today in Review (FR-08, FR-09, FR-10, B3, B4) — files: rust/crates/worklog-core/src/auto_send.rs, rust/crates/worklog-core/src/auto_send_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core auto_send` — after: T000 — done: a09cd90
- [x] T004 [P] GitHub collect pages through results up to GITHUB_MAX_PAGES (FR-18, B9) — files: rust/crates/worklog-core/src/collectors/github.rs, rust/crates/worklog-core/src/collectors/github_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core github` — after: T000 — done: e9f8dd6
- [x] T005 [P] Day page names failed sections and renders the rest (FR-13) — files: web/app/[day]/page.tsx, web/components/DayLoadNotice.tsx, web/components/DayLoadNotice.test.tsx — verify: `cd web && bun test components/DayLoadNotice.test.tsx && bun run typecheck` — after: T000 — done: 27de4f0
- [x] T006 Status says "check failed (reason)" and prints fix commands (FR-14, FR-15, B7) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli status` — after: T001 — done: 0dc2cbb

## Phase 2 — Source health
Goal: a quiet or failing source shows up in `worklog status`.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collect_health`
- [x] T007 collect_health module: record and read per-source health (FR-16, B8) — files: rust/crates/worklog-core/src/collect_health.rs, rust/crates/worklog-core/src/collect_health_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collect_health` — after: T000 — done: 1c086a2
- [x] T008 Collect records every outcome; status lists sources (FR-16, FR-17, B8) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T006, T007 — done: 69c95df
- [x] T009 [P] Hook and transcript reader record their errors (FR-16) — files: rust/crates/worklog-core/src/hook_run.rs, rust/crates/worklog-core/src/hook_run_test.rs, rust/crates/worklog-core/src/collectors/claude_transcripts.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core hook_run` — after: T007 — done: cc13977

## Phase 3 — Ignore with a reason
Goal: ignore says why, and Duplicate/Internal take the better path.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ignore && cd web && bun test components/BlockCard.test.tsx`
- [x] T010 ignore_reason column, model field, set_ignored(reason) (FR-04, B5) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/models.rs, rust/crates/worklog-core/src/repo.rs, rust/crates/worklog-core/src/block_service.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — after: T000 — done: 77de18d
- [x] T011 Rebuild keeps the ignore reason (FR-05, B5) — files: rust/crates/worklog-core/src/infer.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer` — after: T010 — done: 6be7dde
- [x] T012 Ignore route takes an optional reason (FR-04, FR-07, B5) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon` — after: T010 — done: 1fddeeb
- [x] T013 Ignore menu: Noise, Duplicate (merge or ignore), Internal (ticket picker) (FR-06a, FR-06b, FR-06c) — files: web/components/BlockCard.tsx, web/components/BlockCard.test.tsx, web/app/actions.ts, web/lib/daemon.ts — verify: `cd web && bun test components/BlockCard.test.tsx && bun run typecheck` — after: T012 — done: 41c2567

## Phase 4 — Helpers
Goal: loose ends, deild hints, install command and focus line work end to end.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core loose_ends && cd web && bun test components/LooseEnds.test.tsx`
- [x] T014 loose_ends module for the current billing cycle (FR-11, B6) — files: rust/crates/worklog-core/src/loose_ends.rs, rust/crates/worklog-core/src/loose_ends_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core loose_ends` — after: T007 — done: 6bbb464
- [x] T015 GET /loose-ends route (FR-11) — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon` — after: T012, T014 — done: 0eb63b4
- [x] T016 Loose ends page and nav link (FR-11, FR-12) — files: web/app/loose-ends/page.tsx, web/components/LooseEnds.tsx, web/components/LooseEnds.test.tsx, web/components/AppNav.tsx, web/lib/daemonLooseEnds.ts — verify: `cd web && bun test components/LooseEnds.test.tsx && bun run typecheck` — after: T015 — done: 74c26f5
- [x] T017 Deild suggestion query and route (FR-21) — files: rust/crates/worklog-core/src/daemon_tenants.rs, rust/crates/worklog-core/src/daemon_tenants_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_tenants` — after: T015 — done: 1e3a406
- [x] T018 Deild suggestion chip, saves only on click (FR-21) — files: web/components/BlockCustomerSplit.tsx, web/components/BlockCustomerSplit.test.tsx, web/lib/tenants.ts — verify: `cd web && bun test components/BlockCustomerSplit.test.tsx && bun run typecheck` — after: T017 — done: 6c7eb4f
- [x] T019 [P] Focus summary in the standup draft, never in the post (FR-23) — files: rust/crates/worklog-core/src/standup.rs, rust/crates/worklog-core/src/standup_test.rs, rust/crates/worklog-core/src/daemon_standup.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core standup` — after: T000 — done: 115cc7d
- [x] T020 Focus line in CLI and web standup draft (FR-23) — files: rust/crates/worklog-cli/src/helpers_cmd.rs, web/components/StandupButton.tsx, web/components/StandupButton.test.tsx, web/lib/daemonStandup.ts — verify: `cd web && bun test components/StandupButton.test.tsx && cd .. && cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T019 — done: be89308
- [x] T021 [P] /wl install in the Claude Code mod, origin/main only, refuses outside the repo (FR-22) — files: mods/worklog/hooks/command.tsx, mods/worklog/hooks/command.test.tsx — verify: `claude plugin test mods/worklog` — after: T000 — done: 43db4ce

## Phase 5 — Restarts and tidy-up
Goal: install failures are loud, help and README match, old folders are gone.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core -- daemon_service web:: schedule`
- [x] T022 [P] Daemon and schedule install fail on a failed load; daemon install confirms it answers (FR-19a, FR-19b) — files: rust/crates/worklog-core/src/daemon_service.rs, rust/crates/worklog-core/src/schedule.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core -- daemon_service schedule` — after: T000 — done: 5acd1fc
- [x] T023 [P] web down keeps the pid file until the process is gone; install script stops masking kill errors (FR-20) — files: rust/crates/worklog-core/src/web.rs, scripts/install-local.sh — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core web:: && bash -n scripts/install-local.sh` — after: T000 — done: 2829310
- [x] T024 Hide serve and eval from help; both still run (FR-24) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T008 — done: ee50e9f
- [x] T025 README command table matches --help; upgrade is the alias (FR-25) — files: README.md — verify: `cargo run -q --manifest-path rust/Cargo.toml -p worklog-cli -- --help >/dev/null` — after: T024 — done: 82f7c66
- [~] T026 Archive spec folders 003, 004, 007, 008 with LEDGER rows (orchestrator) (FR-26) — files: .specs/LEDGER.md, .specs/003-browser-slack-event-routing, .specs/004-verdict-routing, .specs/007-session-lanes-and-fresh-descriptions, .specs/008-session-customer-pins, .specs/archive — verify: `test ! -d .specs/003-browser-slack-event-routing && test ! -d .specs/008-session-customer-pins` — after: T000 — dropped: already done before Base in 5d202a9 (#132); verify passes, LEDGER rows exist
- [~] T027 Remove the 5 clean merged worktrees after re-checking clean, unlocked, unchanged for 48 h (orchestrator) (FR-27) — files: .claude/worktrees — verify: `! git worktree list | grep -E 'billed-hours-header|settings-z-index|task-board-5-columns|task-board-wide|ticket-create-assign-me'` — after: T000 — dropped: worktree-isolated session may not run git on sibling worktrees; owner runs the check+remove in NOTES.md Ruling
- [x] CHK001 Owner checks the day page: ignore menu, load notice, loose-ends page, deild chip — files: web/components/BlockCard.tsx, web/components/LooseEnds.tsx — verify: human: Owner says the four look right on the running app — after: T005, T013, T016, T018 — done: 74c26f5 by user
- [x] CHK002 Owner checks `worklog week` and `worklog status` after install — files: rust/crates/worklog-cli/src/cli.rs — verify: human: Owner says 4–10 Oct adds up and status lists sources — after: T008, T024 — done: ee50e9f by user

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 5d765ac`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
