# Spec: Logged — see what's really in Tempo, and one home for Jira/Tempo

**Created**: 2026-10-04 · **Route**: dispatch · **Prep**: PREP.md (D-01…D-09, V-01)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Owner must log time in Tempo but has to open Tempo's website to see what is actually logged per day, and worklog's Jira/Tempo screens are scattered across pages and tangled with Billing, which the Owner no longer uses.

**Solution**: A new **Logged** section shows the real Tempo entries by month, week and day, fetches them on its own, flags past days under Tempo's target (dismissable with a reason like "dentist"), and one shared top menu — Day · Week · Tasks · Logged · Settings · Billing — replaces the per-page navigation, with Billing tucked onto its own page.

**Who it's for**: The Owner — the single person running worklog and logging time to Tempo.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Logged is read-only and Tempo stays the system of record (D-01); the only thing worklog writes for it is the Owner's own dismissal note, which never leaves worklog.

## 1. Context

### 1.1 Problem statement

worklog already pulls the Owner's real Tempo worklogs and Tempo's required hours per day (PREP A-02), but only one week at a time behind a "Pull from Tempo" button, and only into a small totals table (A-03). To see what is really logged on a given day, the Owner opens Tempo's website. Meanwhile Jira/Tempo controls live on the day page, the week page, Tasks and Settings, each page with its own header, and Billing UI is mixed through all of them (A-06, A-07, A-08).

**Current workaround**: Open Tempo's website; click "Pull from Tempo" week by week.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Logs work to Tempo, reviews worklog's estimates, sends lines to Tempo | Must use Tempo; does not want to open its website; no longer uses Billing |

**Primary actor**: Owner.
**Hidden stakeholders**: Tempo's API (auth, rate); the purge job, which reads billing marks (A-09).

## 2. Scope

### 2.1 In scope

- A Logged section with month, week and day views of the Owner's real Tempo worklogs (D-02, D-06).
- Auto-fetch from Tempo of the shown range on open, plus a refresh button (D-05).
- An "is this filled out?" flag on past days logged under Tempo's required hours for that day (D-03, D-04).
- Dismissing a day's flag with a short reason, stored only in worklog (D-03).
- One shared top menu on every page: Day · Week · Tasks · Logged · Settings · Billing (D-09).
- Billing removed from every Jira/Tempo screen and reachable only via `/billing` (D-07, D-08).

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Removing, hiding or disabling any Tempo sync code. — user, Q1
- Creating, editing or deleting Tempo entries from the overview — it is read-only. — user, Q5
- Deleting Billing code, tables or the `exported_at` purge rule. — user, Q8
- Sending dismissal reasons ("dentist") to Tempo — they stay in worklog only. — user, Q5
- Named time zones, other people's Tempo time, team views.
- Replacing Tempo's own reports (approvals, team timesheets, accounts).

## 3. Journeys

### Journey 1 — Check this month (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Tempo has entries this month | Owner opens Logged | The current month shows each day's logged vs required hours from stored data at once, then refreshed from Tempo |
| Error | Tempo is unreachable or the token is bad | Owner opens Logged | Stored data still shows, with "Couldn't reach Tempo — showing data from <time>"; nothing stored is lost |
| Edge | A range was never fetched | Owner opens it | Days read "not fetched yet", never "0h" |
| Error | A range was never fetched and Tempo fails or no Tempo token is set | Owner opens Logged | Days read "not fetched yet" and the banner reads "Couldn't reach Tempo — nothing stored yet" |

### Journey 2 — Look at one day (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A day has 3 Tempo entries | Owner opens that day | Each entry shows ticket, hours, description, and whether worklog sent it or it was hand-logged |
| Error | An entry's ticket isn't cached | Owner opens the day | The entry shows its Tempo issue id instead of a key |
| Edge | A holiday: required 0, no entries | Owner opens the day | No flag; reads as a day off |

### Journey 3 — Dismiss a short day (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A past day has 5h of an 8h target | Owner dismisses it with "dentist" | The flag is replaced by "dentist"; still so after reload |
| Error | Reason is empty or > 80 chars | Owner submits | Nothing is saved; the field says why |
| Edge | The Owner later logs the missing hours | The day reaches target | The day reads as full; the stored reason is not shown as a flag |

