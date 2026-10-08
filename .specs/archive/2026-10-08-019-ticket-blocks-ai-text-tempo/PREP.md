# Prep — Add blocks on tickets, AI writes the text, sync to Tempo from here
Gathered: 2026-10-08 · Questions: 5 of 12 · Route: oneshot · Status: ready for spec

## Decisions
- D-01 Flow: click a + → enter time, pick the ticket in the form, write a rough description → saved as a normal block. — user, Q1
- D-02 The AI rewrites the rough description into the block's real description (not only the Tempo line text). — user, Q1
- D-03 The + lives on the day page; the new block lands on the day being viewed. — user, Q2
- D-04 The block saves right away with the rough note; the AI swaps in the real description afterwards. — user, Q4
- D-05 The user can edit the description after; a hand edit is never overwritten by the AI. — user, Q4
- D-06 The user can regenerate the AI description on demand. — user, Q4
## Not this
- No new Tempo sync button — the existing sync stays as-is. — user, Q3
- No blocks without a ticket from this form. — user, Q3
- No automatic send to Tempo on save. — user, Q3
## Discretion
- Where the rough note is stored so regenerate can reuse it.
- Whether regenerate on a hand-edited description asks first.
- Loading state shown while the AI writes.
## Assumptions
- A-01 "Log time" on a ticket (Tasks page) already exists: day, start, minutes, rough description → one manual block locked to the ticket (shipped #80) — evidence: rust/crates/worklog-core/src/ticket_log.rs:1, web/components/TaskWorkLog.tsx:13 — confidence: high — corrected Q1 (user: ticket is picked in the form, not by opening a ticket first; reuse log_time as the backend)
- A-02 The text sent to Tempo is already written by the model from the blocks' rough descriptions (per ticket per day line), unless hand-edited — evidence: rust/crates/worklog-core/src/collectors/tempo.rs:530 — confidence: high — unconfirmed
- A-03 Sending one ticket-day to Tempo already works from the Tasks page (preview, then send/update) — evidence: web/components/TaskDayGroup.tsx:99, web/components/TaskWorkLogSync.test.tsx:1 — confidence: high — unconfirmed
- A-04 The block's own description stays the raw text the user typed; only the Tempo line text is rewritten — evidence: rust/crates/worklog-core/src/ticket_log.rs:54 — confidence: medium — confirmed Q1 (this is the gap D-02 closes)
- A-05 Sync also skips a line that is already in Tempo as a hand entry (dedupe) — evidence: rust/crates/worklog-core/src/tempo_match.rs:1 — confidence: high — unconfirmed
- A-06 Existing block re-describe (`estimate`) works from a block's events; a hand block has none, so the AI must write from the rough note instead — evidence: rust/crates/worklog-core/src/infer.rs:562, rust/crates/worklog-core/src/estimate.rs:3396 — confidence: medium — unconfirmed
## Verify
- On the day page: click +, enter 30 min, pick a ticket, type "fixed login bug" → the block appears at once; within ~10 s its description becomes a proper sentence; Regenerate gives new text; the existing Send to Tempo sends it. — user, Q5
## Open
