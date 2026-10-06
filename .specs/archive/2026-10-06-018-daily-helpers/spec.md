# Spec: Daily helpers

**Created**: 2026-10-06 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Worklog collects almost everything about the Owner's day, but the Owner still double-checks Tempo by hand, rebuilds the morning standup from memory, cannot reverse a mistaken block edit, and has no quick way to ask "when did I last touch X". Some Tempo failures arrive with no reason at all.

**Solution**: Ten small helpers on top of the data worklog already has: Tempo errors that name their cause, one source for billed hours, an "already in Tempo?" check, a pre-send checklist, a one-click standup posted to the team's Daily thread, undo, ask, a monthly customer report, a footer nudge, and a 17:00 recap.

**Who it's for**: the Owner (single user, Icelandic IT consultant logging to Tempo and billing customers).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: anything that touches Verdict or the 17:00 run (§4.1 groups C and J) is built only after spec 017 "Verdict does more" is merged, on top of its code. Nothing here changes 017's auto-send, readiness rules or Review section.

## 1. Context

### 1.1 Problem statement

The Owner's daily log is mostly automatic (spec 017 adds a 17:00 auto-send). What is left is checking: a hand sync can fail with "HTTP 400 —" and nothing after the dash; the screen and Tempo compute billed hours in two separate places; a line can be sent while an equivalent hand-logged entry already sits on the same ticket. The morning standup ("Daily:thread") is typed from memory, and a fat-fingered block delete can only be repaired by re-inferring the day.

**Current workaround**: open Tempo's website to compare; type the standup by hand; re-run inference after a bad edit; search the database by hand.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day, sends to Tempo, posts the standup, bills customers | Sole user; wants the day "look-only" |
| Teammates | Read the Owner's reply in the Slack Daily thread | Never touch worklog |
| Verdict | Local classifier that picks one option from a list or abstains | Can be off or unreachable |

**Primary actor**: Owner.
**Hidden stakeholders**: the spec 017 session, which edits the same Tempo-send and 17:00 code; customers, who receive the hours the report summarises.

## 2. Scope

### 2.1 In scope

- A. Tempo errors carry the reason Tempo gave; billing lines with no start/end say "fill in".
- B. Billed hours come from one place; the web stops computing them itself.
- C. Before sending a line, worklog checks that ticket's existing Tempo entries for that day and skips the line when Verdict says it is the same work.
- D. A pre-send checklist for sends the Owner starts by hand.
- E. A one-click standup in the team's three-question format, posted as a reply in the Slack Daily thread after the Owner confirms.
- F. Undo for the Owner's last 20 block changes.
- G. Ask: a read-only search over saved blocks and the Owner's Claude prompts.
- H. A monthly per-customer report from the terminal.
- I. One nudge line in the Claude Code footer.
- J. A recap shown right after spec 017's 17:00 run.

### 2.2 Non-goals

- No automatic Slack posting: the standup is posted only by the Owner's click or confirm.
- No change to spec 017's auto-send, readiness rules, line checks or Review section.
- Nothing is ever deleted from Tempo.
- No web page for the report or for ask; both are terminal and Claude Code only.
- No change to which folders count as work (the "work outside ~/Desktop/Work" idea is dropped).
- No calendar-based features.

## 3. Journeys

### Journey 1 — Hand send to Tempo (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A day with 5 lines, all checks green | Owner presses Send to Tempo | Checklist shows 5 green rows; send runs; read-back row turns green |
| Error | One work block has no ticket | Owner opens Send | That row is red, names the block and links to it; Send is disabled; "Send anyway" link remains |
| Edge | GENAI-12 already has a 2.0 h hand entry describing the same work; worklog's line is 2.0 h | Send runs (after 017) | The line is skipped and shows "already in Tempo"; no second worklog is created |

