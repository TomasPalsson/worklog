# Acceptance — 027 super block regenerate with prompt

Run 2026-10-16 at 073d075 from the repo root. `R` = `cargo test --manifest-path rust/Cargo.toml -p worklog-core`.
Exit = `bash -c "<cmd>" > log 2>&1; echo $?`, read per row (no pipe after the command).

| Criterion | Command | Exit | Showed |
|---|---|---|---|
| FR-01 only line text + origin change | `R tempo_line_redescribe::tests::commit_writes_manual_text_and_touches_nothing_else` | 0 | 1 passed; 0 failed |
| FR-02 header right-click opens group menu | `cd web && bun test components/TicketGroup.test.tsx -t 'right-click on the header opens'` | 0 | 1 pass 0 fail |
| FR-03 ContextMenu / Shift+F10, Esc returns focus | `cd web && bun test components/TicketGroup.test.tsx -t 'Shift\+F10\|hands focus back'` | 0 | 2 pass 0 fail |
| FR-04 model input has instruction + current text | `R tempo_line_redescribe::tests::user_msg_carries_instruction_and_current_text` | 0 | 1 passed; 0 failed |
| FR-05 empty / 501-char instruction → 400, box disabled | `R daemon_tempo_line_redescribe && cd web && bun test components/GroupContextMenu.test.tsx -t '500 Icelandic\|blank and whitespace'` | 0 | 9 passed (route); 2 pass (box) |
| FR-06 nothing of a first instruction sent or stored | `R tempo_line_redescribe::tests::second_redescribe_carries_nothing_of_the_first_instruction` | 0 | 1 passed; 0 failed |
| FR-07 3 failed checks → error, text unchanged | `R tempo_line_redescribe::tests::write_surfaces_the_validation_failure && R bad_model_reply_is_502_and_stores_nothing` | 0 | 1 passed (error after 3 tries); 1 passed (502, nothing stored) — re-run after claim-check |
| FR-08 synced block dirty, unsynced not | `R tempo_line_redescribe::tests::commit_` | 0 | 7 passed: commit_writes_manual_text… (synced → dirty=1) and commit_into_a_line_with_no_stored_text_succeeds (unsynced stays 0); also live in verify/CHK001.md — re-run after claim-check |
| FR-09 auto pass keeps the text as manual | `R tempo_line_redescribe::tests::redescribed_line_survives_the_automatic_generation_pass` | 0 | 1 passed; 0 failed |
| FR-10 text changed after prepare → REASON_TEXT_CHANGED / 409 | `R tempo_line_redescribe::tests::commit_refuses_when && R daemon_tempo_line_redescribe` | 0 | 2 passed (core); route suite incl. commit_error 409 |
| FR-11 Unassigned disabled "Pick a ticket first"; 404 for no line | `R daemon_tempo_line_redescribe && cd web && bun test components/GroupContextMenu.test.tsx components/TicketGroup.test.tsx -t 'Pick a ticket first'` | 0 | 9 passed (route, incl. 404s); 2 pass (UI) |
| FR-12 outside click while writing keeps the box | `cd web && bun test components/GroupContextMenu.test.tsx -t 'does not close it while writing'` | 0 | 1 pass 0 fail |
| FR-12a Esc while writing closes, reply still saved + toasted | `cd web && bun test components/GroupContextMenu.test.tsx -t 'Esc while writing'` | 0 | 1 pass 0 fail |
| FR-12b success toast + close; failure toast keeps text | `cd web && bun test components/GroupContextMenu.test.tsx -t 'success closes\|failure toasts'` | 0 | 2 pass 0 fail |
| FR-13 block card in a group opens the block menu | `cd web && bun test components/TicketGroup.test.tsx -t 'block card inside the group'` | 0 | 1 pass 0 fail |
| FR-13a header toggle / Edit / Generate unchanged, existing tests unedited | `git diff --numstat dc9a952 -- web/components/TicketGroup.test.tsx` then `cd web && bun test components/TicketGroup.test.tsx` | 0 | numstat `78 0` (0 lines removed); 69 pass 0 fail |
| CHK001 live run in the review UI | see verify/CHK001.md | human | text changed; hours, ticket, block texts unchanged; synced block dirty |