### Journey 4 — Move around (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Any page | Owner clicks a menu item | The same menu shows on every page with the current section marked |
| Edge | Owner opens an old `/billing?from=` link | Page loads | Billing still works there |
| Edge | Day page | Owner looks for Billing | No billing toggle, export, pins or invoice text on Day, Week or Tasks |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST see a month view listing every day of the month with logged hours and required hours | component test |
| FR-02 | MUST | Owner MUST see a week view of 7 days with per-day totals and that week's entries | component test |
| FR-03 | MUST | Owner MUST see a day view listing each Tempo entry: ticket, hours, description, and source (sent by worklog = `owner worklog`, hand-logged = `owner outside`) | component test |
| FR-04 | MUST | Owner MUST be able to go to the previous/next month, week and day, and from a month day into its day view | component test |
| FR-05 | MUST | Opening a Logged view MUST render stored data first, then fetch the shown range from Tempo and update | component test: stored data renders before `refreshLogged(from,to)` resolves; it is called once on mount |
| FR-06 | MUST | Owner MUST have a refresh button that re-fetches the shown range | component test |
| FR-07 | MUST | A re-fetch MUST replace the stored range so entries deleted in Tempo disappear | Rust test |
| FR-08 | MUST | A Tempo failure MUST leave stored data untouched and show the error with the last fetch time, or "nothing stored yet" when there is none | Rust test + component test |
| FR-09 | MUST | A day before today (daemon local date, `$WORKLOG_TZ`) with required > 0, logged < required and no dismissal MUST be flagged "is this filled out?" | Rust test |
| FR-10 | MUST | A day with required = 0, required unknown, or today/later MUST NOT be flagged | Rust test |
| FR-11 | MUST | Owner MUST be able to dismiss a flagged day with a reason of 1–80 characters that survives reload | Rust route test + component test |
| FR-12 | MUST | The dismiss path MUST make no Tempo call | Rust route test with no Tempo mock reachable |
| FR-13 | MUST | A day never fetched MUST read "not fetched yet", not 0h | Rust test |
| FR-14 | MUST | Every page MUST show one shared menu: Day · Week · Tasks · Logged · Settings · Billing, current section marked | component test |
| FR-15 | MUST | Day, Week and Tasks screens MUST show no Billing UI (view toggle, export, pins, invoice text, "Not invoiced") | component tests |
| FR-16 | MUST | `/billing` MUST keep working, including the billing export, reachable from the menu's Billing link | billing tests green + component test |
| FR-17 | SHOULD | Owner SHOULD be able to clear a dismissal | Rust route test |
| FR-18 | MAY | Logged MAY show a month total vs month required | component test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Stored-data render | Logged month page server response < 500 ms with 42 stored days × 10 entries | timed `curl` against `next start` |
| Fetch range cap | ≤ 42 days per Tempo fetch (a 6-week month grid) | Rust route test: 43 days → 400 |
| Reason length | 1–80 characters after trim | Rust route test |
| Security | Not applicable: single local user, daemon on 127.0.0.1 / unix socket; reasons never leave worklog | FR-12 test |
| Look | Matches the design chosen in the `/design:design` pass, light and dark, at 375 px and 1280 px | human check (CHK) |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test.
- [ ] The error paths of Journeys 1 and 3 are exercised.
- [ ] V-01: Open Logged for the current month: every day's hours match Tempo's website; an under-target day shows the "is this filled out?" flag; dismiss one with "dentist", reload, it stays dismissed; the Day page shows no Billing UI. — user, Q10
- [ ] `cargo test`, `cargo clippy -D warnings`, `bun test`, `bun run typecheck`, `bun run build` all green.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Real Tempo entries + required hours are already pulled into `tempo_remote_worklogs` / `tempo_required_days` (PREP A-02) | High | New fetch code needed |
| A2 | The Tempo fetch functions take any from/to; only the store step is week-bound | High | Larger Rust task |
| A3 | Logged reads and writes go through the daemon (the web has no live `bun:sqlite` use; CLAUDE.md's line is stale) | High | Different data layer |
| A4 | Logged counts all entries, worklog-sent and hand-logged, as the week close-out does | High | Totals disagree with Tempo |
| A5 | Only days strictly before today (daemon local date, `$WORKLOG_TZ`) are flagged | Med | Owner nagged mid-day |
| A6 | Billing components no longer mounted (ViewToggle, BillingGroup, day billing view) stay in the tree unused, per Not-this | Med | Dead code lingers |
| A7 | The Settings "billing cycle pruner" stays — it governs purge, not invoicing | Med | One Billing label left in Settings |
| A8 | "Pull from Tempo" on the week close-out stays; Logged adds a range fetch | High | Two fetch buttons |
| A9 | Visual design comes from one `/design:design` pass before the view components are built (PREP Discretion) | High | Views reworked |
| A11 | Tempo's schedule returns a row (0 s) for weekends and holidays, so a fetched past day never reads "not fetched yet"; T003 pins this with a test | Med | Weekends read "not fetched yet" |
| A10 | The billing ExportPanel moves onto `/billing` rather than disappearing | Med | Export unreachable |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Logged | Time that is really in Tempo, as Tempo reports it |
| Required / target | Tempo's required seconds for a day, from the Owner's user schedule |
| Under target | A past day with required > 0 and logged < required |
| Dismissal | The Owner's worklog-only note that a short day is fine ("dentist") |
| Hand-logged | A Tempo entry worklog did not send (`owner = outside`) |
