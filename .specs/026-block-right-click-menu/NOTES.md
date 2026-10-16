# Notes — 026 block right-click menu

- Minor: T001 no new RED test; pure move covered by existing BlockCard Ignore/Duplicate tests.
- Minor: T003 commit() missed `ask::refresh`, so search kept the old description — fixed by orchestrator with test `reindexes_the_block_for_ask`.
- Discovered: rust/crates/worklog-core/src/undo.rs:12 — BlockChange / UndoOutcome are imported privately; callers must use daily_helpers_contract paths — defer (friction only)
- Assumption: [mechanical] T003 cuts a >500-char reply at a word boundary (spec FR-07a), unlike note_writer which errors.