### Journey 2 — Morning standup (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Today's "Daily:thread" message exists in the configured channel | Owner presses Standup on the Day page | Within 30 s an editable preview of the three answers appears; Post replies in the thread |
| Error | No Daily thread found today, or Slack refuses the post | Owner presses Post | Message names the cause ("no Daily thread today in #channel" / Slack's error); Copy button offered; nothing was posted |
| Edge | Yesterday had no blocks | Owner presses Standup | Answers come from open tickets only; question 1 says so plainly |

### Journey 3 — Undo (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Owner merged two blocks by mistake | Owner runs `undo` or clicks Undo | Both original blocks return with their tickets, hours and text |
| Error | The newest change touched a block already sent to Tempo | Owner runs undo | Refused with "block N was sent to Tempo; undo would desync it"; nothing changes |
| Edge | 21 changes made | Owner undoes 21 times | 20 succeed; the 21st says "nothing left to undo" |

## 4. Requirements

### 4.1 Functional requirements

**Groups** (from §2.1): A = FR-01..03 · B = FR-04..05 · C = FR-06..10 · D = FR-11..16 · E = FR-17..24 · F = FR-25..29 · G = FR-30..33 · H = FR-34..38 · I = FR-39..42 · J = FR-43..46. Groups C and J wait for spec 017.

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | When Tempo answers a write with a failure, the Owner MUST see Tempo's status and the body Tempo sent | Test: a 400 with body "Issue is closed" surfaces both in the sync report |
| FR-02 | MUST | When the failure body cannot be read, the Owner MUST see that reading failed and why, never an empty reason | Test: unreadable body yields a non-empty reason |
| FR-03 | MUST | A billing line with no start or end time MUST show "fill in" for that field instead of an empty value | Test: a line with no blocks renders "fill in" |
| FR-04 | MUST | Every billed-hours number the Owner sees on the day page MUST equal the number worklog sends to Tempo for the same line | Test: for 0, 899, 900, 1801 and 5100 s the screen value equals the sent value |
| FR-05 | MUST | The half-hour rounding rule MUST exist in exactly one place | Test/grep: the web has no rounding implementation |
| FR-06 | MUST | Before sending a line, worklog MUST read the Tempo entries on that line's ticket for that day | Test with a fake Tempo: the read happens before the write |
| FR-07 | MUST | When Verdict says an existing entry is the same work and its hours differ from the line's by at most 30 min (30 counts as same), worklog MUST NOT send the line and MUST mark it "already in Tempo" | Test: same-work fake at 30 and 31 min → no write at 30, write at 31; status shown |
| FR-08 | MUST | When Verdict says different work, worklog MUST send the full line as today | Test: different → one write |
| FR-09 | MUST | When Verdict is off, unreachable or abstains, worklog MUST send the line as today | Test: each of the three → one write |
| FR-10 | MUST | A line marked "already in Tempo" MUST NOT be re-checked or sent by later syncs unless the Owner edits it | Test: second sync makes no read or write for it |
| FR-11 | MUST | Before a send the Owner starts by hand, the Owner MUST see a checklist with one row per check: every work block has a ticket; no time counted twice; each day's hours vs Tempo's required hours; every line has text | Test: fixture with one failure per check shows four red rows |
| FR-12 | MUST | Each red row MUST name the block, line or day at fault | Test: row text contains the id/day |
| FR-13 | MUST | Send MUST be disabled while any row is red, with a separate "Send anyway" action | Test: button disabled; "Send anyway" sends |
| FR-14 | MUST | After a send, the Owner MUST see whether Tempo's read-back totals match what was sent, per day | Test: mismatch fixture shows a red read-back row |
| FR-15 | MUST | The terminal sync MUST print the same checklist and stop on red unless the Owner confirms | Test: CLI output lists rows; red without confirm sends nothing |
| FR-16 | MUST | The 17:00 auto-send MUST NOT run the checklist | Test: auto-send path does not call it |
| FR-17 | MUST | The Owner MUST be able to start a standup from the Day page and from the terminal | Test: both entry points return a draft |
| FR-18 | MUST | The draft MUST answer, in order: "What are you working on today?", "What is next/coming up?", "Are there any blockers we need to clear?" | Test: three numbered answers, in that order |
| FR-19 | MUST | The draft MUST be built from yesterday's and today's blocks, their tickets, merged PRs, open tickets and the first prompt of each Claude session (cut to 300 characters); every text passes through the same secret-removal step used for invoice text before it reaches the model, and that redacted text is the only model input | Test: a planted API key and bearer token never appear in the model input |
| FR-20 | MUST | The Owner MUST be able to edit the draft before posting | Test: edited text is what gets posted |
| FR-21 | MUST | Posting MUST reply in today's message beginning "Daily:thread" in the channel set in Settings, as the Owner; while no channel is set, Post is disabled and Copy is offered | Test with a fake Slack: reply goes to that message's thread; unset channel → Post disabled |
| FR-22 | MUST | Nothing MUST be posted without the Owner's explicit Post click or terminal confirm | Test: draft alone makes no post call |
| FR-23 | MUST | When the thread is missing, the channel is unset, or Slack refuses, the Owner MUST see the cause and a Copy action, and nothing is posted | Test: each case |
| FR-24 | SHOULD | The Owner SHOULD be able to regenerate the draft for a different wording | Test: the second request's model input contains the previous draft and a different-wording instruction |
| FR-25 | MUST | Every Owner block change (delete, merge, split, ticket, hours, text, personal, ignored) MUST be undoable, newest first | Test per change type |
| FR-26 | MUST | Worklog MUST keep the last 20 changes; older ones are forgotten | Test: 21st oldest is gone |
| FR-27 | MUST | Undo MUST refuse when it would touch a block already sent to Tempo, and change nothing | Test: synced block → refusal, DB unchanged |
| FR-28 | MUST | Undo MUST never clear or restore a Tempo sent-marker | Test: marker unchanged across undo |
| FR-29 | MUST | The web MUST offer Undo on the confirmation shown after a block change | Test: Undo appears and works |
| FR-30 | MUST | The Owner MUST be able to ask a free-text question from the terminal and from Claude Code | Test: both return results |
| FR-31 | MUST | Ask MUST return up to 5 matches, newest first, each with date, block time and ticket | Test: fixture query returns ≤5 ordered hits |
| FR-32 | MUST | "Where did I stop" for a repo MUST show the last 3 prompts and files touched in that repo | Test: fixture repo |
| FR-33 | MUST | Ask MUST NOT change any data | Test: DB byte-identical after ask |
| FR-34 | MUST | The Owner MUST be able to print a report for one customer and one month | Test: fixture month prints |
| FR-35 | MUST | The report MUST group hours by deild, with each line's Icelandic text | Test: grouping and text present |
| FR-36 | MUST | The report MUST show each deild's change from the previous month, or "n/a" when the previous month has no hours | Test: +/- hours shown; empty previous month → n/a |
| FR-37 | MUST | Unresolved customer or deild fields MUST stay empty, never guessed | Test: unresolved line has empty field |
| FR-38 | SHOULD | The report SHOULD export as a spreadsheet file | Test: CSV parses |
| FR-39 | MUST | The Claude Code footer MUST show at most one nudge line at a time | Test: never two lines |
| FR-40 | MUST | Nudges MUST cover: open GitHub PRs where the Owner is a requested reviewer; Jira tickets in status In Progress that have a merged PR mentioning their key; In Progress tickets whose latest event or block is older than 14 days ("activity" = newest event or block on that ticket) | Test: one fixture per kind |
| FR-41 | MUST | The shown nudge MUST change on each new prompt | Test: rotation |
| FR-42 | MUST | When nothing needs the Owner, no nudge line appears | Test: empty → no line |
| FR-43 | MUST | Right after spec 017's 17:00 run, the Owner MUST see a recap: what was sent, what was held back and why, coverage %, and the 3 largest unexplained gaps. Coverage % = seconds covered by blocks inside the day's work-hours window ÷ that window's seconds. A gap = a continuous stretch of at least 15 min inside the window with no block, ranked longest first | Test: fixture run produces all four parts with the stated maths |
| FR-44 | MUST | Each gap MUST offer Personal (creates a personal block over the gap: never billed, never sent), Break (records the gap as a break: never billed, never sent, removed from the gap list) and Pick a ticket (creates a work block over the gap with that ticket) | Test: each action resolves the gap and only Pick a ticket adds billable time |
| FR-45 | MUST | The recap MUST run inside 017's 17:00 run, never on its own timer | Test: no separate schedule entry |
| FR-46 | MUST | The recap MUST appear in the Claude Code footer and on the Day page | Test: both surfaces |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Standup draft time | ≤ 30 s from click to preview | Timed in the CHK on a real day |
| Footer cost | ≤ 50 ms added per prompt | Timing in the mod test with a warm cache |
| Nudge freshness | data ≤ 10 min old | Cache age asserted in test |
| Ask latency | ≤ 1 s over 12 months of data | Bench on the Owner's real database |
| Undo depth | 20 changes | Test FR-26 |
| "Same hours" tolerance | ≤ 30 min difference (30 counts) | Test FR-07 at 30 and 31 min |
| Tempo read before send | ≤ 10 s, then send as today (FR-09 path) | Test with a slow fake Tempo |
| Secrets in model input | 0 planted secrets reach it | Test FR-19 |
| Stale ticket | 14 days without activity | Test FR-40 |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised by a test
- [ ] §5 latencies measured on the Owner's machine, numbers written in verify/
- [ ] One real standup posted to the real Daily thread by the Owner (CHK)
- [ ] Groups C and J built on a base that contains spec 017

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Spec 017 merges before groups C and J start | Med | C and J wait; A–I ship without them |
| A2 | The Slack user token can get the post-message permission by re-authorising once | High | Standup falls back to Copy only |
| A3 | Today's Daily message is findable by searching the channel for "Daily:thread" posted today | Med | Owner pastes the thread link instead |
| A4 | The standup is written in English, short bullets, like the team template | High | Wording only |
| A5 | Every ticket group on the day page has a Tempo line, so billed hours can come from the line | Med | Groups without a line need a billed value added server-side |
| A6 | The web's overlap helpers are slider/band maths for the UI, not billing maths, so they stay | Med | Scope of group B grows |
| A7 | Ask needs no AI; plain text search is good enough | Med | Add an optional summary later |
| A8 | "Same work" is judged by Verdict on the two texts; hours are compared by code | Med | More or fewer skips than wanted |

## 8. Open questions

None open. The Daily-thread channel is resolved as a Settings field the Owner fills in (FR-21); until then Post is disabled.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Line | One Tempo worklog worklog would send: one ticket, one day, union of its blocks' time |
| Billed hours | A line's time rounded up to the next half hour, 0 under 15 min |
| Daily thread | The day's Slack message starting "Daily:thread" where the team replies |
| Verdict | The local classifier; picks one option or abstains |
| Deild | A department/project of a customer on the invoice form (Verkefni) |
| Sent marker | The stored Tempo worklog id that means "already sent" |
