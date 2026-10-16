# Spec: Super block — Regenerate with prompt

**Created**: 2026-10-16 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The text of a super block (the ticket group's line text, which becomes the Tempo worklog comment) can only be generated blind or typed by hand. When the AI gets it wrong, the Owner can't tell it what the work actually was.

**Solution**: The Owner right-clicks the super block's header, picks "Regenerate with prompt…", and types a one-time instruction. The AI rewrites the group's text using that instruction, the current text and the usual evidence.

**Who it's for**: The Owner, the one person who reviews their own day in the worklog review UI.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The instructed text is saved as the Owner's own text (`manual`), so the automatic pass never silently replaces it.

## 1. Context

### 1.1 Problem statement

On the day page each ticket group (super block, e.g. "GENAI-2023 · 4 blocks") has a line text. That text is sent to Tempo as the worklog comment. Today the only choices are "Generate", which takes no guidance, and "Edit text", which means writing the Icelandic text yourself. Spec 026 gave single blocks a "Regenerate with prompt…". The super block, whose text actually reaches Tempo, has nothing like it.

**Current workaround**: Press Generate repeatedly and hope, or hand-write the Icelandic text.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day, fixes texts, syncs to Tempo | The only user. Reads Icelandic Tempo comments. |

**Primary actor**: Owner.
**Hidden stakeholders**: Tempo, which receives the text on the next sync. The Owner's manager reads those comments.

## 2. Scope

### 2.1 In scope

- A right-click (and keyboard) menu on the super block header with exactly one item, "Regenerate with prompt…".
- A one-time instruction box. The AI rewrites the group's line text from the instruction, the current text and the existing Generate evidence.
- The result is saved as the Owner's text, and the group's synced blocks are marked for re-sync.

### 2.2 Non-goals

- No other menu items (no Generate, Edit text, Merge, Personal, Ignore…).
- No menu on billing groups (the Billing page's invoice lines).
- No undo. The old text is replaced, as with Generate today.
- No change to the single-block menu from spec 026.
- No change to how the automatic pass or the Generate button work.

### 2.3 Ask first

- new runtime dependency
- CI/workflow config
- schema migration

## 3. Journeys

### Journey 1 — Steer a super block's text (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A ticket group with a Tempo line | The Owner right-clicks its header, picks "Regenerate with prompt…", types "this was the login bug fix" and presses Enter | The box shows "Writing…". On success the menu closes, a toast says "Text rewritten", the header shows the new text and the badge reads as the Owner's text. Hours, ticket and block texts are unchanged. |
| Error | The same, but the AI fails or its replies fail the text check 3 times | The Owner submits | A toast says "Rewrite failed — <reason>". The box stays open with the instruction kept. The group's text is unchanged. |
| Error | The Owner presses "Edit text" (or anything else changes the text) while the AI is writing | The AI reply arrives | The rewrite is refused with "the text changed while writing — try again". The text the Owner saved stays. |
| Edge | The "Unassigned" group (no ticket, so no Tempo line) | The Owner right-clicks its header | The menu opens with the item disabled and the tooltip "Pick a ticket first". |
| Edge | A group whose blocks are already synced to Tempo | The Owner regenerates | The text changes, and each synced block in the group is marked changed so the next sync sends the new comment. |

### Screen states

| Screen | Loading | Empty | Error | Success | Partial |
|--------|---------|-------|-------|---------|---------|
| Group menu instruction box | "Writing…" on a disabled button. An outside click does not close it, Esc does. | Regenerate disabled while the trimmed instruction is empty | Toast "Rewrite failed — <reason>". Box stays, text kept. | Menu closes, toast "Text rewritten", new text shown | n/a |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | A regenerate MUST change only the group's line text and its origin. Block hours, tickets, block descriptions, Tempo ids, export stamps and the line's hours override MUST be identical before and after; the only other change allowed is FR-08's re-sync mark. | Core test diffs every blocks row and the line row before and after |
| FR-02 | MUST | Right-clicking a super block header MUST open the group menu, with exactly one item "Regenerate with prompt…", and MUST NOT show the browser's menu | Component test: contextmenu is cancelled, one menuitem |
| FR-03 | SHOULD | With the header focused, the ContextMenu key or Shift+F10 MUST open the menu; Esc MUST close it and return focus to the header | Component test |
| FR-04 | MUST | The model input MUST contain the Owner's instruction, the group's current text (stored text, else the fallback text) and the same evidence Generate uses | Core test with a capturing fake model |
| FR-05 | MUST | An instruction that is empty after trimming, or longer than 500 characters (counted in characters, not bytes), MUST be refused with a reason and nothing changed; the box MUST keep Regenerate disabled for it | Route test 400 + row unchanged; component test |
| FR-06 | MUST | The instruction MUST be used for that one request only; it MUST NOT be stored or sent with any later request | Core test: second call carries no trace of the first |
| FR-07 | MUST | The new text MUST pass the same text check Generate uses (with up to 3 tries); when no try passes, the request MUST fail with the reason and the text MUST be unchanged | Core test with fake replies that fail the check |
| FR-08 | MUST | When any block of the group is synced to Tempo, those blocks MUST be marked changed after a successful regenerate; unsynced blocks MUST stay unmarked | Core test |
| FR-09 | MUST | The saved text MUST be marked as the Owner's own text, and the automatic generation pass MUST leave it untouched | Core test: automatic pass after a regenerate keeps the text |
| FR-10 | MUST | If the group's text changed between the start of the request and the AI reply, the request MUST be refused with "the text changed while writing — try again" and the newer text kept | Core test |
| FR-11 | MUST | For a group with no Tempo line (Unassigned), the item MUST be disabled with the tooltip "Pick a ticket first", and the route MUST answer not-found | Component test + route test |
| FR-12 | MUST | While the AI is writing, an outside click MUST NOT close the box; Esc MUST close it; success MUST close the menu, toast "Text rewritten" and show the new text | Component test |
| FR-13 | MUST | Right-clicking a block card inside a group MUST still open that block's own menu, not the group menu; a left click on the header MUST still open and close the group | Component test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Instruction length | 1–500 characters after trimming | Route and component tests at 0, 500 and 501, using Icelandic characters |
| Model tries | at most 3 per request | Core test counts the fake model calls |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error paths of Journey 1 are exercised (model failure, text changed while writing)
- [ ] External check: `bun run typecheck` and `cargo clippy -D warnings` are clean, plus one real-browser run on an isolated database with the real model (CHK001)
- [ ] Test seam(s): the core functions with an injected fake model — catches: what is sent and what is written / misses: real model wording. The group menu component with mocked server actions — catches: open, close, disabled and writing states / misses: real layout.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | The instructed text is stored as `manual`, so the badge reads as the Owner's text (discovery item 4) | High | The Owner expected an "AI" badge; cosmetic |
| A2 | No "Replace your text?" confirm before overwriting hand-written text (item 6) | Med | An Owner loses hand-written text with one Enter; no undo |
| A3 | The Verdict line check that Generate runs after writing is skipped; check status is cleared, as for hand-edited text | Med | A steered text gets no "needs a look" chip |
| A4 | The model sees the existing Icelandic system prompt plus one Icelandic paragraph telling it to follow the Owner's instruction | High | Instruction ignored → Owner retries |
| A5 | A meeting-only line (meeting shortcut) is regenerated through the model like any other line when the Owner gives an instruction | Med | Meeting lines get model text instead of the meeting title |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Super block | The ticket group card on the day page (header "<TICKET> · N blocks"), whose line text goes to Tempo |
| Line text | The group's text stored per day + ticket; the Tempo worklog comment |
| Owner's text | Line text with origin `manual`, shown with the hand-edited badge and skipped by the automatic pass |
