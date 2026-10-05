# Notes — 016

- Discovered: mods/worklog/hooks/lib.ts:58 — recent-task labels carry the full Jira summary; a long one makes a long dialog option — fold into T002 (truncate to ~60 chars)
- Discovered: mods/worklog/hooks/lib.ts:12 — daemonGet times out at 2000 ms, spec §5 caps the /tasks lookup at 1.5 s — fold into T002 (own 1500 ms race)
- Discovered: skills/worklog/references/jira.md:31 — skill keys on instructions "create a ticket" / "find the ticket for: <text>"; T005's context wording must contain those phrases — fold into T005
- Discovered: rust/crates/worklog-core/src/skill.rs — phrase tests pass on old and new jira.md alike (no test pins the new flow) — defer
- Ruling: CHK001 was missing after: T005, T003 (router offered it before the mod code existed); added the ordering only — 2026-10-05
- Discovered: mods/worklog/hooks/lib.ts:60 — ticketChoices emits uncut labels; the 60-char cut lives in ticket.ts, so any other caller gets long labels — defer
- Discovered: mods/worklog/hooks/ticket.ts:61 — typed Other text with surrounding spaces (" GENAI-9 ") is not trimmed, so it records nothing — defer
- Discovered: mods/worklog/hooks/ticket.ts:77 — picking the branch ticket toasts `worklog: KEY` twice (start toast + record toast) — defer
- Discovered: mods/worklog/hooks/ticket.ts:72 — FR-07 "until a ticket is recorded" only holds for keys the mod records; when Claude runs `worklog ticket use` itself the hand-off stays in context (risk: a second ticket created) — fold into T006 (watch Claude's Bash `tool.call` for a successful `worklog ticket use KEY --session <id>`)
- Discovered: mods/worklog/hooks/ticket.ts:104 — the clear re-ask relies on `$.session.id()` already returning the new id inside `session.end`; only the mock proves it — CHK001 should watch for it
- Ruling: added T006 (FR-07 gap found in T005) and CHK001 after: T006 so the human check covers it — 2026-10-05
- Discovered: mods/worklog/hooks/ticket.test.ts:140 — `const QUESTION =` is unused and missing a space — defer (gates may flag it)
- Discovered: mods/worklog/hooks/ticket.test.ts:57 — `seat` doesn't mock `command.register`, so every run prints a "session.start hook was skipped" line — defer
- Discovered: main moved to c5fc10e (PR #106) and also edits rust/crates/worklog-cli/src/cli.rs — the PR may need a merge from main before it lands; not rebased here so task shas stay valid — defer
- Discovered: skills/worklog/references/jira.md:36 — says the session id comes from "the hint", but ticket_hint no longer prints it (review score 68, below keep bar) — defer
