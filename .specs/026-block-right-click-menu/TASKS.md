Approved: 2026-10-16 by user
Verified: 2026-10-16 by user (pre-approved: /flow:next --finish)
# Tasks — Right-click menu on blocks
Spec: spec.md · Design: design.md · Base: 6a168ef · Route: dispatch · Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Behaviors
| ID | Given / When / Then | Task | Proven by | Key |
|----|---------------------|------|-----------|-----|
| FR-01 (P0) | Given a described, synced block, when it is redescribed, then only description (and dirty) change; duration, ticket, origin, estimated_by, Tempo id, exported_at are identical | T003 | redescribe_test::only_description_changes | blocks/redescribe-description-only |
| FR-02 (P0) | Given an instruction, when redescribe runs, then the model prompt contains the instruction and the current description | T003 | redescribe_test::prompt_carries_instruction | blocks/redescribe-prompt |
| FR-03 (P1) | Given two redescribes, when the second has a new instruction, then no trace of the first is sent or stored | T003 | redescribe_test::instruction_not_stored | blocks/redescribe-one-time |
| FR-04 (P0) | Given an empty or 501-char instruction, when posted, then 400 with the contract reason and the block is unchanged | T004 | daemon_redescribe_test::rejects_bad_instruction | blocks/redescribe-instruction-limits |
| FR-05 (P0) | Given a personal, note or undescribed block, when posted or menu opened, then 400 with reason and the item is disabled with that tooltip | T003, T004, T005 | redescribe_test::refuses_*, BlockContextMenu.test disabled-reasons | blocks/redescribe-refusals |
| FR-06 (P0) | Given a synced block, when redescribed, then it is dirty and its Tempo id is kept | T003 | redescribe_test::synced_goes_dirty | blocks/redescribe-dirty |
| FR-07 (P0) | Given a redescribed block, when Undo runs, then the old description and changed-flag are back | T003 | redescribe_test::undo_restores | blocks/redescribe-undo |
| FR-07a (P0) | Given a fake model reply that is empty or 600 chars, when redescribe runs, then empty fails with the block unchanged and 600 is cut at a word boundary to ≤500 with no ellipsis | T003 | redescribe_test::empty_reply_fails, redescribe_test::long_reply_cut | blocks/redescribe-output-limits |
| FR-07b (P0) | Given a running regenerate, when the Owner clicks outside, then the box stays open; it closes on success or Esc | T005 | BlockContextMenu.test pending-stays-open | blocks/redescribe-box-pending |
| FR-08 (P0) | Given the day page, when a card is right-clicked, then the block menu opens and the browser menu does not | T002 | BlockContextMenu.test opens-on-contextmenu | blocks/menu-opens |
| FR-09 (P1) | Given a focused card, when ContextMenu or Shift+F10 is pressed, then the menu opens; Esc closes it and focus returns to the card | T002 | BlockContextMenu.test keyboard | blocks/menu-keyboard |
| FR-10 (P0) | Given the menu, when Describe / Personal / Noise / Duplicate / Internal is picked, then the same action as the card's button runs | T001, T002 | BlockContextMenu.test mirrors-buttons | blocks/menu-mirrors-buttons |
| FR-11 (P0) | Given the menu, when Delete is picked, then deleteBlock runs and a toast offers Undo; on a synced or exported block Delete is disabled with its tooltip | T002 | BlockContextMenu.test delete-with-undo, delete-disabled-synced | blocks/menu-delete |
| FR-12 (P0) | Given a 120-min block, when Split… 45 is confirmed, then split is called with 45; 0 and 120 keep Confirm disabled | T006 | BlockContextMenu.test split, actions-block-menu.test split | blocks/menu-split |
| FR-13 (P0) | Given neighbours, when Merge with previous/next is picked, then merge keeps the clicked block as primary; disabled with no, synced or overlapping neighbour | T006 | BlockContextMenu.test merge | blocks/menu-merge |
| FR-14 (P1) | Given a synced or exported block, when the menu opens, then ignore and merge items are disabled | T002, T006 | BlockContextMenu.test synced-disabled | blocks/menu-synced-guard |
| FR-15 (P1) | Given a card rendered with hideTicketing, when right-clicked, then no block menu opens | T002 | BlockCard.test no-menu-when-hideTicketing | blocks/menu-day-page-only |

## Review focus
Icelandic text (þ, ð, æ) and 500/501-char instructions, counted in chars not bytes — T004, T005
Synced or exported blocks (tempo_worklog_id / exported_at set) — T003, T006

