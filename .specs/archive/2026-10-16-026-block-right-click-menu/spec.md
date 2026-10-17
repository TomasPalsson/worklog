# Spec: Right-click menu on blocks

**Created**: 2026-10-16 · **Route**: dispatch · **Prep**: PREP.md (D-01..D-03 copied verbatim)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: When the AI writes a bad description for a block, the Owner's only options are to retype it by hand or press Describe again and hope. There is no way to tell the AI what the block was actually about.

**Solution**: Right-clicking a block on the day page opens a menu. "Regenerate with prompt…" takes a one-time instruction and rewrites only that block's description. The same menu also holds the block's existing actions, plus Split, Merge and Delete, which have no button today.

**Who it's for**: the Owner (the single user who reviews and fixes their own day in the review UI).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Regenerating with a prompt changes the description and nothing else. Hours, ticket and estimate state stay exactly as they were (D-01).

## 1. Context

### 1.1 Problem statement

The AI estimator sometimes writes a description that is vague or wrong ("worked on stuff"). The Owner knows what the block was, but the AI cannot be told. Re-running Describe also rewrites hours and ticket, which the Owner may already have fixed.

**Current workaround**: click the description and retype it by hand, or press Describe (✨) again and accept whatever comes back, including new hours and ticket.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day's blocks in the web UI and fixes them before Tempo sync and billing | Desktop browser with a mouse; one user per install |

**Primary actor**: Owner.
**Hidden stakeholders**: Tempo sync and billing export, which read the description. A synced or exported block whose description changes must be flagged as changed (dirty), never silently diverge.

## 2. Scope

### 2.1 In scope

- A right-click menu on each block card on the day page, also reachable from the keyboard.
- "Regenerate with prompt…": a one-time instruction that rewrites only the description.
- The menu mirrors today's per-block buttons (D-02): Describe with Claude, Personal/Work toggle, Ignore → Noise / Duplicate / Internal.
- New actions with no button today: Delete (D-02 lists it; the daemon action exists but the card never had a button), Split block in two, Merge with previous block, Merge with next block.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Prompt-regenerate never changes duration or ticket. — user, Q1
- No menu items for things already one click away on the card (edit/copy description, ticket picker, details link, undo). — user, Q3
- Day page only: no right-click menu in the billing view or Tempo/Mirres pages. — user, Q6
- No multi-select / acting on many blocks at once. — user, Q6
- No touch long-press support. — user, Q6
- The instruction is never stored or reused for later descriptions (D-03).
- The existing icon buttons are not removed or changed (D-02).

### 2.3 Ask first

