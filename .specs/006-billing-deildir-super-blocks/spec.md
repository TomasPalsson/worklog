# Spec: Billing deildir and super blocks

**Created**: 2026-09-26 · **Route**: dispatch · **Prep**: PREP.md (10 of 12, D-01…D-08)

## TL;DR

**Problem**: A customer bills under several deildir (Verkefni), but the owner can set only one Verkefni per folder, so a day's work for one customer+deild is scattered over many billing lines and a shared folder can't put different blocks on different deildir. Automatic changes (Claude rewrites, Verdict re-routing, re-resolution) move blocks silently.

**Solution**: The owner keeps a deildir list per customer, each block's time is guessed onto a (customer, deild) and can be split by % across several, every (customer, deild) becomes one billing line (a super block), and every automatic change shows up as a batched, source-named notice — live and as a catch-up.

**Who it's for**: The Owner — the one person who reviews their day and fills in the external invoicing form.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1.

**Key decision**: A deild may now be *guessed* from keywords the Owner typed on that deild (D-02). This amends the CLAUDE.md rule "Verkefni only from an explicit pin": a keyword is the Owner's own explicit configuration, so nothing on the invoice is invented; no keyword and no folder default → the deild stays blank.

## 1. Context

### 1.1 Problem statement

Sjúkra and APRÓ each have several deildir (e.g. Rekstur, Áskrift, AI hraðall). Today Verkefni lives only on a folder pin, one per folder, and billing lines split further by Jira ticket, so the 2026-09-25 day shows three APRÓ·[O] AI hraðall lines and no way to put one vitinn-infra block on Sjúkra·Rekstur and another on Sjúkra·Áskrift. The Owner also cannot tell when a background model changed something.

**Current workaround**: Edit the folder pin (moves every block at once) and re-type lines by hand in the invoicing form.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews blocks, sets deildir and splits, exports billing | Single user, often not looking when background jobs run |
| Automatic writer | Claude estimator, Verdict router, block rebuild, keyword/alias resolution | Changes data with no click |

**Primary actor**: Owner. **Hidden stakeholders**: the external invoicing system (consumes Verkefni text verbatim).

## 2. Scope

### 2.1 In scope

- A deildir list per customer, each deild with a name and keywords.
- A default deild per folder (the existing folder Verkefni pin).
- Per-block guess of (customer, deild) for each slice of time.
- A per-block split editor with rows of (customer, deild, %), available on every work block.
- One billing line per (customer, deild): the super block.
- A change log of automatic changes (surfaced live and as a catch-up) and of the Owner's own edits (catch-up only), each naming its source.

### 2.2 Non-goals

- Never auto-change a deild, customer or split the user set by hand. — user, Q7
- Notifications stay in the app: no email, phone or Slack. — user, Q7
- No changes to Tempo or Jira. — user, Q7
- No askrift/hourly flag or pricing on a deild. — user, Q1
- Reikningshæfi (billable) logic — the time clocking system derives it from the Verkefni. — user, idea
- Jira ticket assignment — user doesn't care about it for billing. — user, earlier in session
- No DB-level merge of blocks: a super block is a grouping only (D-03).

## 3. Journeys

### Journey 1 — Set up deildir (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Customer Sjúkra exists | Owner adds deildir "Rekstur" (keyword `rekstur`) and "Áskrift" | Both appear under Sjúkra and survive a reload |
| Error | Sjúkra already has "Rekstur" | Owner adds "Rekstur" again to Sjúkra | Save is refused with a visible message; the list is unchanged |
| Edge | APRÓ also has a deild named "Rekstur" | Owner adds it | Allowed: deild names are unique per customer, not globally |

### Journey 2 — Super blocks on a day (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Four blocks resolve to APRÓ·AI hraðall across two folders and two tickets | Owner opens the Billing view | One line APRÓ·AI hraðall holding all four, hours = union of their intervals |
| Error | The block's text matches no deild keyword under its resolved customer, and the folder has no default deild | Owner opens the Billing view | It lands on a line with a blank deild for that customer+folder, flagged as needing input |
| Edge | Two blocks on the same line overlap by 30 min | Owner reads the hours | The overlap counts once |

### Journey 3 — Split one block (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A vitinn-infra block | Owner saves Sjúkra·Rekstur 33 / Sjúkra·Áskrift 33 / APRÓ·AI hraðall 34 | Three lines each gain that block's share; the block shows "set by you" |
| Error | Rows total 90% or a row has no customer | Owner presses Save | Save stays disabled; nothing is written |
| Edge | Blocks are rebuilt (new ids) | Owner reopens the day | The split is still applied to the block with the same day and start time |

