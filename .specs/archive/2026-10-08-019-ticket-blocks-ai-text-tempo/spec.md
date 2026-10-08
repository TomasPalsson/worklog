# Spec: Add a ticket block from the day page, AI writes its description

**Created**: 2026-10-08 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Owner can only hand-log time from inside one ticket on the Tasks page, and the rough note they type is stored as the block's description word for word.

**Solution**: A **+** on the day page opens a small form (start, minutes, ticket, rough note). The block saves at once; the AI then rewrites the note into a proper description, which the Owner can edit or regenerate. The existing Tempo sync sends it.

**Who it's for**: The Owner — the one person whose time this worklog tracks.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The block keeps the rough note beside its description and records who wrote the description (note / AI / hand), so the AI only ever overwrites its own text unless the Owner asks.

## 1. Context

### 1.1 Problem statement

Work that left no trail (a call, a whiteboard session) never becomes a block on its own. Today the Owner must open the ticket on the Tasks page and click "Log time", and whatever they type is the description sent to Tempo. They want to stay on the day they are reviewing, jot a rough note, and let the model write it properly.

**Current workaround**: Tasks page → open ticket → "Log time" (shipped #80), then hand-polish the text, or log it straight in Tempo.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews a day, adds missing time, sends it to Tempo | Single user; edits are final |
| AI writer | Turns a rough note into a description | Runs in the background; never touches a hand edit on its own |

**Primary actor**: Owner.
**Hidden stakeholders**: Tempo (receives the text via the existing sync); billing export (reads the block like any other).

## 2. Scope

### 2.1 In scope

- A **+** control on the day page that opens an add-block form for that day. (D-01, D-03)
- Saving the form creates a normal manual block on the picked ticket, at once. (D-01, D-04)
- The AI rewrites the rough note into the block's description in the background. (D-02, D-04)
- The Owner can hand-edit the description; the AI never overwrites that edit by itself. (D-05)
- The Owner can regenerate the AI description on demand. (D-06)

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No new Tempo sync button — the existing sync stays as-is. — user, Q3
- No blocks without a ticket from this form. — user, Q3
- No automatic send to Tempo on save. — user, Q3
- No change to the Tasks-page "Log time" form.
- No AI rewrite for blocks that did not come from this form.

## 3. Journeys

### Journey 1 — Add a block with a rough note (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Day page for today or a past day, ticket ABC-1 open | Owner clicks +, enters 10:00, 30 min, ABC-1, "fixed login bug", saves | Block 10:00–10:30 on ABC-1 appears showing the note; its text becomes a full sentence written by the AI (p95 ≤ 10 s) |
| Error | AI call fails or times out | Owner saves the form | Block stays with the rough note as its description; a toast says the AI could not write it; Regenerate is offered |
| Error | Ticket key is malformed, the note is blank, or the daemon is down | Owner saves the form | The form shows the reason ("Invalid ticket key", "Write a note", or the daemon error); no block is created; typed values stay |
| Edge | Day is in the future, or compressed, or start + minutes ends after midnight | Owner saves the form | Save is refused with the reason shown in the form; no block is created |
| Edge | The new block overlaps an existing block | Owner saves the form | Saved as-is; billing already counts overlapping time once (union of intervals) |

### Journey 2 — Edit and regenerate (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A note block with AI text | Owner clicks Regenerate | New AI text replaces the old (p95 ≤ 10 s) |
| Error | Owner hand-edited the text while a job ran | The background AI write lands afterwards | The hand edit stays; the AI write is dropped |
| Edge | Owner hand-edited the text | Owner clicks Regenerate | Asked "Replace your edit?"; yes → AI text replaces it, no → nothing changes |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to open an add-block form from a **+** on the day page | Component test: + visible on a day with and without blocks |
| FR-02 | MUST | The form MUST take start (local HH:MM, `WORKLOG_TZ`), minutes (1–720), ticket key and rough note (1–500 chars after trimming) | Component test rejects each out-of-range field with a message |
| FR-02a | MUST | The form MUST reject a ticket key that does not match the daemon's key rule (`PROJECT-123`; letters, digits, `_`) with "Invalid ticket key" | Component test + daemon route test (400, no block) |
| FR-02b | MUST | The daemon MUST refuse a note block whose end falls after midnight of the viewed day | Core test: 23:30 + 60 min → error, no block |
| FR-03 | MUST | Saving MUST create a manual block on the viewed day with the note as its first description | Core test: block row has the note, ticket, times, manual marker |
| FR-04 | MUST | The daemon MUST keep the rough note beside the description | Core test: note column equals the typed note after the AI rewrite |
| FR-05 | MUST | After save the AI writer MUST replace the description with text written from the note, ticket and minutes | Core test with fake model: description = model output, origin = ai |
| FR-06 | MUST | The daemon MUST mark a note block's description hand-written when the Owner edits it | Core test: set description → origin = hand |
| FR-07 | MUST | The AI writer MUST NOT overwrite a hand-written description unless the Owner confirmed Regenerate | Core test: hand edit, then commit → description unchanged |
| FR-08 | MUST | Owner MUST be able to regenerate the description of a note block | Route test: regenerate starts a job; status reaches done |
| FR-09 | MUST | Regenerating a hand-written description MUST ask the Owner first | Component test: confirm shown when origin = hand |
| FR-10 | MUST | An AI write on an already-synced block MUST mark it as changed for the next sync | Core test: synced block → dirty after AI commit |
| FR-11 | MUST | The day page MUST show the new AI text without a manual reload | Component test: status done → page refresh called |
| FR-12 | SHOULD | The start field SHOULD default to the end of the day's last block, else 09:00 | Component test for both cases |
| FR-14 | MUST | The AI writer MUST reject an empty model reply or one over 500 chars, leaving the description unchanged and the job failed | Core test with fake model |
| FR-13 | SHOULD | While the AI is writing, the block SHOULD say so | Component test: "Writing…" shown while status = running |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Block appears after save | < 1 s | Manual check in CHK001 |
| AI text lands after save | p95 ≤ 10 s; UI stops polling at 30 s and says "still writing — check back" | Manual check in CHK001; poll cap in component test |
| Poll interval | 1 s | Component test |
| Concurrent jobs per block | 1 (a second request while running returns `started: false`) | Route test |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised
- [ ] On the day page: click +, enter 30 min, pick a ticket, type "fixed login bug" → the block appears at once; within ~10 s its description becomes a proper sentence; Regenerate gives new text; the existing Send to Tempo sends it. — user, Q5
- [ ] `cargo test`, `cargo clippy -D warnings`, `bun test`, `bun run typecheck` green

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | `ticket_log::log_time` is the save path; it already validates day, minutes, note length and compressed days (prep A-01) | High | Duplicate validation |
| A2 | The existing estimate prompt can't write from a free-text note; a new small prompt is needed (prep A-06) | High | Hallucinated text |
| A3 | The day sync and the Tasks-page sync pick up a hand-logged block with no change (prep A-03) | High | Block not sent |
| A4 | The ticket field offers the day page's open assigned tickets and accepts any typed key the daemon's `validated_key` accepts | Med | Owner can't pick an unassigned ticket |
| A5 | Job state lives in memory; a daemon restart mid-write leaves the rough note as the description, and Regenerate recovers | High | Note block keeps raw text until regenerated (see A8) |
| A7 | The note goes to the same model provider that already writes every block and line description; no new data leaves the machine's existing paths | High | Privacy review needed |
| A8 | A block still at origin `note` never reaches Tempo raw: the Tempo sync writes line text with the AI from block descriptions, and its preview shows the text before sending (prep A-02) | High | Raw note in Tempo |
| A6 | On note blocks, the existing Sparkles button becomes Regenerate rather than adding a second button | Med | Owner hunts for Regenerate |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Note block | A block created by this form; it has a rough note |
| Rough note | The Owner's own words, kept as typed |
| Compressed day | A day whose events were folded into a digest; it can no longer be edited and every write refuses with "day is compressed" |
| Description origin | Who wrote the current description: `note` (still the raw note), `ai`, or `hand` |