- new runtime dependency (no context-menu library; the repo's own menu helpers are used)
- CI/workflow config
- schema migration (none is needed; no new column)

## 3. Journeys

### Journey 1 — Fix a bad description with a prompt (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A described block reads "worked on stuff", 1h 30m on PROJ-12 | The Owner right-clicks it, picks "Regenerate with prompt…", types "this was the login bug fix" and submits | The card shows a new description about the login bug fix; it still reads 1h 30m and PROJ-12 |
| Error | Same block; the AI call fails or times out | The Owner submits an instruction | An error toast names the failure; the old description is unchanged; the typed instruction is still in the box |
| Edge | A block that was never described (no description yet), or a personal block, or a note block | The Owner opens the menu | "Regenerate with prompt…" is disabled and its tooltip says why |

### Journey 2 — Use the menu for an existing or new action (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A 2h block | The Owner right-clicks, picks "Split…", enters 45 and confirms | The card becomes two blocks, 45m and 1h 15m, both with the old ticket and description |
| Error | The Owner enters a split point of 0 or ≥ the block's length | They confirm | The menu shows the daemon's reason; nothing is split |
| Edge | The block is the first of the day | The Owner opens the menu | "Merge with previous" is disabled |

### Screen states

| Screen | Loading | Empty | Error | Success | Partial |
|--------|---------|-------|-------|---------|---------|
| Regenerate-with-prompt box | Submit button reads "Writing…" and is disabled; the box stays open | Submit is disabled while the instruction is blank | Error toast; box stays open with the text kept | Box closes; card shows the new description; toast "Description rewritten" | n/a |

## 4. Requirements

> Shapes, routes and reason strings live in `design.md` and the contract files it names; tests assert the contract's reason strings verbatim.

**Definitions used below.** *Described*: the description is non-empty after trimming (a hand-typed description already marks the block manual, so Estimate day skips it). *Note block*: a block created from a rough note. *Personal*: the block's existing personal flag. *Neighbour*: the nearest other block on the same day by start time, before or after. *Synced*: has a Tempo id; *exported*: has a billing export stamp.

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Regenerating with a prompt MUST change only the block's description: its duration, ticket, ticket origin, estimate state, Tempo id and export stamp stay byte-identical (D-01) | Core test: the block row before and after differs only in `description` (and `dirty` when synced) |
| FR-02 | MUST | The Owner's instruction MUST reach the AI together with the block's current description, duration, ticket and evidence (events, commits) | Core test with a capturing fake model: the prompt it received contains the instruction and the current description |
| FR-03 | MUST | The instruction MUST be used once and never stored (D-03) | No new column or table; a second regenerate without the text sends no trace of the first |
| FR-04 | MUST | An empty (after trimming) or over-500-character instruction MUST be refused with a reason, and the block MUST stay unchanged | Route tests for both, 400 with the contract reason |
| FR-05 | MUST | A personal block, a note block, or a block with no description MUST be refused with a reason, and the menu item MUST be disabled for it with that reason as its tooltip | Route tests (400) + component test (disabled + title) |
| FR-06 | MUST | When a regenerate changes a synced block, the block MUST be flagged changed (dirty) so Tempo sync re-sends it; its Tempo id MUST never be cleared | Core test on a synced block |
| FR-07 | MUST | A prompt-regenerate MUST be undoable with the day page's existing Undo, restoring the old description and the old changed-flag | Core test: undo after regenerate restores the whole row (Undo snapshots the full row) |
| FR-07a | MUST | The model MUST be asked for a description only; a reply with no usable description MUST fail the request with the block unchanged, and a reply over 500 characters MUST be cut at the last word boundary before 500, with no ellipsis | Core tests with fake replies (empty, 600 chars) |
| FR-07b | MUST | While a regenerate is running, the menu MUST stay open showing the instruction box in place of the item list, MUST ignore outside clicks, and MUST close only on success or Esc | Component test |
| FR-08 | MUST | Right-clicking a block card on the day page MUST open the block menu at the pointer instead of the browser menu | Component test: contextmenu event opens a `role="menu"` and is default-prevented |
| FR-09 | MUST | The menu MUST be keyboard-reachable: the ContextMenu key or Shift+F10 on a focused card opens it, arrow keys move, Esc closes and returns focus to the card | Component test |
| FR-10 | MUST | The menu MUST offer Describe with Claude, Personal/Work, and Ignore as Noise / Duplicate / Internal, each doing exactly what today's card button does (D-02) | Component test per item calls the same action as the button |
| FR-11 | MUST | Delete MUST remove the block and show a toast with Undo; it MUST be disabled, with the tooltip "Synced or exported — can't delete", on a synced or exported block (the daemon has no such guard and deleting would drop the Tempo id) | Component test: deleteBlock called; toast has Undo; disabled when synced/exported |
| FR-12 | MUST | "Split…" MUST take a whole-minute first-part length, starting blank, with Confirm disabled unless 1 ≤ minutes ≤ length − 1; the split follows the existing split rule (both halves keep ticket and description); a daemon refusal shows its reason in the menu | Component + action test: split called with the minutes |
| FR-13 | MUST | "Merge with previous" / "Merge with next" MUST merge the right-clicked block with its neighbour, keeping the right-clicked block's description and ticket, whatever the neighbour's ticket or the gap; each is disabled when there is no neighbour, the neighbour is synced, or the two overlap in time (merging adds durations, so an overlap would double-count) | Component test |
| FR-14 | MUST | When the right-clicked block is synced or exported, Ignore (all three) and both Merge items MUST be disabled, as the Ignore button is today; Delete follows FR-11; Describe, Personal/Work, Split and Regenerate stay enabled (split keeps the original's Tempo id and marks it changed; regenerate follows FR-06), and any daemon refusal shows as an error toast | Component test |
| FR-15 | MUST | The right-click menu MUST NOT appear outside the day page (D-02, Non-goals) | Component test: card rendered with `hideTicketing` has no menu |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Instruction length | 1–500 characters after trimming | FR-04 route tests |
| Regenerate wait | ≤ 60 s before the daemon gives up (same timeout as today's Describe) | Existing model timeout; error path test |
| Rewritten description length | ≤ 500 characters (longer model output is cut at a word boundary) | Core test with a 600-char fake reply |
| Menu open | Menu rendered in the same React commit as the contextmenu event (no fetch before open) | Component test: menu present synchronously after firing contextmenu |
| Accessibility | 100% of menu items are `role="menuitem"` buttons; disabled items carry their reason in `title`; focus lands on the first enabled item on open | Component test |
| Security | 0 new auth surface: the route sits behind the daemon's existing local socket/127.0.0.1 listener, like `/blocks/:id/estimate` | Route registered beside existing block routes |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised
- [ ] The NFR numbers in §5 are measured, not assumed
- [ ] User check (from PREP Verify): on the day page, right-click a block with a bad description → "Regenerate with prompt" → type "this was the login bug fix" → the description changes to match; the block's duration and Jira ticket are unchanged.
- [ ] External check: `cargo clippy -D warnings` + `bun run typecheck` — the Rust and TS contract mirrors agree with their callers
- [ ] Test seam(s): daemon route test with a fixed fake model — catches: request validation, which columns change, dirty/undo / misses: real model wording quality

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Day-wide Estimate only fills blocks never estimated and skips manual ones, so a rewritten description survives it (PREP A-10, estimate.rs:1-11) | High | A later Estimate day could wipe the fix |
| A2 | Requiring an existing description (FR-05) keeps D-01 safe: an undescribed block would otherwise get a description while staying "unestimated", and Estimate day would overwrite it | High | Owner must press Describe once before using a prompt |
| A3 | Note blocks keep their own rewrite-from-note path; the prompt item is disabled on them | Medium | Owner cannot steer a note block's rewrite in v1 |
| A4 | There is no Delete button today; `deleteBlock` exists and records an undo step (block_service.rs:256), so Delete in the menu is new UI with Undo | High | — |
| A5 | Merge keeps the right-clicked block as primary, so its description and ticket win; merged duration is the sum (existing `merge_blocks` rule), which is why overlapping neighbours are disabled (FR-13) | Medium | Owner expected the neighbour's text to win |
| A6 | The regenerate runs synchronously like today's Describe (no job/poll) | High | A slow model holds the request up to 60 s |
| A7 | Prior rulings hold: new daemon code goes in a sibling `daemon_*.rs` file, `daemon.rs` only gets wiring lines (archive 006 NOTES.md:3); BlockCard tests need the shared next/navigation mock (archive 019 NOTES.md:19) | High | Size-guard hook fails the build |
| A8 | Ignore keeps its stored reason (archive 021 B5) because the menu calls the same action | High | — |
| A9 | Which extra actions to add was delegated by the Owner (PREP Discretion, Q4); Split and Merge were picked because the daemon supports them with no UI today | Medium | Owner wanted different extras |
| A10 | The instruction is typed in a box that replaces the menu's item list in place (FR-07b); PREP left placement to the build | Medium | Owner prefers a popup |
| A12 | Single local user: no concurrent-edit guard; redescribe's write reads the row inside its own transaction, so the last write wins as for every other block edit | High | Two open tabs could overwrite each other, as today |
| A11 | A model failure maps to the same status and body today's Describe returns | High | Error toast wording differs |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Block | One stretch of work time on the day page, with a description, duration and optional Jira ticket |
| Instruction | The Owner's one-time text for "Regenerate with prompt…" |
| Synced | The block has a Tempo worklog id |
| Dirty | A synced block changed since its last sync, so the next sync re-sends it |
| Note block | A block the Owner added by hand with a rough note that the AI rewrites |
