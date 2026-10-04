Approved: 2026-10-04 by user
Base: 430299a
# Tasks — Claude Code terminal mod for worklog
Spec: spec.md · Design: design.md · Base: 430299a · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && claude plugin test mods/worklog`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a cwd on branch `PROJ-7-x` and no key in prompt or path, when hook-run handles an event, then it carries `PROJ-7` | T001 | hook_uses_branch_ticket_as_fallback |
| B2 (P0) | Given prompt `PROJ-42` on branch `PROJ-7-x`, when hook-run handles it, then it carries `PROJ-42` | T001 | hook_prefers_prompt_ticket_over_branch |
| B3 (P0) | Given `worklog hook install` ran twice, then the mod files exist once and the plugin-dir key holds the path once, other values kept | T002 | install_writes_files_and_registers_once, install_appends_to_existing_value |
| B4 (P0) | Given today has work and personal blocks, when 60 s pass, then the status line shows only work hours | T004 | status.test.ts |
| B5 (P1) | Given Monday and Friday unsynced in a work folder, when two sessions start, then exactly one toast shows | T004 | status.test.ts |
| B6 (P0) | Given a work folder on `GENAI-9-x`, when a commit attribution is composed, then it ends with `Ticket: GENAI-9` | T005 | ticket.test.ts |
| B7 (P0) | Given `/worklog review` and a picked block, when `t` + `GENAI-1` + Enter, then `POST /blocks/<id>/ticket {"jira_issue":"GENAI-1"}` is sent | T006 | command.test.tsx |

## Phase 1 — hook-run reads the branch
Goal: Claude Code events on a ticket branch carry that ticket, with or without the mod.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core hook_run` — green with no mod present.
- [x] T001 Branch as last ticket source in hook-run (FR-01, FR-02; B1, B2) — files: rust/crates/worklog-core/src/hook_run.rs, rust/crates/worklog-core/src/hook_run_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core hook_run` — done: b8796a0

## Phase 2 — the mod
Goal: inside a Claude Code terminal session the Owner sees hours, reminders and the ticket, and can review today with `/worklog`.
Independent test: `claude plugin validate mods/worklog && claude plugin test mods/worklog` — green with Rust untouched.
- [x] T003 Mod skeleton, shared lib and registrar stubs (FR-03b, FR-07, FR-10b, FR-04 timeout) — files: mods/worklog/.claude-plugin/plugin.json, mods/worklog/hooks/hooks.json, mods/worklog/hooks/register.tsx, mods/worklog/hooks/lib.ts, mods/worklog/hooks/lib.test.ts, mods/worklog/hooks/status.ts, mods/worklog/hooks/ticket.ts, mods/worklog/hooks/command.tsx — verify: `claude plugin validate mods/worklog && claude plugin test mods/worklog` — done: 2245d6b
- [x] T004 [P] Hours status line and daily reminder (FR-03, FR-04, FR-05, FR-06, FR-06b, FR-06c; B4, B5) — files: mods/worklog/hooks/status.ts, mods/worklog/hooks/status.test.ts — verify: `claude plugin test mods/worklog` — after: T003 — done: 83dc33b
- [x] T005 [P] Branch ticket toast, context and trailer (FR-08, FR-09, FR-10, FR-11; B6) — files: mods/worklog/hooks/ticket.ts, mods/worklog/hooks/ticket.test.ts — verify: `claude plugin test mods/worklog` — after: T003 — done: 36a7080
- [x] T006 [P] /worklog command and review pane (FR-12, FR-13, FR-14; B7) — files: mods/worklog/hooks/command.tsx, mods/worklog/hooks/command.test.tsx — verify: `claude plugin test mods/worklog` — after: T003 — done: ec26c11

## Phase 3 — ship it with worklog
Goal: `worklog hook install` puts the mod in place and Claude Code loads it in every session.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core claude_mod` — green.
- [x] T002 Embed the mod and enable it from hook install/uninstall/status (FR-15, FR-16, FR-17; B3) — files: rust/crates/worklog-core/src/claude_mod.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/hook.rs, rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-cli/src/wizard.rs, CLAUDE.md — verify: `cargo test --manifest-path rust/Cargo.toml && cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` — after: T004, T005, T006 — done: 3474d2d
- [ ] CHK001 human-verify the mod loads from install — files: rust/crates/worklog-core/src/claude_mod.rs — verify: human: after `worklog hook install`, a new `claude` session under ~/Desktop/Work/ on a `GENAI-…` branch shows `worklog <H>h<MM>` and a ticket toast, and `/worklog review` opens the pane — after: T002

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
