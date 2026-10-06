Approved: 2026-10-06 by user
# Tasks — Daily helpers
Spec: spec.md · Design: design.md · Base: 6eb7a92 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

UI rule: every task that changes a screen is built with the `/design:design` skill (Owner's rule, 2026-10-06).

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given Tempo returns 400 "Issue is closed", when sync runs, then the report shows status and body | T002 | tempo error_body tests |
| B2 (P0) | Given a line of 5100 s, when the day page renders, then it shows the same billed hours Tempo receives | T005 | LineHours/format tests |
| B3 (P0) | Given a mistaken merge, when the Owner undoes, then both blocks return; given a synced block, undo refuses | T006 | undo_test |
| B4 (P0) | Given a draft, when nothing is clicked, then nothing is posted; when Post is clicked, the reply lands in today's Daily thread | T016, T020 | slack_post_test, StandupButton.test |
| B5 (P0) | Given a block with no ticket, when the Owner opens Send, then a red row names it and Send is disabled | T023, T026 | preflight_test, TaskSyncConfirm.test |
| B6 (P1) | Given a same-work hand entry within 30 min, when sync runs, then no worklog is created | T029, T030 | tempo_match_test |
| B7 (P1) | Given 17:00 run finished, then a recap with 3 gaps shows on Day page and footer | T031–T033 | recap_test, RecapBanner.test |

## Phase 1 — Honest numbers
Goal: Tempo failures name their cause, and the billed hours on screen are the ones Tempo gets.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib && (cd web && bun test lib/format components/LineHours)` — green with no new UI.
- [ ] T001 Module stubs declared once — files: rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/billing_round.rs, rust/crates/worklog-core/src/undo.rs, rust/crates/worklog-core/src/ask.rs, rust/crates/worklog-core/src/report.rs, rust/crates/worklog-core/src/standup.rs, rust/crates/worklog-core/src/slack_post.rs, rust/crates/worklog-core/src/nudges.rs, rust/crates/worklog-core/src/preflight.rs, rust/crates/worklog-core/src/daemon_undo.rs, rust/crates/worklog-core/src/daemon_ask.rs, rust/crates/worklog-core/src/daemon_standup.rs, rust/crates/worklog-core/src/daemon_nudges.rs, rust/crates/worklog-core/src/daemon_preflight.rs, rust/crates/worklog-cli/src/lib.rs, rust/crates/worklog-cli/src/helpers_cmd.rs — verify: `cargo build --manifest-path rust/Cargo.toml`
- [ ] T002 [P] Tempo errors say why (FR-01, FR-02, B1) — files: rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo` — after: T001
- [ ] T003 [P] Billing lines say "fill in" for missing times (FR-03) — files: rust/crates/worklog-core/src/billing.rs, web/lib/export.ts, web/lib/export.test.ts — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib billing && (cd web && bun test lib/export)` — after: T001
- [ ] T004 Half-hour rounding lives in billing_round only (FR-05) — files: rust/crates/worklog-core/src/billing_round.rs, rust/crates/worklog-core/src/collectors/tempo.rs, rust/crates/worklog-core/src/billing.rs, rust/crates/worklog-core/src/tempo_lines.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib` — after: T002, T003
- [ ] T005 Web shows the server's billed hours; mirror deleted (FR-04, FR-05, B2) — files: web/app/[day]/page.tsx, web/components/TicketGroup.tsx, web/components/LineHours.tsx, web/lib/format.ts, web/lib/format.test.ts — verify: `(cd web && bun test lib/format components/LineHours && bun run typecheck) && ! grep -rn "Math.ceil(seconds / 1800)" web/lib web/components web/app` — after: T004

## Phase 2 — Undo, ask, report
Goal: a wrong block edit is one undo away, "when did I last touch X" answers in a second, and a customer month prints.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib undo ask report && cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — green with Phase 3 untouched.
- [ ] T006 Undo journal and undo_last (FR-25, FR-26, FR-27, FR-28, B3) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/undo.rs, rust/crates/worklog-core/src/undo_test.rs, rust/crates/worklog-core/src/block_service.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib undo` — after: T001
- [ ] T007 [P] Undo daemon handler — files: rust/crates/worklog-core/src/daemon_undo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_undo` — after: T006
- [ ] T008 Ask search and index (FR-31, FR-32, FR-33) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/ask.rs, rust/crates/worklog-core/src/ask_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib ask` — after: T006
- [ ] T009 [P] Ask daemon handler — files: rust/crates/worklog-core/src/daemon_ask.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_ask` — after: T008
- [ ] T010 [P] Monthly customer report (FR-34, FR-35, FR-36, FR-37, FR-38) — files: rust/crates/worklog-core/src/report.rs, rust/crates/worklog-core/src/report_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib report` — after: T001
- [ ] T011 CLI wiring: undo, ask, report (FR-30) — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/helpers_cmd.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T006, T008, T010
- [ ] T012 Register undo and ask routes — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon` — after: T007, T009
- [ ] T013 [P] Web Undo on the block-change toast (FR-29) — UI built with the /design:design skill — files: web/lib/daemonUndo.ts, web/components/ToastHost.tsx, web/lib/toast.ts, web/lib/toast.test.ts, web/app/actions.ts — verify: `(cd web && bun test lib/toast && bun run typecheck)` — after: T012
- [ ] T014 [P] /wl ask in Claude Code (FR-30) — files: mods/worklog/hooks/command.tsx, mods/worklog/hooks/command.test.tsx — verify: `(cd mods/worklog/hooks && bun test command)` — after: T012

## Phase 3 — Standup and nudges
Goal: one click drafts the standup in the team's format and posts it to today's Daily thread; the footer shows one nudge.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib standup slack_post nudges && (cd web && bun test components/StandupButton) && (cd mods/worklog/hooks && bun test status)`.
- [ ] T015 [P] Standup draft from blocks, tickets, PRs and prompts (FR-18, FR-19, FR-24) — files: rust/crates/worklog-core/src/standup.rs, rust/crates/worklog-core/src/standup_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib standup` — after: T001
- [ ] T016 [P] Find today's Daily thread and reply (FR-21, FR-22, FR-23, B4) — files: rust/crates/worklog-core/src/slack_post.rs, rust/crates/worklog-core/src/slack_post_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib slack_post` — after: T001
- [ ] T017 Standup daemon handlers (draft, post) — files: rust/crates/worklog-core/src/daemon_standup.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_standup` — after: T015, T016
- [ ] T018 [P] Nudges: review-requested query, merged-not-done, stale (FR-40, §5 cache) — files: rust/crates/worklog-core/src/collectors/github.rs, rust/crates/worklog-core/src/nudges.rs, rust/crates/worklog-core/src/nudges_test.rs, rust/crates/worklog-core/src/daemon_nudges.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib nudges` — after: T001
- [ ] T019 Register standup and nudge routes, Daily channel setting — files: rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon` — after: T012, T017, T018
- [ ] T020 Standup button, editable preview, Post and Copy, Settings channel field (FR-17, FR-20, FR-22, FR-23, B4) — UI built with the /design:design skill — files: web/components/StandupButton.tsx, web/components/StandupButton.test.tsx, web/lib/daemonStandup.ts, web/components/DayHeader.tsx, web/components/SettingsFormSections.tsx, web/lib/settingsForm.ts — verify: `(cd web && bun test components/StandupButton && bun run typecheck)` — after: T019
- [ ] T021 [P] `worklog standup` with confirm before post (FR-17, FR-22) — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/helpers_cmd.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T011, T017
- [ ] T022 [P] Footer nudge, one line, rotates per prompt (FR-39, FR-41, FR-42) — files: mods/worklog/hooks/status.ts, mods/worklog/hooks/status.test.ts — verify: `(cd mods/worklog/hooks && bun test status)` — after: T019
- [ ] CHK001 human-verify a real standup — files: web/components/StandupButton.tsx — verify: human: the Owner presses Standup, edits, presses Post, and the reply appears in today's Daily thread within 30 s of the click — after: T020

## Phase 4 — Pre-send checklist
Goal: a send the Owner starts by hand shows a checklist first and proves itself after.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib preflight && (cd web && bun test components/TaskSyncConfirm)`.
- [ ] T023 Preflight checks and read-back (FR-11, FR-12, FR-14, B5) — files: rust/crates/worklog-core/src/preflight.rs, rust/crates/worklog-core/src/preflight_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib preflight` — after: T004
- [ ] T024 Preflight handler and route — files: rust/crates/worklog-core/src/daemon_preflight.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_preflight` — after: T019, T023
- [ ] T025 [P] Terminal sync prints the checklist and stops on red (FR-15) — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/helpers_cmd.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T021, T023
- [ ] T026 [P] Checklist in the Send to Tempo confirm, Send anyway (FR-13, B5) — UI built with the /design:design skill — files: web/components/TaskSyncConfirm.tsx, web/components/TaskSyncConfirm.test.tsx, web/components/TaskDaySync.tsx, web/lib/daemonPreflight.ts — verify: `(cd web && bun test components/TaskSyncConfirm && bun run typecheck)` — after: T024

## Phase 5 — After spec 017: already-in-Tempo and the 17:00 recap
Goal: a line equal to a hand entry is not sent twice, and the 17:00 run ends with a recap.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match recap && (cd web && bun test components/RecapBanner)`.
- [ ] T027 Rebase onto main that contains spec 017 — files: . — verify: `test -f rust/crates/worklog-core/src/auto_send.rs` — after: T026
- [ ] T028 Extend the contract for match status and recap (orchestrator) — files: rust/crates/worklog-core/src/daily_helpers_contract.rs, web/lib/daily_helpers_contract.ts — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daily_helpers_contract && (cd web && bun run typecheck)` — after: T027
- [ ] T029 Already-in-Tempo check with Verdict (FR-06, FR-07, FR-08, FR-09, FR-10, B6) — files: rust/crates/worklog-core/src/tempo_match.rs, rust/crates/worklog-core/src/tempo_match_test.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match` — after: T028
- [ ] T030 Send path asks tempo_match first; auto-send skips the checklist (FR-06, FR-16) — files: rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::tempo` — after: T029
- [ ] T031 Recap built inside the 17:00 run (FR-43, FR-45, B7) — files: rust/crates/worklog-core/src/recap.rs, rust/crates/worklog-core/src/recap_test.rs, rust/crates/worklog-core/src/auto_send.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib recap` — after: T029
- [ ] T032 Recap route and Day banner with gap actions (FR-44, FR-46) — UI built with the /design:design skill — files: rust/crates/worklog-core/src/daemon.rs, web/components/RecapBanner.tsx, web/components/RecapBanner.test.tsx, web/app/[day]/page.tsx — verify: `(cd web && bun test components/RecapBanner && bun run typecheck)` — after: T031
- [ ] T033 [P] Recap in the Claude Code footer (FR-46) — files: mods/worklog/hooks/status.ts, mods/worklog/hooks/status.test.ts — verify: `(cd mods/worklog/hooks && bun test status)` — after: T032
- [ ] CHK002 human-verify the 17:00 recap — files: web/components/RecapBanner.tsx — verify: human: after a real 17:00 run the Owner sees what was sent, what was held back, and 3 gaps with working Personal/Break/Pick a ticket buttons — after: T032

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 6eb7a92`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
