# claim-check: 026 acceptance.md (re-run at HEAD bf05bc9)
FR-01 CONFIRMED: exit 0, 1 test (rewrites_only_the_description_column) ran; asserts only description column changes.
FR-02 CONFIRMED: exit 0, 1 passed; prompt carries instruction, description, ticket, evidence.
FR-03 CONFIRMED: exit 0, 1 passed (instruction_is_not_remembered).
FR-04 CONFIRMED: exit 0, 8 passed; blank, 500-limit-after-trim, over_long_instruction_leaves_block_row_unchanged all present and cover empty/501 -> 400 + block unchanged.
FR-05 CONFIRMED: exit 0; rust refuses 4 passed (unknown/note/personal/missing-empty-blank); bun 17 pass 0 fail (37 skipped by filter); personal/note/undescribed/blank disabled-with-tooltip tests present. Note: the 17 include FR-07b and FR-14 tests; the 4 disabled-reason ones are the FR-05 evidence.
FR-06 CONFIRMED: exit 0, 1 passed (synced_block_is_marked_dirty_and_keeps_its_ids).
FR-07 CONFIRMED: exit 0, 1 passed (undo_restores_the_old_row).
FR-07a CONFIRMED: exit 0; 3+1+2 = 6 passed (reply_*, unusable_replies, long_reply_*); empty and 600-char cut covered.
FR-07b CONFIRMED: exit 0, bun pass 3 fail 0 (button Writing..., outside click ignored, Esc closes).
FR-08 CONFIRMED: exit 0, bun pass 1 fail 0; asserts synchronous open and preventDefault.
FR-09 CONFIRMED: exit 0, bun pass 4 fail 0 (ContextMenu key, Shift+F10, arrows, Esc + focus return). Command works only with the table `\|` read as `|` (as instructed); literal backslash-pipe in basic-regex matches 0.
FR-10 CONFIRMED: exit 0, bun pass 6 fail 0; Describe, Personal, Work, Noise, Duplicate, Internal each tested.
FR-11 CONFIRMED: exit 0, bun pass 9 fail 0; deleteBlock, Undo toast, refused delete, disabled on synced/exported present. The 3 synced-guard Ignore tests in the count are FR-14 filler, but 6 delete tests cover FR-11.
FR-12 CONFIRMED: exit 0; 14 component + 13 splitBlock = 27 pass 0 fail; blank start, 0/30 disabled, 1/29 enabled, split called with minutes, refusal shown in menu.
FR-13 CONFIRMED: exit 0, bun pass 14 fail 0; primary kept, any ticket/gap, prev/next/none/synced neighbour, overlap-by-1-minute disabled.
FR-14 CONFIRMED: exit 0, bun pass 20 fail 0; Ignore x3 and both Merge disabled on synced/exported; Describe/Personal, Split, Regenerate stay enabled tests present; Delete via FR-11. Not tested here: the "daemon refusal shows as error toast" clause (merge/delete refusal toast tests exist in other filters but not in this command).
FR-15 CONFIRMED: exit 0, bun pass 2 fail 0 (right-click and ContextMenu key open nothing with hideTicketing).
CHK001 CONFIRMED (existence only): verify/CHK001.md (1837 B) and CHK001-1..5 jpg (49-56 KB each) exist and are non-empty.

Observed, not requested: acceptance.md line 3 says "Run at HEAD 22cb325"; actual HEAD is bf05bc9 (rows re-run at 4af6057 per same line).

18 confirmed, 0 unsupported, 0 inconclusive, 0 not-checkable.
