Approved: 2026-10-08 by user
# Tasks — Add a ticket block from the day page, AI writes its description
Spec: spec.md · Design: design.md · Base: 6ada5b4 · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Behaviors
| ID | Given / When / Then | Task | Proven by |
|----|---------------------|------|-----------|
| B1 (P0) | Given a valid form, when saved, then a manual block with the note as description and `description_origin = note` exists (FR-03, FR-04) | T002 | note_writer_test::log_note_block_creates_note_block |
| B2 (P0) | Given a note block, when the AI commits, then description = model text, origin = ai, rough_note unchanged (FR-05) | T002 | note_writer_test::commit_writes_ai_text |
| B3 (P0) | Given a note block, when hand-edited, then origin = hand (FR-06) | T002 | note_writer_test::hand_edit_marks_hand |
| B4 (P0) | Given a hand-edited block, when an unforced AI commit lands, then the text is unchanged and the job fails "hand-edited" (FR-07) | T002 | note_writer_test::commit_skips_hand_edit |
| B12 (P0) | Given 23:30 + 60 min, when saved, then refused, no block (FR-02b); an empty or >500-char model reply fails the job, text unchanged (FR-14) | T002 | note_writer_test::rejects_past_midnight, note_writer_test::rejects_bad_model_reply |
| B5 (P0) | Given a synced note block, when the AI commits, then it is dirty (FR-10) | T002 | note_writer_test::commit_marks_synced_dirty |
| B6 (P0) | Given a note block, when regenerate is posted, then status runs to done; a second post while running returns started:false (FR-08) | T003 | daemon_note_block_test::regenerate_runs_to_done |
| B7 (P0) | Given the day page, when + is clicked and the form saved, then addNoteBlock is called with the typed values; invalid fields (incl. ticket key) and daemon errors show messages and keep the typed values (FR-01, FR-02, FR-02a) | T005 | AddNoteBlock.test.tsx |
| B8 (P0) | Given a running job, when status turns done, then the page refreshes; polling stops at 30 s (FR-11) | T005 | useNoteJob.test.ts |
| B9 (P0) | Given origin = hand, when Regenerate is clicked, then "Replace your edit?" is asked before any call (FR-09) | T006 | useNoteRegenerate.test.ts |
| B10 (P1) | Given a day with blocks, when + opens, then start defaults to the last block's end, else 09:00 (FR-12) | T005 | AddNoteBlock.test.tsx |
| B11 (P1) | Given a running job, then the block shows "Writing…" (FR-13) | T006 | useNoteRegenerate.test.ts |

## Phase 1 — Note blocks in the daemon
Goal: the daemon can save a note block and have the AI rewrite its description in the background, never over a hand edit.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core note_` — green with the web untouched.
- [ ] T001 Note columns on blocks — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/db_test.rs, rust/crates/worklog-core/src/models.rs, rust/crates/worklog-core/src/repo.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core db_test`
- [ ] T002 Note writer core: save, prompt, guarded commit, hand-edit marker (B1–B5, B12) — files: rust/crates/worklog-core/src/note_writer.rs, rust/crates/worklog-core/src/note_writer_test.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/block_service.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core note_writer` — after: T001
- [ ] T003 Daemon routes and note jobs (B6) — files: rust/crates/worklog-core/src/daemon_note_block.rs, rust/crates/worklog-core/src/daemon_note_block_test.rs, rust/crates/worklog-core/src/daemon.rs, rust/crates/worklog-core/src/line_text_jobs.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_note_block` — after: T002

## Phase 2 — "+" and Regenerate on the day page
Goal: the Owner adds a note block from the day page and sees the AI text land, and can regenerate it.
Independent test: `cd web && bun test components/AddNoteBlock.test.tsx components/useNoteJob.test.ts components/useNoteRegenerate.test.ts app/actions-note-block.test.ts && bun run typecheck` — green with the daemon mocked.
- [ ] T004 [P] Web client and server actions — files: web/lib/daemonNoteBlock.ts, web/app/actions-note-block.ts, web/app/actions-note-block.test.ts — verify: `cd web && bun test app/actions-note-block.test.ts`
- [ ] T005 "+" form and job polling (B7, B8, B10) — files: web/components/AddNoteBlock.tsx, web/components/AddNoteBlock.test.tsx, web/components/useNoteJob.ts, web/components/useNoteJob.test.ts, web/components/ActionBar.tsx, web/app/[day]/page.tsx, web/app/globals.css — verify: `cd web && bun test components/AddNoteBlock.test.tsx components/useNoteJob.test.ts` — after: T004
- [ ] T006 Regenerate on note blocks (B9, B11) — files: web/components/useNoteRegenerate.ts, web/components/useNoteRegenerate.test.ts, web/components/BlockCard.tsx, web/lib/group-actions.ts, web/lib/group-actions.test.ts — verify: `cd web && bun test components/useNoteRegenerate.test.ts lib/group-actions.test.ts` — after: T005
- [ ] CHK001 human-verify the whole flow on the real day page — files: web/components/AddNoteBlock.tsx — verify: human: on the day page, click +, enter 30 min, pick a ticket, type "fixed login bug" → the block appears at once; within ~10 s its description becomes a proper sentence; Regenerate gives new text; the existing Send to Tempo sends it — after: T003, T006

## Gates
- [ ] G001 project gates clean — files: . — verify: `flow check --fix --since 6ada5b4`
- [ ] G002 branch review clean — files: . — verify: `flow pass`
- [ ] G003 verification evidence exists — files: . — verify: `test -s verify/`
