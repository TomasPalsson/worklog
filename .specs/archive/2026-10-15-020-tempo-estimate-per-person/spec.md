# Spec: Estimate progress per person on day-page blocks

**Created**: 2026-10-09 · **Route**: dispatch · **Prep**: PREP.md (8 questions, D-01…D-06) · **Mock**: mock.html (approved, D-05)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The team now agrees hours per ticket up front (the Jira "Estimated" field, e.g. 4h on GENAI-1897). On a block, though, the Owner can't see how much of that is already used, or by whom. Today they open Tempo's Time Tracking panel one ticket at a time.

**Solution**: When a block's ticket has an estimate, the block on the day page shows a progress bar of the ticket's hours used against the estimate, split per person. This block's not-yet-synced hours show as a striped piece. A small running-total chart sits beside the bar.

**Who it's for**: the Owner (the developer reviewing their day in worklog).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Per-person hours come from the ticket's Jira worklogs. Tempo Cloud stores its worklogs there, so this is the same data as Tempo's "Collaborators" panel. No new Tempo permission or endpoint is needed.

## 1. Context

### 1.1 Problem statement

The Owner logs time on tickets where a lead (e.g. RL) has set a fixed allowance, and several people log on the same ticket. Worklog shows the Owner's own hours. It does not show the allowance, the teammates' hours, or whether today's block will push the ticket over.

**Current workaround**: open each ticket in Jira and read Tempo's Time Tracking panel (Estimated / Logged / Collaborators).

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews and syncs their blocks on the day page | Logs on tickets shared with teammates |
| Teammate (e.g. Jón Geir) | Logs time on the same tickets in Tempo | Never uses worklog; appears only as data |

**Primary actor**: Owner.
**Hidden stakeholders**: the lead who sets the estimate in Jira (reads the same numbers in Tempo).

## 2. Scope

### 2.1 In scope

- For each day's tickets, read the estimate and every author's logged hours from Jira. Cache them and refresh them.
- On each day-page block, show a stacked per-person bar against the estimate, a legend, and a running-total chart, as in mock.html.
- Count this block's not-yet-synced hours as a striped "+this block" piece, and say "once synced" (D-06).

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Ticket views (Tasks board card, ticket popup) are not the target; leave them as they are. — user, Q3
- No editing the estimate from worklog. It is set in Jira. — user, Q7
- No Slack pings, pop-ups or notifications when a ticket goes over. — user, Q7
- Never block or warn on "Sync to Tempo" because of the estimate. — user, Q7
- No phone/mobile layout for this. Desktop only. — user, mid-mock
- No display of Jira's "Remaining" estimate; only Estimated vs Logged.

## 3. Journeys

### Journey 1 — See the ticket's budget on a block (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | GENAI-1897 has estimate 4h; Jira holds Owner 2h, Jón Geir 2h 30m; today's 13:00–14:00 block (1h) is not in Tempo | Owner opens the day page | The block shows "5h 30m of 4h once synced · 1h 30m over", bar segments You 2h / Jón Geir 2h 30m / striped +1h, a flag at 4h, and a running-total chart |
| Error | Jira does not answer and nothing is cached | Owner opens the day page | Each affected block shows "Couldn't load hours for GENAI-1897 — Jira didn't answer. Your blocks are safe. Try again" |
| Error | Jira does not answer but older numbers are cached | Owner opens the day page | The block shows the cached numbers with their "Jira numbers from HH:MM" time |
| Edge | GENAI-1880 has no estimate | Owner opens the day page | The block shows one muted line telling the Owner to set "Original estimate" in Jira; no bar, no chart |

