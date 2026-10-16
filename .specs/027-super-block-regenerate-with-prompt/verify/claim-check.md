# claim-check — 027 acceptance.md (re-run 2026-10-16, head 073d075)

Rust runs are the named test(s) via `cargo test ... -p worklog-core`; web via `bun test`. All re-runs passed (cargo 14/14 tempo_line_redescribe::tests, 9/9 daemon_tempo_line_redescribe; bun 82 pass 0 fail across GroupContextMenu + TicketGroup; git diff dc9a952 removed-lines count = 0, so TicketGroup alone = 82-13 = 69).
Caveat: the shell here is fish, so the literal "echo $?" exit code was not captured; pass/fail is from the test-result lines.

- FR-01 CONFIRMED: test asserts text, origin=manual, check_status None, hours_override, source_hash None, block description and tempo id unchanged.
- FR-02 CONFIRMED: header right-click test passes, named for the criterion.
- FR-03 CONFIRMED: Shift+F10/ContextMenu key test and Esc-returns-focus test pass.
- FR-04 CONFIRMED: asserts user_msg JSON owner_instruction and current_text.
- FR-05 CONFIRMED: route tests cover blank 400, 500 ok / 501 400 (ASCII and multibyte); UI tests cover submit disabled for blank and at 501.
- FR-06 INCONCLUSIVE: the "sent" half is asserted (second user_msg and system prompt lack the first instruction). "Stored" is not asserted anywhere in this test; it holds only because prepare/commit keep the instruction out of the DB signature.
- FR-07 INCONCLUSIVE: asserts the error contains "reynt 3 sinnum". "Text unchanged" is not asserted (write() has no DB handle); the daemon test bad_model_reply_is_502_and_stores_nothing covers it but is not the row's command.
- FR-08 INCONCLUSIVE: the named test only seeds a synced block and asserts dirty==1. "Unsynced not dirty" is asserted in a different test (commit_into_a_line_with_no_stored_text_succeeds). CHK001.md shows both live (a document, not re-run).
- FR-09 CONFIRMED: pending_generation empty and commit_generated leaves text NEW_TEXT with origin manual.
- FR-10 CONFIRMED: two core tests assert REASON_TEXT_CHANGED (edited meanwhile; text appeared after a None prepare). 409 is proven by a unit test of the error-to-status mapping, not by an end-to-end request.
- FR-11 CONFIRMED: UI tests assert disabled + title "Pick a ticket first" in both components; route tests unknown_line_is_404 and line_with_only_personal_blocks_is_404 pass.
- FR-12 CONFIRMED: outside-click-while-writing test passes.
- FR-12a CONFIRMED: Esc while writing closes the menu, later reply toasts "Text rewritten". (Saving is the daemon's job; the UI test checks the toast.)
- FR-12b CONFIRMED: success and failure toast tests pass.
- FR-13 CONFIRMED: block-card-inside-group test passes.
- FR-13a CONFIRMED: 0 removed lines vs dc9a952; TicketGroup suite 69 pass 0 fail (derived from the combined run).
- CHK001 CONFIRMED (artifacts exist): CHK001.md and CHK001-0..5 jpgs present. Contents are a human/orchestrator live run; not re-executed, and the screenshots were not opened.

Summary: 14 confirmed, 0 unsupported, 0 not-checkable, 3 inconclusive (FR-06, FR-07, FR-08).
