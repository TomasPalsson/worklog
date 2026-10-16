# claim-check: 026 acceptance.md (HEAD 22cb325, each command re-run via `bash -c`, exit read directly)

FR-01 CONFIRMED: exit 0, 1 passed; test diffs full row, only `description` changed.
FR-02 CONFIRMED: exit 0, 1 passed; capturing fake model checks instruction + description + ticket + evidence.
FR-03 CONFIRMED: exit 0, 1 passed; 1st run "make it shorter", 2nd call's prompt asserted free of it.
FR-04 CONFIRMED (weak spot): exit 0, 7 passed; blank -> 400 and row unchanged asserted; 501 -> 400 asserted but "block unchanged" not asserted for the 501 case.
FR-05 CONFIRMED: exit 0, rust 4 passed + bun 17 pass / 0 fail; personal/note/undescribed/blank disabled with exact reason title.
FR-06 CONFIRMED: exit 0, 1 passed; changed cols = description+dirty, tempo id and exported_at kept.
FR-07 CONFIRMED: exit 0, 1 passed; undo restores full row snapshot.
FR-07a CONFIRMED: exit 0, 3+1+2 = 6 passed; empty/unusable reply leaves block, 600-char cut at word boundary.
FR-07b CONFIRMED: exit 0, 3 pass / 0 fail (outside click ignored while writing, Esc closes).
FR-08 CONFIRMED: exit 0, 1 pass; synchronous menu + default prevented.
FR-09 CONFIRMED (narrow): exit 0, 3 pass; covers ContextMenu key, Shift+F10, Esc+focus return. Spec FR-09 also says "arrow keys move"; that test ("arrow keys move focus") exists but is NOT in the -t filter, so it is not part of this evidence.
FR-10 CONFIRMED: exit 0, 6 pass; each item asserted against the same action mock.
FR-11 CONFIRMED: exit 0, 9 pass; deleteBlock + Undo toast, disabled with exact "Synced or exported — can't delete" tooltip. (Filter also pulls in the 'synced guard' tests; count 9 is real.)
FR-12 CONFIRMED: exit 0, 14 + 13 = 27 pass; 0/30 disabled, 1..29 enabled, splitBlock(…, minutes) asserted, refusal reason shown.
FR-13 CONFIRMED: exit 0, 14 pass; primary = clicked block, no-neighbour / synced neighbour / overlap disabled, touching ok.
FR-14 UNSUPPORTED (partial): exit 0, 19 pass, but the claim "others enabled" is only evidenced for Describe, Personal, Regenerate, and Delete's disabled state. No test in the filtered set (or anywhere in BlockContextMenu.test.tsx) asserts Split is enabled on a synced/exported block. Ignore x3 and Merge x2 disabled are covered.
FR-15 CONFIRMED: exit 0, 2 pass; hideTicketing card: contextmenu not cancelled, no role=menu, ContextMenu key opens nothing.
CHK001 CONFIRMED (existence only): CHK001.md and CHK001-{1-before,2-menu,3-instruction,4-writing,5-after}.jpg all exist and are non-empty. Content not judged (human check).

Counts: every "Showed" count matched the actual run (pass counts above, 0 fail; skips are only filter exclusions).
Summary: 16 confirmed, 1 unsupported, 0 inconclusive, 0 not-checkable.
