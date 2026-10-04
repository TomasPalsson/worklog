Approved: 2026-10-04 by user
Base: e85e7da
# Tasks — Jira assistant
Spec: spec.md · Design: design.md · Base: e85e7da · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a GENAI ticket in To Do, when it is started, then Jira gets one In Progress transition (FR-02) | T006 | ticket_assist_test::start_moves_todo |
| B2 (P0) | Given a non-GENAI or already-started ticket, when it is started, then no transition is sent (FR-03) | T006 | ticket_assist_test::start_leaves_others |
| B3 (P0) | Given any start, hint or web load, then no Done transition is ever sent (FR-04) | T006, T010 | ticket_assist_test::start_never_done, status_hints_test::hints_are_read_only |
| B4 (P0) | Given a status name, when moved, then the matching transition is sent; non-GENAI refused (FR-05) | T006 | ticket_assist_test::move_by_name |
| B5 (P0) | Given an allowed account, when created, then Jira gets a Story with the bare-number account and it moves to In Progress (FR-07) | T006 | ticket_assist_test::create_then_start |
| B6 (P0) | Given an account not in the fresh list, an empty/failed list, a non-GENAI project, or emoji text, when created, then it is refused before Jira create; a failed move after create reports the key and never re-creates (FR-08, FR-09, FR-11, FR-24, FR-25) | T006 | ticket_assist_test::create_refusals |
| B7 (P0) | Given markdown with headings, bullets, checkboxes, paragraphs, then the ADF keeps each shape (FR-10) | T002 | ticket_text_test::adf_shapes |
| B8 (P0) | Given a seeded clue log, when suggesting, then ≤3 allowed accounts come back best-first with clues and counts (FR-12) | T005 | account_clues_test::suggest_ranks |
| B9 (P0) | Given 200 accounted tickets, when relearning, then clues and counts are rebuilt (FR-13) | T005 | account_clues_test::relearn_rebuilds |
| B10 (P0) | Given a creation (guessed or named account), then a decision row is logged; a clue wrong twice stops counting (FR-14, FR-15, FR-23) | T005 | account_clues_test::wrong_twice_drops |
| B11 (P0) | Given a merged PR naming an In Progress GENAI key, then a Done hint is listed (FR-16, FR-17) | T009, T010 | github_test::records_merged_at, status_hints_test::merged_pr_hint |
| B12 (P0) | Given a card with a Done hint, then My Tasks shows the chip; click + confirm moves it (FR-18) | T012 | TaskDoneHint.test.tsx |
| B13 (P0) | Given pending Done hints, when a Claude Code session starts, then they are printed (FR-21) | T013 | cli session-hint test |

## Phase 1 — Foundations
Goal: shared types, ticket text converter and the clue tables exist.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ticket_text db_test` — green with nothing else built.
- [ ] T001 Contract + module registration: copy `.specs/013-jira-assistant/contract.rs` verbatim (minus its two T001 lines) and add `pub mod` lines plus one-line doc-comment stub files for ticket_text, account_clues, ticket_assist, status_hints — files: rust/crates/worklog-core/src/jira_assist_contract.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/ticket_text.rs, rust/crates/worklog-core/src/account_clues.rs, rust/crates/worklog-core/src/ticket_assist.rs, rust/crates/worklog-core/src/status_hints.rs — verify: `cargo check --manifest-path rust/Cargo.toml`
- [ ] T002 [P] Markdown→ADF converter (headings, bullets, `- [ ]` task lists, paragraphs, **bold**, links) and `has_emoji` (B7) — files: rust/crates/worklog-core/src/ticket_text.rs, rust/crates/worklog-core/src/ticket_text_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ticket_text` — after: T001
- [ ] T003 [P] Clue tables `account_clues`, `account_decisions`, `account_ticket_counts`; SCHEMA_VERSION 19→20 — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db_test` — after: T001

## Phase 2 — Jira hands and the clue log
Goal: core can start, move and create GENAI tickets and suggest an account.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ticket_assist account_clues` — green with no daemon or CLI change.
- [ ] T004 Jira additions: `allowed_accounts_with` (createmeta allowed values for the account field), `search_accounted_with` (JQL `project = GENAI AND "Account" is not EMPTY ORDER BY created DESC`, ≤200), `fetch_account_with`; `create_issue_with` sends description through `markdown_to_adf` — files: rust/crates/worklog-core/src/collectors/jira.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core collectors::jira` — after: T002
- [ ] T005 [P] Clue log: `relearn`, `suggest` (empty log → caller relearns first), `record_decision` with the 2-wrong drop rule (B8, B9, B10) — files: rust/crates/worklog-core/src/account_clues.rs, rust/crates/worklog-core/src/account_clues_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core account_clues` — after: T003
- [ ] T006 Start / move / assist-create with the GENAI guard, fresh allowed-list check (empty → refuse), emoji refusal, never-auto-Done, no create retry after a failed move (B1–B6) — files: rust/crates/worklog-core/src/ticket_assist.rs, rust/crates/worklog-core/src/ticket_assist_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core ticket_assist` — after: T004, T005

## Phase 3 — Status hints data
Goal: worklog knows which In Progress GENAI tickets have a merged PR.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core status_hints github` — green with no UI.
- [ ] T009 [P] GitHub collector records `merged_at` on `RawRecord::Commit` and runs a second `merged:<window>` PR query (B11) — files: rust/crates/worklog-core/src/collectors/github.rs, rust/crates/worklog-core/src/collectors/github_test.rs, rust/crates/worklog-core/src/clues_contract.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core github` — after: T001
- [ ] T010 `done_hints(conn)`, DB only (B3, B11) — files: rust/crates/worklog-core/src/status_hints.rs, rust/crates/worklog-core/src/status_hints_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core status_hints` — after: T009

