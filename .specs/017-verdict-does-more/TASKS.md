# Tasks — Verdict does more
Spec: spec.md · Design: design.md · Base: 6eb7a92 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a fix, when it is saved, then the log holds before and after, and purge keeps it | T001, T005, T006 | verdict_decisions tests |
| B2 (P0) | Given Verdict switched on, when it crashes, then it restarts at most 3 times per 10 min | T004 | verdict_supervisor tests |
| B3 (P0) | Given 30 folders, when an event is routed, then Verdict sees ≤6 with ≤5 examples each | T002, T005 | routing_shortlist tests, server self-test |
| B4 (P0) | Given Verdict disagrees with itself across orders, when routing runs, then the event stays unsorted | T002, T003 | routing tests |
| B5 (P0) | Given a manual or event ticket, when Verdict picks another, then the ticket is kept | T006 | ticket_verdict tests |
| B6 (P0) | Given a vague generated line, when it fails twice, then it is flagged needs_look | T007 | line_check tests |
| B7 (P0) | Given auto-send on, when 17:00 passes, then only ready lines are sent, once | T009 | auto_send tests |
| B8 (P0) | Given an edit in Review, when saved, then the same Tempo worklog is updated | T009, T012 | auto_send + ReviewSection tests |
| B9 (P1) | Given 30 days of fixes, when the scorecard runs, then right/wrong/unsure counts are reported | T008 | scorecard tests |

## Phase 1 — The decision log and Verdict's new answer
Goal: every Verdict answer carries its top 3 and the order check, and a permanent log exists.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core verdict` — green with routing untouched.
- [ ] T001 Decision log table, new columns and log functions — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/verdict_decisions.rs, rust/crates/worklog-core/src/verdict_decisions_test.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/purge.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core verdict_decisions`
- [ ] T002 [P] Verdict server: top-3 ranking, order check, examples in option text, delete group split/merge — files: rust/crates/worklog-core/templates/verdict_server.py — verify: `python3 rust/crates/worklog-core/templates/verdict_server.py --self-test`
- [ ] T003 [P] Verdict client returns Ranking; filing rule needs the order check; text-match client — files: rust/crates/worklog-core/src/verdict.rs, rust/crates/worklog-core/src/routing_contract.rs, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/routing_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core verdict routing`

## Phase 2 — Verdict runs itself and does more work
Goal: Verdict starts with worklog, files from a shortlist with examples, picks clear tickets and checks Tempo text.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green with the web untouched.
- [ ] T004 Verdict supervisor and its daemon routes — files: rust/crates/worklog-core/src/verdict_supervisor.rs, rust/crates/worklog-core/src/verdict_supervisor_test.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core verdict_supervisor` — after: T001
- [ ] T005 [P] Project shortlist, examples, ranking stored on the event, every guess and fix logged — files: rust/crates/worklog-core/src/routing_shortlist.rs, rust/crates/worklog-core/src/routing_shortlist_test.rs, rust/crates/worklog-core/src/routing.rs, rust/crates/worklog-core/src/routing_test.rs, rust/crates/worklog-core/src/routing_rows.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing` — after: T001, T003
- [ ] T006 [P] Verdict picks clear tickets; Owner ticket swaps logged — files: rust/crates/worklog-core/src/ticket_verdict.rs, rust/crates/worklog-core/src/ticket_verdict_test.rs, rust/crates/worklog-core/src/estimate.rs, rust/crates/worklog-core/src/block_service.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ticket_verdict estimate` — after: T001, T003
- [ ] T007 [P] Tempo line text checks, one regenerate, needs_look flag — files: rust/crates/worklog-core/src/line_check.rs, rust/crates/worklog-core/src/line_check_test.rs, rust/crates/worklog-core/src/tempo_lines.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core line_check tempo_lines` — after: T001, T003
- [ ] T008 Nightly scorecard and `worklog eval --replay` — files: rust/crates/worklog-core/src/scorecard.rs, rust/crates/worklog-core/src/scorecard_test.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-cli/src/eval_cmd.rs, rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml scorecard eval` — after: T004, T005, T006
- [ ] T009 Auto-send at 17:00, readiness, review list and confirm routes — files: rust/crates/worklog-core/src/auto_send.rs, rust/crates/worklog-core/src/auto_send_test.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core auto_send` — after: T006, T007, T008

## Phase 3 — What the Owner sees
Goal: Settings controls Verdict and auto-send; the day page shows Verdict's state, one-tap choices, flagged lines and the Review section.
Independent test: `cd web && bun test && bun run typecheck` — green.
- [ ] T010 Settings: Verdict switch with state and scorecard line; auto-send switch — files: web/components/VerdictControl.tsx, web/components/VerdictControl.test.tsx, web/components/SettingsPanel.tsx, web/lib/settingsForm.ts, web/lib/daemon.ts, web/app/actions.ts — verify: `cd web && bun test components/VerdictControl.test.tsx && bun run typecheck` — after: T004, T009
- [ ] T011 Day page: Verdict-off line, one-tap project buttons, needs-a-look flag — files: web/components/VerdictBanner.tsx, web/components/VerdictBanner.test.tsx, web/components/UnsortedList.tsx, web/components/UnsortedList.test.tsx, web/components/TicketGroup.tsx, web/lib/types.ts, web/lib/daemon.ts, web/app/[day]/page.tsx — verify: `cd web && bun test components/VerdictBanner.test.tsx components/UnsortedList.test.tsx && bun run typecheck` — after: T005, T007, T010
- [ ] T012 Review section on today's page: confirm, confirm day, edit, send again — files: web/components/ReviewSection.tsx, web/components/ReviewSection.test.tsx, web/lib/daemon.ts, web/app/actions.ts, web/app/[day]/page.tsx — verify: `cd web && bun test components/ReviewSection.test.tsx && bun run typecheck && bun run build` — after: T009, T011
- [ ] CHK001 human-verify Verdict control — files: web/components/VerdictControl.tsx — verify: human: Owner switches Verdict on in Settings and sees "running", kills the helper and sees it come back, switches it off and sees the grey line on the day page
- [ ] CHK002 human-verify the Review section on a real day — files: web/components/ReviewSection.tsx — verify: human: Owner sees yesterday's auto-sent lines, confirms one, edits the hours of another, and Tempo shows the same worklog with the new hours

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 6eb7a92`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
