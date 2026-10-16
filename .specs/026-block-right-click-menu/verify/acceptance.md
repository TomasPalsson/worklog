# Acceptance — 026 block right-click menu

Run at HEAD 22cb325 on 2026-10-16 (FR-04, FR-09, FR-14 re-run at 4af6057 after claim-check) by the orchestrator under /flow:next --finish.
Exit = the command's own exit code (`bash -c "<cmd>" > log 2>&1; echo $?`, no pipes).

| Criterion | Command | Exit | Showed |
|---|---|---|---|
| FR-01 redescribe changes only description | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::rewrites_only_the_description_column` | 0 | rust passed 1 |
| FR-02 prompt carries instruction + current description | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::prompt_carries_instruction` | 0 | rust passed 1 |
| FR-03 instruction not stored between calls | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::instruction_is_not_remembered` | 0 | rust passed 1 |
| FR-04 empty / 501-char instruction → 400, block unchanged | `cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_redescribe` | 0 | rust passed 8 (incl. `over_long_instruction_leaves_block_row_unchanged`, T007) |
| FR-05 personal / note / undescribed refused + disabled with reason | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::refuses && cd web && bun test components/BlockContextMenu.test.tsx -t 'regenerate with prompt'` | 0 | rust passed 4; bun pass 17 fail 0 |
| FR-06 synced block goes dirty, keeps Tempo id | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::synced_block_is_marked_dirty` | 0 | rust passed 1 |
| FR-07 Undo restores old description | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::undo_restores` | 0 | rust passed 1 |
| FR-07a empty reply fails; 600 chars cut at word to ≤500 | `cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::reply && cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::unusable_replies && cargo test --manifest-path rust/Cargo.toml -p worklog-core redescribe::tests::long_reply` | 0 | rust passed 6 |
| FR-07b box stays open on outside click while writing; Esc closes | `cd web && bun test components/BlockContextMenu.test.tsx -t 'while writing'` | 0 | bun pass 3 fail 0 |
| FR-08 right-click opens block menu, not browser menu | `cd web && bun test components/BlockContextMenu.test.tsx -t 'right-click opens a menu'` | 0 | bun pass 1 fail 0 |
| FR-09 ContextMenu / Shift+F10 open; Esc closes, focus back | `cd web && bun test components/BlockContextMenu.test.tsx -t 'ContextMenu key on the card\|Shift.F10\|Esc closes it and focus\|arrow keys move focus'` | 0 | bun pass 4 fail 0 |
| FR-10 menu items run the same actions as card buttons | `cd web && bun test components/BlockContextMenu.test.tsx -t 'mirror the card buttons'` | 0 | bun pass 6 fail 0 |
| FR-11 Delete + Undo toast; disabled when synced/exported | `cd web && bun test components/BlockContextMenu.test.tsx -t 'delete .FR-11.\|synced guard'` | 0 | bun pass 9 fail 0 |
| FR-12 Split calls split(minutes); 0 and full length disabled | `cd web && bun test components/BlockContextMenu.test.tsx -t 'split .FR-12.' && bun test app/actions-block-menu.test.ts -t splitBlock` | 0 | bun pass 27 fail 0 |
| FR-13 Merge prev/next keeps clicked block primary; disabled cases | `cd web && bun test components/BlockContextMenu.test.tsx -t 'merge with neighbour'` | 0 | bun pass 14 fail 0 |
| FR-14 synced/exported: ignore + merge disabled, others enabled | `cd web && bun test components/BlockContextMenu.test.tsx -t 'synced guard\|stays enabled on a\|merge with neighbour'` | 0 | bun pass 20 fail 0 (incl. Split "stays enabled on a synced/exported block (FR-14)") |
| FR-15 hideTicketing card opens no menu | `cd web && bun test components/BlockCard.test.tsx -t 'day-page only'` | 0 | bun pass 2 fail 0 |
| CHK001 Regenerate with prompt in the running UI: description changes, hours + ticket do not | see `verify/CHK001.md` | human | real model, isolated DB: "Did stuff" → "Fixed the login bug on ABC-1…"; 5400 s and ABC-1 unchanged |

Notes: FR-12's split test uses a 30-min block (0 and 30 disabled, 1..29 enabled, 12 posts
`splitBlock(9, day, 12)`) — the same rule as the spec's 120/45 example. Full suites at this HEAD:
cargo test 2498 passed 0 failed; bun test 2066 pass 0 fail.
