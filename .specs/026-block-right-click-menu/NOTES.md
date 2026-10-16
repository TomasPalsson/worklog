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
