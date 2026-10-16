# Notes — 026 block right-click menu

- Minor: T001 no new RED test; pure move covered by existing BlockCard Ignore/Duplicate tests.
- Minor: T003 commit() missed `ask::refresh`, so search kept the old description — fixed by orchestrator with test `reindexes_the_block_for_ask`.
- Discovered: rust/crates/worklog-core/src/undo.rs:12 — BlockChange / UndoOutcome are imported privately; callers must use daily_helpers_contract paths — defer (friction only)
- Assumption: [mechanical] T003 cuts a >500-char reply at a word boundary (spec FR-07a), unlike note_writer which errors.
- Minor: T002 "Describe with Claude" menu item shows even where the card hides its sparkle button (shouldShowSparkles false); runs the same handler.
- Discovered: web/components/BlockCard.tsx:~20-28 — `billingCustomer` prop declared in Props but never used — defer
- Discovered: web/components/BlockCard.tsx — at ~395 of the 400-line size guard; T005/T006 must not grow it — defer
- Minor: T004 BoxedInvoker wrapper exists only because redescribe::invoke lacks `I: ?Sized` (redescribe.rs:74); prepare errors mapped to 400/404 by message text.
- Assumption: [mechanical] T004 model failure / blank reply returns 502 (contract names no status); reuses NOTE_MODEL and NOTE_THINKING_TOKENS.
- Minor: T005 `.block-context-menu-instruction` box has no CSS (globals.css not in T005 files) — fold into gating fix.
- Minor: T005 InstructionBox.submit has no try/finally; a thrown server action leaves "Writing…" up (Esc still closes).
- Minor: T005 reason strings duplicated from redescribe_contract.rs into BlockContextMenu.tsx (match today); stale doc comment at BlockContextMenu.tsx:~20 omits Regenerate.
- Discovered: web/components/BlockContextMenu.tsx — menu opens at the pointer with no viewport clamp; near the window bottom its last items are off-screen (verify/CHK001-2-menu.jpg) — fold into gating fix
