Approved: 2026-10-09 by user
# Tasks — Estimate progress per person on day-page blocks
Spec: spec.md · Design: design.md · Base: 362bdaf · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

UI rule: every task that changes a screen is built with the `/design:design` skill and matches `mock.html` v3 (D-05). Rebuild the mock with `python3 .specs/020-tempo-estimate-per-person/.vary/build/gen.py`.

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a mocked Jira ticket with estimate 14400 s and 2 pages of worklogs by 2 authors, when collected, then estimate and every worklog (id, name, day, seconds) come back | T002 | jira_time tests |
| B2 (P0) | Given a cache 11 min old, when the day's progress is asked, then that ticket is refetched; a 9-min-old one is not | T003, T004 | ticket_progress_test, daemon_progress_test |
| B3 (P0) | Given Jira returns 500 and a cached row exists, when progress is asked, then the cached view returns with `error: jira_unavailable` | T004 | daemon_progress_test |
| B4 (P0) | Given estimate 4h, You 2h, Jón Geir 2h 30m and an unsynced 1h block, when the bar model is built, then segments are You/Jón Geir/+1h pending, used 5h 30m, tone "over", "1h 30m over" | T006 | progress.test |
| B5 (P0) | Given 79 % / 80 % / 100 % / 101 % used, then tone is ok / low / low / over | T006 | progress.test |
| B6 (P1) | Given 6 authors, then 4 named segments + "Others" | T006 | progress.test |
| B7 (P0) | Given a ticket with no estimate, when the block renders, then only the muted "set Original estimate in Jira" line shows | T007 | EstimateBar.test |
| B8 (P0) | Given the day page, when it renders, then blocks paint before the progress call resolves and show "Loading hours from Jira…" | T008 | BlockCard.test |

## Phase 1 — Numbers from Jira
Goal: the daemon answers "for this day's tickets: estimate, hours per person per day, and when it was fetched", cached for 10 minutes.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib` — green with the web untouched.
- [x] T001 Contract, schema and module stubs — files: rust/crates/worklog-core/src/estimate_progress_contract.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/collectors/mod.rs, rust/crates/worklog-core/src/collectors/jira_time.rs, rust/crates/worklog-core/src/ticket_progress.rs, rust/crates/worklog-core/src/ticket_progress_test.rs, rust/crates/worklog-core/src/daemon_progress.rs, rust/crates/worklog-core/src/daemon_progress_test.rs — verify: `cargo build --manifest-path rust/Cargo.toml && cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib db` — done: bf16f9b
- [x] T002 [P] Jira estimates and all-author worklogs (FR-01, FR-02, B1) — files: rust/crates/worklog-core/src/collectors/jira_time.rs, rust/crates/worklog-core/src/collectors/jira.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib collectors::jira_time` — after: T001 — done: 9a3a37f
- [x] T003 [P] Progress store: save, staleness, mark-stale, per-person view (FR-03, FR-03a store side, FR-06 data order, §5 accuracy, B2) — files: rust/crates/worklog-core/src/ticket_progress.rs, rust/crates/worklog-core/src/ticket_progress_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib ticket_progress` — after: T001 — done: da0621b
- [ ] T004 `GET /progress/:day`: refresh stale tickets, keep cache on failure, mark stale after sync (FR-03, FR-03a, FR-04, FR-04b daemon side, §5 call count + timing, B2, B3) — files: rust/crates/worklog-core/src/daemon_progress.rs, rust/crates/worklog-core/src/daemon_progress_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib daemon_progress` — after: T002, T003

## Phase 2 — Bar and chart on every block
Goal: each day-page block shows the approved mock: per-person bar with flag, over zone and striped pending piece, legend, and running-total chart, in every state.
Independent test: `(cd web && bun test lib/progress components/EstimateBar components/ProgressChart components/BlockCard && bun run typecheck)` — green against a mocked daemon.
- [ ] T005 Web types, daemon wrapper, server action — files: web/lib/types.ts, web/lib/daemonProgress.ts, web/app/actions-progress.ts — verify: `(cd web && bun run typecheck)` — after: T001
- [ ] T006 Bar and chart model: order, fold, pending, tone, 14-day window (FR-06, FR-06a, FR-07, FR-09, FR-11a, B4, B5, B6) — files: web/lib/progress.ts, web/lib/progress.test.ts — verify: `(cd web && bun test lib/progress)` — after: T005
- [ ] T007 EstimateBar + ProgressChart, all states, built with /design:design to match mock.html (FR-04a, FR-04b, FR-05, FR-05a, FR-07a, FR-08, FR-08a, FR-10, FR-10a, FR-11, FR-12, FR-13, FR-14, B7) — files: web/components/EstimateBar.tsx, web/components/EstimateBar.test.tsx, web/components/ProgressChart.tsx, web/components/ProgressChart.test.tsx, web/components/progressIcons.tsx, web/app/globals.css — verify: `(cd web && bun test components/EstimateBar components/ProgressChart && bun run typecheck)` — after: T006
- [ ] T008 Wire into the day page without blocking it; a `pulled_at` of 1970-01-01 (set by mark_stale) reads as unknown, never "from 00:00" (FR-15, FR-15a, B8) — files: web/components/DayProgressProvider.tsx, web/components/BlockCard.tsx, web/components/BlockCard.test.tsx, web/app/[day]/page.tsx — verify: `(cd web && bun test components/BlockCard && bun run typecheck && bun run build)` — after: T004, T007
- [ ] CHK001 Owner checks GENAI-1897 against Tempo — files: web/components/EstimateBar.tsx — verify: human: on a GENAI-1897 block not yet in Tempo the day page shows "5h 30m of 4h once synced · 1h 30m over", You 2h, Jón Geir 2h 30m, striped +1h and a running-total chart, and the numbers match Tempo's Time Tracking panel (Logged + Collaborators) — after: T008

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 362bdaf`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s .specs/020-tempo-estimate-per-person/verify/`
