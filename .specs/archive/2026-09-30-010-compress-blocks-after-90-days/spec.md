# Spec: Compress blocks after 90 days

**Created**: 2026-09-30 · **Route**: dispatch · **Design**: design.md

## TL;DR

**Problem**: Worklog stores about 5 MB of raw activity a day (≈1.8 GB/year) and today solves that by *deleting* whole blocks after two billing cycles. The Owner wants every block kept forever, so `worklog eval` and future time estimates can look back at past work.

**Solution**: After 90 days a block keeps its row and gains a small summary card (measured ≈0.5 KB on the Owner's data); the raw events behind it are deleted. Eval, billing, the day view and the detail panel read the card for old blocks, and old days can no longer be rebuilt from events they no longer have.

**Who it's for**: the Owner (sole user), and Claude when it answers "how long did / will X take".

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1.

**Key decision**: blocks are never deleted again; only raw events are, and only after their block's card is written in the same transaction.

## 1. Context

### 1.1 Problem statement

Raw events (mostly Claude tool-call payloads) are 99% of the database. The current prune deletes blocks, events and sessions older than the just-closed billing cycle, so anything older than ~2 months is gone for eval and for estimating. The Owner wants the history, not the raw detail.

**Current workaround**: none — old blocks are deleted; ad-hoc `.bak` copies of the DB sit in the data dir.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | reviews days, exports billing, runs `worklog eval` | one person, one machine |
| Estimator (Claude) | reads past blocks to judge how long similar work took | needs *what was done*, not raw payloads |
| Daemon | runs the compression on a schedule | unattended; must never lose a block |

**Primary actor**: Daemon. **Hidden stakeholders**: the invoicing form — an old cycle re-exported must produce the same lines.

## 2. Scope

### 2.1 In scope

- A per-block card written once, in the same transaction that deletes that block's raw events.
- Rolling 90-day compression, run automatically at most once a day and by hand via the existing purge command.
- Every reader of a block's events uses the card when the events are gone.
- A fixed list of actions refused on compressed days (§4.2).
- `worklog eval --details` prints the cards of matched blocks.
- The billing-cycle block deletion is retired.

### 2.2 Non-goals

- No zipping or shrinking of data younger than 90 days.
- No export to an archive file outside the database.
- No cleanup of existing `.bak` files; the single pre-run snapshot file `worklog.db.preprune` keeps today's overwrite behaviour.
- No new similarity-search command; the card is readable via eval and plain SQL.
- No web changes beyond showing the card in the block detail panel.
- No change to hand-set data (splits, change trail, snapshots, billing registry) — kept as today.

## 3. Journeys

### Journey 1 — Daily compression (Daemon)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | blocks on day D < horizon, no card yet | the daily run fires | each block has a card; rows in §4.3 are gone; block rows byte-identical |
| Error | the card build fails for any block | the run fires | the transaction rolls back: zero cards, zero deletes; error logged to stderr; the latch is not advanced, so the next run retries |
| Error | the snapshot write fails | the run fires | zero cards, zero deletes |
| Edge | the run already happened today | it fires again | no-op |

### Journey 2 — Eval over old work (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | fixture: block A (recent) and block B (compressed), both matching | `worklog eval "<q>"` | matches and total equal the pre-compression run on the same fixture |
| Edge | `--details` | same query | after the table, each matched block prints its non-empty card fields |

### Journey 3 — Touching an old day (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a compressed day | open it in the review UI | blocks show; the detail panel shows the card instead of an event list |
| Error | a compressed day | any refused action in §4.2 | refused with `day is compressed: <YYYY-MM-DD>` (HTTP 409 / CLI exit 2); blocks byte-identical |
| Edge | the billing fixture in `billing_lifecycle_test` | export its cycle before and after compressing | identical lines (folder, customer, deild, hours, text) |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Daemon MUST write a card for every block with `day` < horizon that has none, in the run's transaction, before any delete | test: after a run, every such block has a card |
| FR-02 | MUST | Daemon MUST NOT delete any block | test: block count and rows equal before/after a run, incl. unbilled and personal blocks |
| FR-03 | MUST | Daemon MUST delete the rows listed in §4.3, by the rules in §4.3 | test per row type |
| FR-04 | MUST | The card's eval fields MUST equal what eval sends today (repos; ≤6 titles × 80 chars, same order, unscrubbed) | test: byte-equal before/after |
| FR-05 | MUST | The card MUST hold the billing fields listed in `digest_contract.rs` (normative, with caps) | test: billing export equality (Journey 3 edge) |
| FR-06 | MUST | The card MUST hold the estimation fields listed in `digest_contract.rs`, each within its cap | test: fixture day fills every field; an oversized fixture is cut to every cap |
| FR-07 | MUST | Prompts, change titles, branches and files on the card MUST pass through the scrubber the clue sender already uses (`scrub_secrets`), before truncation | test: a `ghp_…`-shaped token in a prompt is replaced |
| FR-08 | MUST | Personal blocks MUST get empty prompts, files, change titles, branches and tool counts | test: personal fixture |
| FR-09 | MUST | Every action in §4.2's refuse-list MUST fail on a compressed day with the exact text | one test per action |
| FR-10 | MUST | Every action in §4.2's allow-list MUST keep working on a compressed day | one test per action |
| FR-11 | MUST | Collectors MUST clamp `since` to the horizon | test |
| FR-12 | MUST | Day summary MUST take event count, sources and path from the card when present | daemon test |
| FR-13 | MUST | The detail panel MUST show the card when a block has no events | UI test |
| FR-14 | MUST | `worklog eval --details` MUST print each matched block's card fields | CLI test |
| FR-15 | MUST | `worklog db purge [--days N] [--dry-run]` MUST run the compression with horizon today−N (default 90, N ≥ 1) and print: horizon, blocks carded, card bytes median/max, events/sessions/cache rows deleted, bytes freed; exit 0 | CLI test; dry-run changes zero rows |
| FR-16 | MUST | The billing-cycle cutoff MUST no longer drive any delete; 90 days is the only boundary | test: no block deleted with default cycle settings |

### 4.2 Compressed-day rules

A day is **compressed** when any block with that `day` has a card. Runs are all-or-nothing, so a day is never half-carded by a run.

- **Refused**: regenerate/re-infer a day (review UI, `POST /infer`), save or delete an overlap allocation, `worklog infer --day`, `worklog day` rebuild, same-ticket merge, LLM re-description.
- **Allowed**: hand-edit description, customer split, ignore/unignore, personal flag, ticket change, mark exported, Tempo sync.

### 4.3 What a run deletes (horizon H = local midnight of today−N in `$WORKLOG_TZ`, as UTC)

- events with `started_at` < H **unless** linked to a block whose `day` ≥ H's day (a block spanning midnight keeps its events); their `block_events` links go by cascade
- sessions with `started_at` < H and no remaining events
- transcript-cache rows whose window ends before H
- session pins whose session is gone, **after** cards are written

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Horizon | 90 days | unit test on H |
| Card size | median ≤ 1 KB, max ≤ 4 KB | dry-run report on the Owner's DB copy |
| Growth | ≤ 10 MB/year of carded blocks | card bytes × blocks/year |
| Frequency | ≤ 1 run per calendar day | latch test |
| Run time | ≤ 60 s for 90 days of backlog on the Owner's Mac | CHK001 timing |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] Every refused and allowed action in §4.2 has a test
- [ ] Dry-run on a copy of the Owner's DB with `--days 3` reports card median/max and freed bytes within §5
- [ ] Owner runs one eval query on that copy before and after compressing it and sees the same total

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Owner accepted discovery positions 1–10 by changing none | High | re-plan |
| A2 | The card exists for eval + estimating, per the Owner's reply | High | wrong fields kept |
| A3 | §4.2's split between refused and allowed actions | Med | Owner cannot regenerate an old day |
| A4 | A card is frozen once written; later relabels don't change it | High | stale evidence |
| A5 | One transaction per run is fast enough (the Owner's backlog is 10 days today) | Med | split into per-day batches |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Card | the per-block summary (`BlockDigest`, `digest_contract.rs`) |
| Horizon | H in §4.3 |
| Compressed day | §4.2 |
