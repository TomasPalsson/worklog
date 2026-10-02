# Spec: Tempo hub

**Created**: 2026-10-01 · **Route**: dispatch · **Prep**: PREP.md (D-01: design delegated)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Owner logs work to Tempo through worklog, but everything around the worklog lives elsewhere. To move a ticket to Done or leave a comment they open Jira. To check the week is full they open Tempo. Worklog never reads Tempo back, so a worklog typed in Tempo by hand can be sent a second time.

**Solution**: Two new screens. **My Tasks** lists the Owner's tickets with hours this week; the Owner changes status, posts a comment, or asks the AI to draft the update (comment + suggested status) in one click. **Week close-out** on the week page shows each day against Tempo's required hours, pulls what is already in Tempo, and sends the whole week in one confirmed click, never twice.

**Who it's for**: The Owner.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Read-back is a guard, not a merge. A Tempo worklog made outside worklog is shown and blocks a duplicate send for that day × ticket; worklog never edits or deletes it.

## 1. Context

### 1.1 Problem statement

Ticket picking, line text and line hours already work (spec 011). What is left is the round trip. Jira is search-and-create only today: no status change, no comment. Tempo is write-only: nothing reads worklogs back. Sync runs one day at a time. The Owner closes a week by visiting seven day pages and two other apps.

**Current workaround**: A Jira tab for status and comments, a Tempo tab to check hours, and seven Sync clicks.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews tasks, updates tickets, closes the week | Wants the fewest clicks possible |
| AI drafter | Writes a draft ticket update from recent line texts | Never posts; the Owner always presses Post |

**Primary actor**: Owner.
**Hidden stakeholders**: The team lead who reads Jira comments and Tempo worklogs.

## 2. Scope

### 2.1 In scope

- My Tasks page: assigned open tickets plus any ticket worked this week, with this-week and today hours.
- Change a ticket's Jira status from the page, using the ticket's live transitions.
- Post a Jira comment from the page.
- AI draft: comment text and one suggested transition, editable before posting.
- Pull a week's Tempo worklogs and required hours into worklog.
- Sync skips a day × ticket that already has a worklog made outside worklog.
- Week close-out panel: per-day logged vs required vs in-Tempo, gaps flagged (FR-12e), one-click week sync.
- Links to My Tasks from the day and week headers.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No automatic Tempo sync; the Owner still confirms every sync (carried from 011).
- The AI never posts a comment or changes a status by itself.
- Worklog never edits or deletes a Tempo worklog it did not create.
- No Jira edits beyond status and comment (no assignee, estimate, priority or field edits).
- No ticket creation on My Tasks (the block ticket picker already does it).
- No scheduler change: `worklog day` does not pull Tempo or refresh Jira.
- No new target-hours setting; required hours come from Tempo only.
- Billing export is unchanged.
- No named time zones; the day offset stays fixed.

## 3. Journeys

### Journey 1 — Update a ticket (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | ABC-12 is In Progress with 3h logged this week | Owner presses "Draft update", edits the text, keeps the suggested "Done", presses Post | Jira has the comment and the status Done; the card shows Done under "Worked this week". |
| Error | Jira rejects the transition (workflow rule) | Owner presses Post | The card shows Jira's error text; the draft text stays in the box. |
| Edge | The AI returns a transition id not in the live list | Draft finishes | The text is shown; no transition is preselected. |

