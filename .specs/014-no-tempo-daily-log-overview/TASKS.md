Approved: 2026-10-04 by user
# Tasks — Logged: see what's really in Tempo, one home for Jira/Tempo
Spec: spec.md · Design: design.md · Base: 67fc0ec · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a past day with 5h of 8h required and no dismissal, when the range is read, then its state is `under` (FR-09) | T001 | logged_contract day_state tests |
| B2 (P0) | Given required 0, unknown, or today/later, when read, then not `under` (FR-10) | T001 | logged_contract day_state tests |
| B3 (P0) | Given no rows for a day, when read, then `not_fetched` (FR-13) | T003 | logged_test.rs |
| B4 (P0) | Given a stored range, when re-pulled without an entry, then that entry is gone (FR-07) | T003 | tempo_remote_test.rs |
| B5 (P0) | Given Tempo returns 500, when `/logged/pull` runs, then 502 and stored rows unchanged (FR-08) | T004 | daemon_logged_test.rs |
| B6 (P0) | Given a 43-day range, when requested, then 400 (§5) | T004 | daemon_logged_test.rs |
| B7 (P0) | Given a dismiss with "dentist", when read back, then `dismissed` + reason; no Tempo call (FR-11, FR-12) | T004 | daemon_logged_test.rs |
| B8 (P0) | Given an empty or 81-char reason, when dismissed, then 400 and nothing stored (FR-11) | T004 | daemon_logged_test.rs |
| B9 (P0) | Given a Logged view, when opened, then stored data renders and a refresh fetch runs (FR-05, FR-06) | T009 | LoggedFetch.test.tsx |
| B10 (P0) | Given month/week/day pages, when rendered, then days, totals, entries and prev/next links show (FR-01–04) | T009 | Logged*.test.tsx |
| B11 (P0) | Given any page, when rendered, then one menu Day · Week · Tasks · Logged · Settings · Billing with the current one marked (FR-14) | T010 | AppNav.test.tsx |
| B12 (P0) | Given Day/Week/Tasks, when rendered, then no billing UI (FR-15) | T011, T012 | DayHeader.test.tsx, TaskHours.test.tsx |
| B13 (P1) | Given a dismissed day, when undismissed, then `under` again (FR-17) | T004 | daemon_logged_test.rs |

## Phase 1 — Tempo range stored and served
Goal: the daemon serves any ≤42-day range of real Tempo entries with a per-day state, fetches it from Tempo on request, and stores dismissals.
Independent test: `cargo test --manifest-path rust/Cargo.toml logged` — green with the web untouched.
- [ ] T001 Contract files and `day_state` (orchestrator-owned) (B1, B2) — files: rust/crates/worklog-core/src/logged_contract.rs, rust/crates/worklog-core/src/lib.rs, web/lib/logged_contract.ts — verify: `cargo test --manifest-path rust/Cargo.toml logged_contract`
- [ ] T002 [P] Dismissals table `tempo_day_dismissals` in schema, no version bump — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml db_test`
- [ ] T003 Range store (`store_range`, `store_week` delegates) and `logged_range` / `dismiss` / `undismiss` reader (B3, B4; also pins weekend/holiday schedule rows per spec A11, and no-stored-data + Tempo failure per FR-08) — files: rust/crates/worklog-core/src/tempo_remote.rs, rust/crates/worklog-core/src/tempo_remote_test.rs, rust/crates/worklog-core/src/logged.rs, rust/crates/worklog-core/src/logged_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml tempo_remote logged` — after: T001, T002
- [ ] T004 Daemon routes GET /logged, POST /logged/pull, /logged/dismiss, /logged/undismiss (B5–B8, B13) — files: rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/daemon_logged.rs, rust/crates/worklog-core/src/daemon_logged_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml daemon_logged` — after: T003

## Phase 2 — The Logged section
Goal: the Owner opens /logged and sees month, week and day views of real Tempo time, auto-refreshed, with short days flagged and dismissable.
Independent test: `cd web && bun test components/Logged app/actions-logged lib/format` — green with the daemon mocked.
- [ ] T005 [P] Web client `daemonLogged.ts` + server actions `actions-logged.ts` — files: web/lib/daemonLogged.ts, web/app/actions-logged.ts, web/app/actions-logged.test.ts — verify: `cd web && bun test app/actions-logged.test.ts` — after: T001
- [ ] T006 [P] Month helpers in format.ts: `monthOf`, `shiftMonth`, `monthGrid` (Mon-start, ≤42 days) — files: web/lib/format.ts, web/lib/format.test.ts — verify: `cd web && bun test lib/format.test.ts`
- [ ] T007 Run `/design:design` for the Logged month/week/day views, the dismiss control and the shared top menu; write the chosen design to ui.md — files: .specs/014-no-tempo-daily-log-overview/ui.md — verify: `test -s .specs/014-no-tempo-daily-log-overview/ui.md`
- [ ] CHK008 human-verify the Logged + menu design before it is built — files: .specs/014-no-tempo-daily-log-overview/ui.md — verify: human: user picks a design and says "go" — after: T007
- [ ] T009 Logged pages and components per ui.md: /logged redirect, /logged/month/[month], /logged/week/[monday], /logged/day/[day], LoggedFetch, DismissDay (B9, B10) — files: web/app/logged/page.tsx, web/app/logged/month/[month]/page.tsx, web/app/logged/week/[monday]/page.tsx, web/app/logged/day/[day]/page.tsx, web/components/LoggedMonth.tsx, web/components/LoggedWeek.tsx, web/components/LoggedDay.tsx, web/components/LoggedFetch.tsx, web/components/DismissDay.tsx, web/components/Logged.test.tsx, web/app/globals.css — verify: `cd web && bun test components/Logged.test.tsx && bun run typecheck` — after: T004, T005, T006, CHK008

## Phase 3 — One menu, Billing tucked away
Goal: every page shares one menu, and Day, Week and Tasks carry no Billing UI; Billing lives only on /billing.
Independent test: `cd web && bun test components/AppNav components/DayHeader components/TaskHours components/ChangeNotices` — green.
- [ ] T010 Shared menu `AppNav` (usePathname; Day · Week · Tasks · Logged · Settings · Billing; theme toggle) mounted once in layout (B11) — files: web/components/AppNav.tsx, web/components/AppNav.test.tsx, web/app/layout.tsx, web/app/globals.css — verify: `cd web && bun test components/AppNav.test.tsx` — after: T009
- [ ] T011 [P] Strip page headers to their date controls; drop the Tickets/Billing toggle and billing branch from the day page; move ExportPanel onto /billing (B12) — files: web/components/DayHeader.tsx, web/components/DayHeader.test.tsx, web/components/WeekHeader.tsx, web/app/[day]/page.tsx, web/app/tasks/page.tsx, web/app/billing/page.tsx — verify: `cd web && bun test components/DayHeader.test.tsx && bun run typecheck` — after: T010
- [ ] T012 [P] Remove billing leaks: TaskHours "Not invoiced / Go to billing", BlockCard customer-split mount and customer-move toast, billing change notices outside /billing (B12) — files: web/components/TaskHours.tsx, web/components/TaskHours.test.tsx, web/components/BlockCard.tsx, web/components/ChangeNotices.tsx, web/components/ChangeNotices.test.tsx — verify: `cd web && bun test components/TaskHours.test.tsx components/ChangeNotices.test.tsx` — after: T010
- [ ] CHK013 human-verify V-01 on the running app — files: . — verify: human: user opens Logged for this month, hours match Tempo's website, a short day is flagged, "dentist" dismissal survives reload, Day page shows no Billing — after: T011, T012

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
