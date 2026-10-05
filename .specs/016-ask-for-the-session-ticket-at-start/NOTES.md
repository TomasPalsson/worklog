# Notes — 016

- Discovered: mods/worklog/hooks/lib.ts:58 — recent-task labels carry the full Jira summary; a long one makes a long dialog option — fold into T002 (truncate to ~60 chars)
- Discovered: mods/worklog/hooks/lib.ts:12 — daemonGet times out at 2000 ms, spec §5 caps the /tasks lookup at 1.5 s — fold into T002 (own 1500 ms race)
- Discovered: skills/worklog/references/jira.md:31 — skill keys on instructions "create a ticket" / "find the ticket for: <text>"; T005's context wording must contain those phrases — fold into T005
- Discovered: rust/crates/worklog-core/src/skill.rs — phrase tests pass on old and new jira.md alike (no test pins the new flow) — defer
- Ruling: CHK001 was missing after: T005, T003 (router offered it before the mod code existed); added the ordering only — 2026-10-05