## Phase 1 — Right-click opens a menu with today's block actions
Goal: right-clicking a block on the day page opens a menu that runs Describe, Personal/Work, Ignore (Noise / Duplicate / Internal) and Delete through the existing server actions and daemon.
Independent test: `cd web && bun test components/BlockContextMenu.test.tsx components/BlockCard.test.tsx` — menu items call the real action modules' mocked daemon paths.
- [x] T000 Baseline: run the Test: command at Base into verify/baseline-tests.txt (exit code + failing test names) — files: .specs/026-block-right-click-menu/verify/baseline-tests.txt — verify: `test -s .specs/026-block-right-click-menu/verify/baseline-tests.txt` — done: fff68d3
- [x] T001 Prefactor: lift IgnoreMenu's onIgnore / onDuplicate handlers into a hook IgnoreMenu and the new menu both use, no behaviour change (FR-10) — files: web/components/useIgnoreActions.ts, web/components/IgnoreMenu.tsx — verify: `cd web && bun test components/BlockCard.test.tsx && bun run typecheck` — follow: web/components/useNoteRegenerate.ts — after: T000 — done: 1f6e070
- [x] T002 BlockContextMenu opened by right-click and keyboard, with Describe, Personal/Work, Noise, Duplicate, Internal, Delete+Undo; disabled when hideTicketing (FR-08, FR-09, FR-10, FR-11, FR-14, FR-15) — files: web/components/BlockContextMenu.tsx, web/components/BlockContextMenu.test.tsx, web/components/BlockCard.tsx, web/components/BlockCard.test.tsx, web/app/globals.css — verify: `cd web && bun test components/BlockContextMenu.test.tsx components/BlockCard.test.tsx && bun run typecheck` — follow: web/components/IgnoreMenu.tsx — after: T001 — done: 31cd331

## Phase 2 — Regenerate a description from the Owner's instruction
Goal: the Owner types an instruction in the menu and the block's description is rewritten by the AI, with hours and ticket untouched, through the core, the daemon route and the web action.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe && cd web && bun test components/BlockContextMenu.test.tsx app/actions-block-menu.test.ts` — route returns the fake model's description and only that column changed; the menu posts the instruction.
- [x] T003 Core redescribe: prepare / invoke / commit / redescribe_block_with writing only description + dirty under an undo step (FR-01, FR-02, FR-03, FR-05, FR-06, FR-07, FR-07a) — files: rust/crates/worklog-core/src/redescribe.rs, rust/crates/worklog-core/src/redescribe_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe` — no_claim: model is a FixedInvoker; real model wording not exercised — follow: rust/crates/worklog-core/src/note_writer.rs — after: T000 — done: f9f2aa4
- [x] T004 [P] Route POST /blocks/:id/redescribe with instruction validation and the fake-model test seam (FR-04, FR-05) — files: rust/crates/worklog-core/src/daemon_redescribe.rs, rust/crates/worklog-core/src/daemon_redescribe_test.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_redescribe` — no_claim: model is a fixed fake reply — follow: rust/crates/worklog-core/src/daemon_note_block.rs — after: T003 — done: c7c09a3
- [x] T005 [P] Web: daemon client, redescribeBlock server action, and the "Regenerate with prompt…" item with inline instruction box, Writing… state that ignores outside clicks, disabled reasons (FR-05, FR-07b) — files: web/lib/daemonBlockMenu.ts, web/app/actions-block-menu.ts, web/app/actions-block-menu.test.ts, web/components/BlockContextMenu.tsx, web/components/BlockContextMenu.test.tsx, web/components/BlockCard.test.tsx — verify: `cd web && bun test app/actions-block-menu.test.ts components/BlockContextMenu.test.tsx components/BlockCard.test.tsx && bun run typecheck` — follow: web/app/actions-note-block.ts — after: T002 — done: e41ca20
- [x] CHK001 human-verify the PREP Verify line in the running review UI: right-click a badly described block → Regenerate with prompt → "this was the login bug fix" → description matches, duration and ticket unchanged — files: web/components/BlockContextMenu.tsx — verify: human: user confirms the description changed and hours + ticket did not — after: T004, T005 — done: e41ca20 by user

## Phase 3 — Split and merge from the menu
Goal: the Owner splits a block at a minute mark or merges it with its previous or next block from the menu, through a new split client, the server actions and the existing daemon routes.
Independent test: `cd web && bun test app/actions-block-menu.test.ts components/BlockContextMenu.test.tsx` — split and merge call the daemon with the right ids and minutes; disabled states hold.
- [x] T006 Split… (minutes input) and Merge with previous / next, primary = clicked block, disabled with no or synced neighbour (FR-12, FR-13, FR-14) — files: web/lib/daemonBlockMenu.ts, web/app/actions-block-menu.ts, web/app/actions-block-menu.test.ts, web/components/BlockContextMenu.tsx, web/components/BlockContextMenu.test.tsx — verify: `cd web && bun test app/actions-block-menu.test.ts components/BlockContextMenu.test.tsx && bun run typecheck` — after: T005 — done: a4e6c08
- [x] T007 Pin the FR-14 and FR-04 gaps claim-check found: Split stays enabled on a synced block and on an exported block; a 501-char instruction leaves the block row unchanged (FR-04, FR-14) — files: web/components/BlockContextMenu.test.tsx, rust/crates/worklog-core/src/daemon_redescribe_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_redescribe && cd web && bun test components/BlockContextMenu.test.tsx` — after: T006 — done: 4af6057

## Gates
- [x] G001 project gates clean — files: . — verify: `flow check --fix --since 6a168ef` — done: 20b38df
- [x] G002 branch review clean — files: . — verify: `flow pass` — done: 20b38df
- [x] G003 verification evidence exists — files: . — verify: `test -s verify/` — done: 20b38df