### Journey 4 — Notified of automatic changes (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Day page open; Claude re-describes 20 blocks | The estimate finishes | One pop-up "Claude changed 20 blocks · show"; show lists each change old → new |
| Error | Page closed during a Verdict re-route that moved 3 blocks' customer | Owner opens the app later | Catch-up says "3 changes since your last visit", source Verdict |
| Edge | A block has a hand-set split | Estimate rewrites its description | Its split is unchanged; only the description change is logged |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to add, rename and delete deildir under a customer, each with a name and keywords | Registry round-trip test + UI test |
| FR-02 | MUST | The system MUST reject a duplicate deild name within one customer | Test: second insert returns an error shown to Owner |
| FR-03 | MUST | A slice's deild MUST resolve in order: hand-set split row → exactly one keyword match among that customer's deildir → folder default deild when the slice's customer is the folder's pinned customer → blank | Unit tests, one per rung |
| FR-04 | MUST | Owner MUST be able to split any work block into rows of (customer, deild, %), repeating a customer | Save/load test with Sjúkra twice |
| FR-05 | MUST | The split MUST total 100% (±0.1) and every row MUST name a customer before it saves; a row's deild MAY be blank | Validation test |
| FR-06 | MUST | Billing lines MUST group by (customer, deild) when the deild is set, and by (customer, folder) when it is blank; Jira tickets MUST NOT split a line | rows_for_day test on the 2026-09-25 shape |
| FR-07 | MUST | A line's hours MUST stay the union of its slices' intervals | Overlap test |
| FR-08 | MUST | An automatic writer MUST NOT change a hand-set split or a manual description | Test: estimate over a manual block leaves shares byte-identical |
| FR-09 | MUST | Every change to a block's customer, deild, split or description made by an automatic writer MUST be recorded with old value, new value, source and batch | Change-log test per writer |
| FR-10 | MUST | While a day page is open, Owner MUST see one pop-up per new batch naming its source and count | Component test |
| FR-11 | MUST | On opening the app, Owner MUST see a catch-up of unseen changes, and opening it marks them seen | Component + route test |
| FR-12 | MUST | New blocks, deleted blocks and duration changes MUST NOT be notified | Test: rebuild that only adds a block logs nothing |
| FR-13 | SHOULD | Owner SHOULD be able to move a whole super block to another deild from its line header | Test: every slice on that line moves |
| FR-15 | MUST | The Owner's own edits to a block's customer, deild, split or description MUST be recorded with source "you", shown in the catch-up only, never as a live pop-up | Change-log test + component test |
| FR-14 | SHOULD | Existing folder Verkefni pins SHOULD become that customer's deildir on upgrade | Migration test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Live notice latency | ≤ 15 s from change to pop-up | Poll interval constant + component test with fake timers |
| Pop-up count | 1 per (batch, source) | Test: 20 changes, 1 toast |
| Catch-up retention | last 30 days of changes | Purge test |
| Security | N/A — single-user local daemon on a unix socket / 127.0.0.1; no new auth surface | — |
| Billing view cost | rows_for_day on a 30-block day ≤ 200 ms | cargo bench-style timing test |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised
- [ ] On 2026-09-25 real data: add deildir to Sjúkra and APRÓ; the Billing view shows one line per (customer, deild); split one vitinn-infra block Sjúkra·Rekstur 50 / APRÓ·AI hraðall 50 and both lines update; re-run estimate → one batched pop-up naming Claude as the source, and the hand-set split is unchanged. — user, Q10
- [ ] CLAUDE.md "Billing export" rule updated to the keyword amendment

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | A deild is the Verkefni text (confirmed Q1) | High | Export column wrong |
| A2 | Splits stay keyed by (day, started_at), since rebuilds change block ids | High | Splits lost on rebuild |
| A3 | Resolution is computed at read time, so change detection compares against a stored last-seen resolution per block | High | Missed or phantom notices |
| A4 | Registry edits (keywords, pins) that re-resolve blocks are logged with source "keyword guess" | Medium | Owner confused by self-caused notices |
| A5 | Concurrent Owner edit during an automatic batch: last write wins; the batch's refresh logs whatever differs afterwards | Medium | One extra notice |
| A6 | Block rebuild is logged as its own source "rebuild" | Medium | Mislabelled notices |
| A7 | Old `{customer: fraction}` splits read as rows with a blank deild | High | Existing splits vanish |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Deild | A Verkefni name under one customer; free text sent to the invoicing form |
| Slice | A block's time for one (customer, deild) |
| Super block | One billing line: every slice with the same (customer, deild) that day; slices with a blank deild group by (customer, folder) instead |
| Poll failure | The live poll failing (daemon down) shows nothing and retries next interval; the catch-up recovers anything missed |
| Batch | One run of one automatic writer (one estimate, one rebuild, one route) |
