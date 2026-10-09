# Prep — Ticket estimate vs hours used, per person
Gathered: 2026-10-09 · Questions: 8 of 12 · Route: dispatch · Status: ready for spec
Seed: "pull the estimated times from tempo because we use that as times, say RL allowed 4 hours for GENAI-1897 and I've used 2hrs and Jón has used 2.5 I would like to be able to see that in the Ui and be able to keep track of that" — user, prep args

## Decisions
- D-01 The allowed hours = the "Estimated" field in Tempo's Time Tracking panel (Jira original estimate); the team will start filling it in. — user, Q1
- D-02 Show per-person hours like Tempo's "Collaborators" list (e.g. Tómas 2h, Jón 2h 30m) next to the estimate. — user, Q1
- D-03 Shown on each block on the day page (BlockCard), detailed: used / estimate + per-person split — not on ticket views ("I want it on the actual blocks"). — user, Q3
- D-04 Each block shows a progress bar of its ticket's hours vs estimate — always visible, not hidden behind a click; final look via /design:design + /design:graphics. — user, Q4
- D-05 mock.html v3 is the approved look ("looks good"). — user, Q5
- D-06 The bar counts this block's hours not yet in Tempo as a striped "+this block" piece, and the total reads "once synced". — user, Q6
## Not this
- No editing the estimate from worklog — it is set in Jira. — user, Q7
- No Slack pings, pop-ups or notifications when a ticket goes over. — user, Q7
- Never block or warn on "Sync to Tempo" because of the estimate. — user, Q7
- Ticket views (Tasks board card, ticket popup) are not the target; leave them as they are. — user, Q3
- No phone/mobile layout for this — desktop only. — user, mid-mock
## Discretion
- Visual design follows mock.html v3 (same folder; rebuild with `python3 .vary/build/gen.py`): under each block's ticket chip, an always-visible stacked bar (one colour per person, flag at the estimate, amber zone past it, striped piece for this block's not-yet-synced hours) + initials legend + a small "hours this week, by day" burn-up chart with hover/tap tooltip and table view. Person colours validated for colour-blindness. Made with /design:design + /design:graphics + dataviz at user request, Q2–Q4.
- Design-review leftovers (2 evaluator rounds, 3.4 → 3.5/5; title + tooltip fixed after): several blocks on the same ticket repeat the same chart — spec picks (show once per ticket per day, or keep); initials on wide bar segments + hover on segments; make "over" read stronger than "running low"; raise 10px labels to 11–12px; check teal initials contrast; close the dead gap under the legend.
## Assumptions
- A-05 Block card's ticket row (chip + auto/synced pills) is where the pill goes — evidence: web/components/BlockCard.tsx:255 — confidence: high — unconfirmed
- A-01 Jira original/remaining estimate + total time spent (everyone) are already fetched per ticket — evidence: rust/crates/worklog-core/src/collectors/jira.rs:808 — confidence: high — confirmed Q1
- A-02 The task modal already shows an Estimate meter: "spent of estimate", "X left" / "X over" — total for everyone, no per-person split — evidence: web/components/TaskModalTime.tsx:101 — confidence: high — unconfirmed
- A-03 Tempo read-back today pulls only the user's own worklogs (/worklogs/user/{id}), not teammates' — evidence: rust/crates/worklog-core/src/collectors/tempo.rs:1263 — confidence: high — unconfirmed
- A-04 The modal shows "In Tempo (you)" separately — evidence: web/components/TaskModalTime.tsx:96 — confidence: high — unconfirmed
## Verify
- Open the day page on a GENAI-1897 block not yet in Tempo: it shows "5h 30m of 4h once synced · 1h 30m over", a bar with You 2h, Jón Geir 2h 30m and a striped +1h, and a running-total chart — and the numbers match Tempo's Time Tracking panel for that ticket (Logged + Collaborators). — user, Q8
## Open
