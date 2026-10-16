# Notes — 027 super block regenerate with prompt

- Ruling: flow:build-slices child workflow returned no result for every task (twice, nothing dispatched); wave 2 dispatched inline as three developer subagents instead.
- Discovered: web/lib/daemonTempoLines.ts:1 — header comment names only spec 011, not the 027 redescribe addition (cosmetic) — defer
- Discovered: web/components/BlockContextMenu.tsx:36 — useBlockContextMenu is block-named but now also drives the TicketGroup header (naming only) — defer
- Discovered: web/components/TicketGroup.tsx:310 — JSX after the editing ternary has inconsistent indentation — defer
- Assumption: [mechanical] the group menu opens on the Unassigned header and shows its one item disabled (FR-11 says disabled, not absent)
- Assumption: [taste] INSTRUCTION_RULE_IS wording (Icelandic) written by the T003 developer; design named the constant but not its text
- Discovered: rust/crates/worklog-core/src/tempo_lines.rs:357 — set_check only updates generated rows, so a redescribed (manual) line keeps check_status NULL — defer
- Discovered: rust/crates/worklog-core/src/daemon_redescribe_test.rs:93 — process-wide REPLY_LOCK/TEST_REPLY statics held across awaits; fragile if tests multiply — defer
- Assumption: [mechanical] route-level 409 is tested through commit_error directly; the mid-call text change is covered by the T003 core test
- Discovered: CHK001 run 1 — Regenerate with prompt on a super block timed out at the 10s default daemon timeout (real model call) — fold into T007
- Discovered: a freshly built (unsigned) debug daemon blocks indefinitely in SecKeychainFindGenericPassword (estimator provider, Jira, Mirres secrets) with no timeout; requests hang until killed — defer
- Ruling: review-diff ran with cwd web/, so its receipt-check dismissed every finding as "file not found at head"; findings triaged by the orchestrator instead — 3 fixed in one gate-fix dispatch (editable-target right-click, useMenuClamp lint warning, tempo_line_redescribe module doc), 5 kept as minors
- Minor: daemon_tempo_line_redescribe.rs — third copy of the TEST_REPLY static + real_invoker seam (daemon_redescribe.rs, daemon_note_block.rs); follows the existing pattern
- Minor: daemon_tempo_line_redescribe.rs — own not_found closure instead of daemon_tempo_lines::line_or_not_found; glob import of redescribe_contract::*
- Minor: tempo_line_redescribe.rs — prepare() trims the instruction again after the route trimmed it; LineRedescribePrep.instruction is read only by tests
- Minor: GroupContextMenu.tsx — `boxOpen && lineKey` re-checks lineKey only to narrow the type
- Discovered: web/components/GroupContextMenu.test.tsx:34 — open helper renders `lineKey={lineKey ?? undefined}x={10}` with no space (works, reads like a typo) — defer
