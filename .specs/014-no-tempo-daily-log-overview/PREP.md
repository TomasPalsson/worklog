# Prep — See what's logged in Tempo, inside worklog
Gathered: 2026-10-04 · Questions: 10 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 Tempo stays the system of record — the user must keep logging there. The goal is to never open Tempo's website: worklog shows the logged time per day in a good-looking overview. ("I HAVE to use tempo I just don't want to use their website") — user, Q1
- D-02 The overview has three zoom levels: month, week and day. ("All of the above I just want an overview for month week and day") — user, Q2
- D-03 A day logged under its target (~8h) is flagged "is this filled out?"; the user can dismiss the flag with a short reason (e.g. "dentist"). — user, Q2
- D-04 "Under target" means below Tempo's required hours for that day (its schedule — holidays/half days need nothing), not a fixed 8h. — user, Q3
- D-05 Data arrives both ways: opening a month/week/day view auto-fetches that range from Tempo, plus a manual refresh button for the shown range. — user, Q4
- D-06 The overview is its own new section (own nav entry, month/week/day inside it), separate from worklog's existing day and week pages. — user, Q6
- D-07 The revamp is folded into this spec: all ticket/Jira/Tempo UI gathered into one place, fully separated from Billing. ("fold it in"; offered a separate prep, declined) — user, Q7
- D-08 Billing is tucked away, not deleted: removed from every Jira/Tempo screen (day toggle, export panel, task-modal "Not invoiced", etc.); it lives only on `/billing` behind one small link. Code and data stay (A-09). — user, Q8
- D-09 One shared top menu on every page: Day · Week · Tasks · Logged, then Settings and a small Billing link. Day/Week = worklog's estimates + Send to Tempo; Tasks = Jira board; Logged = what is really in Tempo (D-02). — user, Q9
## Not this
- Removing, hiding or disabling any Tempo sync code. — user, Q1
- Creating, editing or deleting Tempo entries from the overview — it is read-only. — user, Q5
- Deleting Billing code, tables or the `exported_at` purge rule. — user, Q8
- Sending dismissal reasons ("dentist") to Tempo — they stay in worklog only. — user, Q5
## Discretion
- Visual design of the overview — user asked for `/design:design` to own it (ungrillable here).
## Assumptions
- A-01 Tempo is woven through the UI: "Sync to Tempo" button, per-day ticket lines, week close-out "In Tempo" column + "Pull from Tempo" — evidence: web/components/ActionBar.tsx:141, web/app/[day]/page.tsx:91, web/components/WeekCloseout.tsx:94 — confidence: high — confirmed Q1
- A-02 Real Tempo entries are already read back into `tempo_remote_worklogs` (day, ticket, seconds, description, owner = worklog vs hand-logged) plus the required-hours schedule — evidence: rust/crates/worklog-core/src/tempo_hub_contract.rs:171, rust/crates/worklog-core/src/daemon_week.rs:43 — confidence: high — unconfirmed
- A-03 That read-back is one week at a time, behind a manual "Pull from Tempo" button, and today only feeds a small totals table — evidence: web/components/WeekCloseout.tsx:94, rust/crates/worklog-core/src/week_closeout.rs:55 — confidence: high — unconfirmed
- A-05 Existing pages: `/[day]`, `/week/[monday]`, `/billing`, `/tasks` — evidence: web/app/ — confidence: high — unconfirmed
- A-04 A week view already exists (7 day columns + close-out table) — evidence: web/app/week/[monday]/page.tsx:36 — confidence: high — unconfirmed
- A-06 No global nav: each page owns its header; DayHeader holds ViewToggle (Tickets|Billing), ExportPanel, /tasks + /billing links, Settings; WeekHeader has /tasks only — evidence: web/components/DayHeader.tsx:55, web/components/WeekHeader.tsx:42 — confidence: high — unconfirmed
- A-07 Jira/Tempo UI is spread over: day page ActionBar (Refresh Jira, dry-run, Send to Tempo) + TicketGroup lines; week WeekCloseout (Pull from Tempo); /tasks TaskBoard + TaskModal (work log, per-day sync, assistant); Settings modal credentials — evidence: web/components/ActionBar.tsx:97, web/components/WeekCloseout.tsx:94, web/app/tasks/page.tsx:42, web/components/CredentialFields.tsx:15 — confidence: high — unconfirmed
- A-08 Billing is mixed into Jira/Tempo UI: day page view toggle + BillingGroup, ExportPanel in day header, TaskHours "Not invoiced / Go to billing" in the task modal, BlockCard billing toasts/tooltips, global ChangeNotices billing notices — evidence: web/app/[day]/page.tsx:141, web/components/TaskHours.tsx:46, web/components/BlockCard.tsx:96, web/components/ChangeNotices.tsx:101 — confidence: high — unconfirmed
- A-09 Billing data is load-bearing for purge (`exported_at` proves a block was billed) — evidence: CLAUDE.md conventions (purge.rs) — confidence: high — unconfirmed
## Verify
- V-01 Open Logged for the current month: every day's hours match Tempo's website; an under-target day shows the "is this filled out?" flag; dismiss one with "dentist", reload, it stays dismissed; the Day page shows no Billing UI. — user, Q10
## Open
