# Tasks — Tempo ticket auto-pick
Spec: spec.md · Design: design.md · Base: d0b5774 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a block, when the Owner assigns or clears its ticket, then its origin is manual | T002 | block_service assign_ticket tests |
| B2 (P0) | Given a fresh block whose events share one key, when infer rebuilds, then it has that key with origin event | T003 | infer_allocations_db_test |
| B3 (P0) | Given a manual ticket (or manual clear), when infer rebuilds, then ticket and origin are unchanged | T003 | infer_allocations_db_test |
| B4 (P0) | Given a non-manual block, when the estimator picks a different valid key, then it is written with origin auto | T004 | estimate tests (FixedInvoker) |
| B5 (P0) | Given a manual block, when the estimator returns another key, then the ticket is unchanged | T004 | estimate tests (FixedInvoker) |
| B6 (P0) | Given two overlapping blocks on one ticket, when the line is read, then hours = rounded union | T005 | tempo_lines_test |
| B7 (P0) | Given a synced line, when text or hours are edited, then its synced blocks are dirty | T005 | tempo_lines_test |
| B8 (P0) | Given a line with stored text and a 2h override, when sync runs, then Tempo receives that text and 7200 s | T006 | tempo.rs httpmock test |
| B9 (P1) | Given lines without text, when the estimate run finishes, then they have generated text | T007, T008 | daemon_tempo_lines_test, cli test |
| B10 (P0) | Given a block with an event/auto ticket, when the day renders, then an "auto" tag shows | T010 | BlockCard.test.tsx |
| B11 (P0) | Given a ticket line, when the Owner edits text or hours, then the saved values render | T011 | TicketGroup.test.tsx |

## Phase 1 — Hand-set tickets stay put
Goal: every block ticket knows who set it, and the scheduler never replaces one the Owner set.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green with web untouched.
- [ ] T001 Schema: blocks.ticket_origin + tempo_line_texts, version 17 — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db_test`
- [ ] T002 Block.ticket_origin field; assign_ticket records manual (B1) — files: rust/crates/worklog-core/src/models.rs, rust/crates/worklog-core/src/repo.rs, rust/crates/worklog-core/src/block_service.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/tenant_split_test.rs, rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/eval_cmd.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core block_service` — after: T001
- [ ] T003 [P] infer writes event origin and keeps manual tickets (B2, B3) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_allocations_db_test` — after: T002
- [ ] T004 [P] estimator writes auto origin and keeps manual tickets (B4, B5) — files: rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core estimate::` — after: T002

## Phase 2 — Ticket lines hold what Tempo gets
Goal: each ticket line has a stored description, union hours and an optional override, and sync sends exactly those.
Independent test: `cargo test --manifest-path rust/Cargo.toml` — green with web untouched.
- [ ] T005 [P] tempo_lines module: read, edit, hours, generation (B6, B7) — files: rust/crates/worklog-core/src/tempo_lines.rs, rust/crates/worklog-core/src/tempo_lines_test.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tempo_lines` — after: T002
- [ ] T006 [P] Sync sends stored line text and effective hours (B8) — files: rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::tempo` — after: T005
- [ ] T007 [P] Daemon /tempo/lines routes; estimate run generates line texts (B9) — files: rust/crates/worklog-core/src/daemon_tempo_lines.rs, rust/crates/worklog-core/src/daemon_tempo_lines_test.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_tempo_lines` — after: T005
- [ ] T008 [P] `worklog day` generates missing line texts (B9) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli` — after: T005

## Phase 3 — Review screen
Goal: the Tickets view shows auto tags, line descriptions you can edit or regenerate, and an hours box.
Independent test: `cd web && bun test && bun run typecheck` — green with the daemon mocked.
- [ ] T009 Web client and server actions for ticket lines — files: web/lib/daemonTempoLines.ts, web/app/actions-tempo-lines.ts, web/app/actions-tempo-lines.test.ts — verify: `cd web && bun test app/actions-tempo-lines.test.ts` — after: T007
- [ ] T010 [P] "auto" tag on BlockCard (B10) — files: web/lib/types.ts, web/components/BlockCard.tsx, web/components/BlockCard.test.tsx — verify: `cd web && bun test components/BlockCard.test.tsx` — after: T007
- [ ] T011 TicketGroup line text, regenerate and hours override (B11) — files: web/components/TicketGroup.tsx, web/components/TicketGroup.test.tsx, web/components/BillingGroup.tsx, web/app/[day]/page.tsx — verify: `cd web && bun test components/TicketGroup.test.tsx components/BillingGroup.test.tsx` — after: T009
- [ ] CHK001 human-verify the day review and a real sync — files: web/components/TicketGroup.tsx — verify: human: Owner opens a day in the Tickets view, sees every block ticketed (auto ones tagged), each line with a description and union hours, sets one line to 2h, presses "Sync to Tempo", and Tempo shows exactly that text and 2h

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