### Journey 2 — Keep the numbers fresh (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Cached numbers are older than 10 minutes | Owner opens or reloads the day page | Numbers refresh from Jira; the page reads "Jira numbers from HH:MM" with the new time |
| Error | Refresh fails (timeout, 5xx, 401/403, 429) | — | Cached numbers stay on screen with their old time; nothing is deleted; with no cache, the error line and "Try again" show |
| Edge | Owner syncs the block to Tempo, then reloads | Page reloads after refresh | The striped piece is gone and those hours sit in the Owner's segment (no double count) |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The system MUST read a ticket's original estimate from Jira | collector test: estimate 14400 s parsed for a mocked ticket |
| FR-02 | MUST | The system MUST read every author's worklogs on a ticket (author id, display name, day, seconds), across all pages | collector test with 2 pages and 2 authors |
| FR-03 | MUST | The system MUST cache estimates and worklogs, and refresh a ticket when its cache is older than 10 minutes | store test: a fresh cache causes no fetch; an 11-minute-old cache causes a fetch |
| FR-03a | MUST | When the Owner syncs blocks to Tempo, the system MUST mark those tickets' cached numbers stale, so the next page load refetches them | daemon test: sync GENAI-1897 → next `/progress` fetches it even though the cache is 1 minute old |
| FR-04 | MUST | On a fetch failure the system MUST keep the cached numbers and report the error per ticket | daemon test: Jira 500 → cached view + error flag |
| FR-04a | MUST | The Owner MUST see when the numbers were fetched ("Jira numbers from HH:MM") | component test |
| FR-04b | MUST | When a ticket's numbers failed to load, the Owner MUST get a "Try again" action that refetches only that ticket | component test |
| FR-05 | MUST | The Owner MUST see "used of estimate" on each block whose ticket has an estimate | component test |
| FR-05a | MUST | The Owner MUST see "X left" or "X over" in words beside it | component test |
| FR-06 | MUST | The bar MUST show one segment per person: the Owner first, then teammates by hours, most first | lib test |
| FR-06a | MUST | With more than 4 people, the bar MUST fold the rest into one "Others" segment | lib test |
| FR-07 | MUST | A block not yet synced to Tempo MUST add its own duration as a striped "+this block" segment (D-06) | lib test |
| FR-07a | MUST | When FR-07 applies, the head MUST say "once synced" | component test |
| FR-08 | MUST | A flag MUST mark the estimate on the bar | component test |
| FR-08a | MUST | Hours past the estimate MUST sit in an amber zone | component test |
| FR-09 | MUST | The tone MUST be "on track" under 80% used, "running low" at 80–100%, and "over" past 100%, each with an icon and words | lib test at 79%, 80%, 100%, 101% |
| FR-10 | MUST | The legend MUST list each person with initials and hours | component test |
| FR-10a | MUST | When FR-07 applies, the legend MUST end with "This block, not in Tempo yet +Xh" | component test |
| FR-11 | MUST | Each block MUST show a running-total chart of the ticket's hours by day, per person, with the estimate as a dashed line | component test |
| FR-11a | MUST | The chart MUST cover the ticket's last 14 logged days, with older hours folded into the first point, and days MUST be bucketed in `$WORKLOG_TZ` | lib test with 20 logged days |
| FR-12 | MUST | The chart MUST offer a "Show as table" view of the same numbers | component test |
| FR-13 | SHOULD | Hovering, tapping or arrow-keying the chart SHOULD show that day's running total per person | component test |
| FR-14 | MUST | A ticket with no estimate (absent or 0) MUST show one muted line pointing to Jira's "Original estimate", and nothing else new | component test |
| FR-15 | MUST | While the numbers load, the block MUST show "Loading hours from Jira…" and a placeholder bar | component test |
| FR-15a | MUST | The rest of the page MUST NOT wait for the estimate numbers | the day page server render does not await the estimate call (test + review) |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Jira calls per refresh | 1 estimate search per day + 1 worklog request per page (100 per page) per stale ticket; 0 when all are fresh | daemon test counts mocked requests |
| Jira timeout | a request that takes over 30 s counts as "Jira didn't answer" (existing `http::client`) | collector test with a delayed mock |
| Cached response time | `/progress/:day` answers in < 200 ms when every ticket is fresh | daemon test timing a fresh-cache call |
| Cache freshness | 10 minutes | store test with a fixed clock |
| Accuracy | per-person totals equal Jira's logged total to the second | store test: sum of people == timeSpentSeconds fixture |
| Visible people | ≤ 4 named + "Others" | lib test with 6 authors |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error paths of Journey 1 and Journey 2 are exercised in a test
- [ ] Owner check (PREP Verify): open the day page on a GENAI-1897 block not yet in Tempo. It shows "5h 30m of 4h once synced · 1h 30m over", You 2h, Jón Geir 2h 30m, a striped +1h and a running-total chart. The numbers match Tempo's Time Tracking panel (Logged + Collaborators) for that ticket.
- [ ] The UI matches mock.html v3 in light and dark mode

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Tempo Cloud worklogs appear as Jira worklogs with the author's displayName (the same numbers as Tempo's Collaborators panel) | Med | Need Tempo `/worklogs/issue/{id}` plus a name lookup instead; T002 changes, the rest holds |
| A2 | "You" is the Jira account id already cached by `resolve_account_id` | High | The Owner shows as a teammate and the colour order is wrong |
| A3 | A block counts as "not in Tempo yet" when its Tempo id is empty or NULL (`tempo::normalise_tempo_id`); an edited-after-sync block counts as synced | High | Edited blocks under-count by their delta |
| A5 | Teammate initials are the first letters of the first two name parts | High | Cosmetic |
| A6 | PREP A-01…A-05 hold: the estimate is fetched today only on ticket detail, Tempo read-back is own-user only, and the BlockCard ticket row is the slot | High | — |
| A7 | The design-review leftovers in PREP Discretion (bar-segment initials, a stronger "over", 11–12px labels, teal contrast, the legend gap) are built in the UI task, not added as requirements | Med | A small polish pass |

## 8. Open questions

None. The chart shows on every block, including several blocks of one ticket on one day.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Estimate | Jira "Original estimate" (Tempo shows it as "Estimated") |
| Used | All authors' logged hours on the ticket, plus this block if not yet synced |
| Running total | Hours logged on the ticket up to the end of each day |
| Once synced | The total after this block is sent to Tempo |