### Journey 2 — Close the week (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Mon–Fri have ticket lines; Tempo requires 8h a day | Owner presses "Sync week", then confirms | Worklog pulls from Tempo, sends each day with pending lines in order, pulls again; every day with no pending lines and no gap shows a check mark. |
| Error | The Tempo token is missing or wrong | Owner presses "Pull from Tempo" | The panel shows the error; previously pulled numbers stay. |
| Error | Thursday's sync fails (Tempo 5xx or timeout) | Sync week reaches Thursday | The loop stops; the panel names Thursday and the error; Friday is not sent. Pressing Sync week again sends only lines still pending. |
| Edge | Wednesday has a 2h ABC-12 worklog typed in Tempo, and worklog has an unsynced ABC-12 line that day | Owner syncs the week | Wednesday ABC-12 is skipped with "already in Tempo — logged outside worklog"; the typed worklog is untouched. |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST see every cached assigned ticket whose status category is not done | board query test with mixed categories |
| FR-02 | MUST | Owner MUST see any other ticket that has a ticket line this week, under "Worked this week" | board query test with an unassigned worked ticket |
| FR-03 | MUST | Each task MUST show this-week and today hours as the sum of its ticket lines' effective hours | board query test with an hours override |
| FR-04 | MUST | When a Jira refresh has read every result page, the refresh MUST mark cached, non-picked tickets it did not return as done | mock with 201 tickets over 2 pages: none marked done; mock omitting one: it is marked done |
| FR-04a | MUST | If a refresh fails before the last page, the refresh MUST mark nothing done | mock failing on page 2: categories unchanged |
| FR-05 | MUST | Owner MUST be able to list a ticket's live transitions | transitions mock test |
| FR-06 | MUST | Owner MUST be able to apply one transition, after which the cached status equals Jira's new status | transition mock test |
| FR-07 | MUST | Owner MUST be able to post a comment of 1–5000 characters | comment mock test; 400 on empty or too long |
| FR-08 | MUST | AI drafter MUST return comment text and at most one transition id from the live list | draft test with a fake model returning a foreign id |
| FR-09 | MUST | Owner MUST be able to pull one week's Tempo worklogs and required hours | pull mock test |
| FR-10 | MUST | A pulled worklog MUST be tagged "worklog" when its id is on a block, else "outside" | pull test with one of each |
| FR-11 | MUST | Sync MUST skip an unsynced day × ticket line that has an "outside" worklog for the same day and issue | sync mock test asserting no POST |
| FR-12 | MUST | Week close-out MUST show each day's logged hours (sum of its ticket lines' effective hours) | 2 lines of 1.5h and 2h → 3.5h |
| FR-12a | MUST | Week close-out MUST show each day's synced hours (lines whose blocks are all synced and clean) | 1 synced + 1 dirty line → only the synced one counts |
| FR-12b | MUST | Week close-out MUST show each day's in-Tempo and outside hours from the last pull | 1 worklog + 1 outside row → both totals right |
| FR-12c | MUST | Week close-out MUST show each day's required hours, or "not pulled" when never pulled | never pulled → None |
| FR-12d | MUST | Week close-out MUST show each day's unticketed hours and pending-line count | 30 min untagged block, 1 dirty line → 1800 s, 1 |
| FR-12e | MUST | Week close-out MUST flag a day as a gap when required > 0 and in-Tempo < required | required 8h: in-Tempo 8h → no flag; 7.5h → flag; required 0 → no flag |
| FR-13 | MUST | Owner MUST be able to sync a whole week with one confirmed action, one day at a time, only days with pending lines | 3 pending days → 3 day syncs in order |
| FR-13a | MUST | If a day's sync fails, the week sync MUST stop, name that day and its error, and leave later days unsent | day 2 of 3 fails → day 3 never called |
| FR-13b | MUST | While a pull or week sync runs, the Owner MUST NOT be able to start another, and Post on a task card MUST be disabled while it is in flight | buttons disabled while pending |
| FR-14 | MUST | Day and week headers MUST link to My Tasks | header render test |
| FR-15 | MUST | Week sync MUST pull from Tempo before and after sending | call order: pull, days, pull |
| FR-16 | SHOULD | A task card SHOULD link to the ticket in Jira | render test |
| FR-17 | MAY | A task card MAY show a seven-day hours strip | 7 bars, one per day, heights ∝ effective hours |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| My Tasks load | p95 < 1 s with 200 cached tickets; 0 Jira or Tempo calls on load | board test with 200 rows; route reads local data only |
| Daemon lock | 0 routes hold the shared connection during a Jira, Tempo or AI call | review against the read / call / write split |
| Pull size | One week = ceil(N/1000) worklog requests + 1 schedule request | mock call counts |
| AI draft | Times out at 60 s; a failure stores nothing and shows an error | draft error test |
| Comment length | 1–5000 characters | validation test |
| Security | New routes are served only on the daemon's existing local socket and 127.0.0.1 port (no new listener); 0 log lines contain a token, a comment body or draft text | grep of new code for log calls; route test |
| AI data | The draft prompt holds exactly: ticket key, summary, status, transition names, and ≤20 line texts from the last 14 days — nothing else | prompt-builder unit test asserts the field set |
| Upstream auth | A Jira or Tempo 401/403/429 is shown to the Owner as that service's error text; nothing is stored | mock tests per status |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test.
- [ ] The error path of each journey is exercised by a test.
- [ ] All project gates are green (Rust tests, lint, format; web tests, typecheck, build).
- [ ] **Owner check**: on My Tasks, move one real ticket to a new status and post an AI-drafted comment — both show in Jira. On the week page, Pull from Tempo shows today's hours matching Tempo, and a second Sync week reports 0 synced.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Owner delegated scope and design (PREP D-01); these choices stand unless corrected | High | Re-scope via `--amend` |
| A2 | The existing Tempo token may read the Owner's worklogs and schedule | Med | Pull fails; close-out shows local numbers only |
| A3 | "Hours" everywhere means the ticket line's effective hours — what Tempo gets | High | Numbers disagree with sync |
| A4 | Week = Monday–Sunday in the configured day offset, same as the week page | High | Off-by-one day at week edges |
| A5 | Matching outside worklogs on (day, Jira issue) is enough; start time is ignored | Med | Two same-ticket entries in a day count as one |
| A6 | Week sync loops days from the browser, reusing the per-day sync and its timeout | High | A week route would hold the lock for minutes |
| A7 | Required hours of 0 (weekend, holiday) mean "no target" and are never flagged short | High | Weekends flagged red |
| A8 | Refresh reads every page (Jira's next-page token) before marking anything done | High | Tickets wrongly hidden |
| A9 | Pulled data is a snapshot; numbers show when they were pulled and go stale until the next pull | High | Owner trusts old numbers |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Ticket line | One day × Jira ticket; becomes one Tempo worklog (spec 011) |
| Effective hours | A line's override if set, else its blocks' union rounded to the half hour |
| Outside worklog | A Tempo worklog whose id is on no block — typed in Tempo, not sent by worklog |
| Required hours | Tempo's per-day schedule for the Owner |
| Unticketed | Work block time (not personal, not ignored) with no Jira ticket |
| Pending line | A ticket line with blocks that are unsynced or dirty |
| Gap | A day with required > 0 and in-Tempo < required |