## Phase 4 — Daemon + CLI
Goal: `worklog ticket get|start|find|move|create|hints` and `worklog account suggest|relearn` work end to end against the daemon.
Independent test: `cargo test --manifest-path rust/Cargo.toml` — green; `worklog ticket --help` lists every verb.
- [ ] T007 Daemon routes `GET /tickets/:key/view`, `POST /tickets/:key/start`, `POST /tickets/:key/move`, `POST /tickets/assist-create`, `GET /accounts/allowed`, `POST /accounts/suggest`, `POST /accounts/relearn`, `GET /hints` — files: rust/crates/worklog-core/src/daemon_assist.rs, rust/crates/worklog-core/src/daemon_assist_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_assist` — after: T006, T010
- [ ] T008 CLI `worklog ticket` (get, start, find, move, create --summary --description-file --account [--guess] [--clue]…, hints) and `worklog account` (allowed, suggest, relearn); human text by default, `--json` flag — files: rust/crates/worklog-cli/src/ticket_cmd.rs, rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli ticket_cmd` — after: T007
- [ ] T013 Session start prints pending Done hints after the pin text (stdout, never fails) (B13) — files: rust/crates/worklog-cli/src/cli.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-cli session_hint` — after: T008, T010

## Phase 5 — Web chip
Goal: My Tasks shows "Move to Done?" on cards with a merged PR.
Independent test: `cd web && bun test TaskDoneHint && bun run typecheck` — green.
- [ ] T011 `TaskRow.done_hint: Option<StatusHint>` filled from `done_hints` — files: rust/crates/worklog-core/src/tempo_hub_contract.rs, rust/crates/worklog-core/src/task_board.rs, rust/crates/worklog-core/src/task_board_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core task_board` — after: T010
- [ ] T012 Chip + confirm using existing `loadTransitions` and transition action, picks the Done-category transition (B12) — files: web/lib/types.ts, web/components/TaskDoneHint.tsx, web/components/TaskDoneHint.test.tsx, web/components/TaskCard.tsx — verify: `cd web && bun test TaskDoneHint && bun run typecheck` — after: T011

## Phase 6 — Skill
Goal: Claude Code routes the Owner's words to the commands and writes good ticket text.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core skill` — green; `worklog skill install` writes references/jira.md.
- [ ] CHK001 human-pick the ticket text format from 2 rendered samples (same ticket, two styles) — files: .specs/013-jira-assistant/NOTES.md — verify: human: user names sample 1 or 2 — after: T002
- [ ] T014 Skill routing rows + `references/jira.md`: work on KEY → `ticket start`; work on text → `ticket find` → pick or create flow (one confirm: title + account, D-07/D-12); waiting → ask Blocked; finished / merged → ask Done; "what can I close?" → `ticket hints`; skill test greps every routing phrase plus "English" and "no emoji"; never Slack; text guide from CHK001 — files: skills/worklog/SKILL.md, skills/worklog/references/jira.md, rust/crates/worklog-core/src/skill.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core skill` — after: T008, CHK001

## Gates
- [ ] CHK002 human-verify the spec's launch check live: fresh Claude Code chat, "let's work on GENAI-9129" shows it and moves it to In Progress; "work on the Innnes SSO thing" gives one yes/no and a new well-formatted GENAI ticket appears In Progress with the account set — files: . — verify: human: user confirms both tickets look right in Jira — after: T014, T012, T013
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
