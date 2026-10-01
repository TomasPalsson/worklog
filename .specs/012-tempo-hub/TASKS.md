# Tasks — Tempo hub
Approved: 2026-10-01 by user
Spec: spec.md · Design: design.md · Base: 5b084dc · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given cached tickets in new/indeterminate/done, when tasks are listed, then only non-done assigned ones are `assigned` | T006 | task_board_test |
| B2 (P0) | Given an uncached ticket with a line this week, when tasks are listed, then it appears with `assigned = false` | T006 | task_board_test |
| B3 (P0) | Given a line with a 2h override, when tasks are listed, then `week_seconds` counts 7200 | T006 | task_board_test |
| B4 (P0) | Given a refresh that omits a cached non-picked ticket, when it finishes, then that ticket's category is done | T002 | jira.rs mock test |
| B5 (P0) | Given a ticket, when transitions are listed and one is applied, then the cached status equals Jira's new status | T002, T009 | jira.rs mock tests, daemon_tasks_test |
| B6 (P0) | Given an empty or 5001-char comment, when posted, then the daemon returns 400 and calls nothing | T009 | daemon_tasks_test |
| B7 (P0) | Given the model returns a transition id not in the list, when drafting, then `suggested_transition_id` is None | T008 | task_draft_test |
| B8 (P0) | Given Tempo returns two pages and a schedule, when the week is pulled, then all rows are stored and owners are classified | T003, T004 | tempo.rs mock test, tempo_remote_test |
| B9 (P0) | Given an outside worklog for (day, issue) and an unsynced line, when sync runs, then the line is skipped and no POST is sent | T005 | tempo.rs mock test |
| B10 (P0) | Given a week with lines, pulled worklogs and a schedule, when close-out is read, then every per-day number matches | T007 | week_closeout_test |
| B11 (P0) | Given Jira answers 400 to a transition, when the route runs, then it returns 502 with Jira's body | T009 | daemon_tasks_test |
| B12 (P0) | Given three days with pending lines, when Sync week runs, then pull, three day syncs in order, pull | T013 | weekSync.test.ts |
| B15 (P0) | Given day 2 of 3 fails, when Sync week runs, then it stops and day 3 is never sent | T013 | weekSync.test.ts |
| B16 (P0) | Given a refresh failing on page 2, when it runs, then no ticket is marked done | T002 | jira.rs mock test |
| B17 (P0) | Given required 8h, when in-Tempo is 7.5h, then the day is a gap; at 8h or required 0 it is not | T013 | WeekCloseout.test.tsx |
| B13 (P1) | Given a task card, when it renders, then it links to Jira and shows week and today hours | T012 | TaskBoard.test.tsx |
| B14 (P1) | Given the day and week headers, when they render, then each links to /tasks | T014 | DayHeader.test.tsx |

## Phase 1 — Core data and clients
Goal: worklog can talk status and comments with Jira, read a week back from Tempo, and answer "my tasks" and "how is my week" from SQLite.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green with web untouched.
- [ ] T001 Schema v18: jira_tickets.status_category, tempo_remote_worklogs, tempo_required_days — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db`
- [ ] T002 [P] Jira transitions, status, comments; refresh pages fully, stores category, marks unreturned done (B4, B5, B16) — files: rust/crates/worklog-core/src/collectors/jira.rs, rust/crates/worklog-core/src/repo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::jira` — after: T001
- [ ] T003 [P] Tempo read client: worklogs (paged) and user schedule; resolve_account_id pub (B8) — files: rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::tempo` — after: T001
- [ ] T004 [P] Store a pulled week and classify owners (B8) — files: rust/crates/worklog-core/src/tempo_remote.rs, rust/crates/worklog-core/src/tempo_remote_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core tempo_remote` — after: T001
- [ ] T006 [P] Task board query (B1, B2, B3) — files: rust/crates/worklog-core/src/task_board.rs, rust/crates/worklog-core/src/task_board_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core task_board` — after: T001
- [ ] T007 [P] Week close-out query (B10) — files: rust/crates/worklog-core/src/week_closeout.rs, rust/crates/worklog-core/src/week_closeout_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core week_closeout` — after: T001
- [ ] T008 [P] AI ticket update draft (B7) — files: rust/crates/worklog-core/src/task_draft.rs, rust/crates/worklog-core/src/task_draft_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core task_draft` — after: T001
- [ ] T005 Sync skips a line that already has an outside worklog (B9) — files: rust/crates/worklog-core/src/collectors/tempo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::tempo` — after: T003, T004

## Phase 2 — Daemon routes
Goal: the web app can list tasks, move and comment on tickets, get an AI draft, pull a week from Tempo and read the close-out, without the daemon lock held during any outside call.
Independent test: `cargo test --manifest-path rust/Cargo.toml` — green with web untouched.
- [ ] T009 Task routes: /tasks, transitions, transition, comment, draft (B5, B6, B11) — files: rust/crates/worklog-core/src/daemon_tasks.rs, rust/crates/worklog-core/src/daemon_tasks_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_tasks` — after: T002, T006, T008
- [ ] T010 Week routes: /tempo/pull, /weeks/:monday/closeout — files: rust/crates/worklog-core/src/daemon_week.rs, rust/crates/worklog-core/src/daemon_week_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_week` — after: T003, T004, T007, T009

## Phase 3 — Screens
Goal: My Tasks page and the week close-out panel work end to end, reachable from the day and week headers.
Independent test: `cd web && bun test && bun run typecheck` — green with the daemon mocked.
- [ ] T011 Web types, daemon client and Server Actions for the hub — files: web/lib/types.ts, web/lib/daemonHub.ts, web/lib/daemon.ts, web/app/actions-hub.ts, web/app/actions-hub.test.ts — verify: `cd web && bun test app/actions-hub.test.ts && bun run typecheck` — after: T010
- [ ] T012 My Tasks page: cards, status menu, comment box, Draft with AI (B13) — files: web/app/tasks/page.tsx, web/components/TaskBoard.tsx, web/components/TaskCard.tsx, web/components/TaskBoard.test.tsx, web/app/globals.css — verify: `cd web && bun test components/TaskBoard.test.tsx && bun run typecheck` — after: T011
- [ ] T013 [P] Week close-out panel, gap flag and Sync week loop (B12, B15, B17) — files: web/components/WeekCloseout.tsx, web/components/WeekCloseout.test.tsx, web/lib/weekSync.ts, web/lib/weekSync.test.ts, web/app/week/[monday]/page.tsx — verify: `cd web && bun test lib/weekSync.test.ts components/WeekCloseout.test.tsx && bun run typecheck` — after: T012
- [ ] T014 [P] My Tasks links in day and week headers (B14) — files: web/components/DayHeader.tsx, web/components/WeekHeader.tsx, web/components/DayHeader.test.tsx — verify: `cd web && bun test components/DayHeader.test.tsx` — after: T012
- [ ] T015 Close-out panel styles — files: web/app/globals.css — verify: `cd web && bun run build` — after: T013
- [ ] CHK016 human-verify My Tasks and week close-out against real Jira and Tempo — files: web/app/tasks/page.tsx, web/components/WeekCloseout.tsx — verify: human: user moves one ticket's status and posts an AI-drafted comment (both show in Jira), and a second Sync week reports 0 synced

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
