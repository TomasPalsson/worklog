# Prep — Tempo hub (tasks + updates + sync in one place)
Gathered: 2026-10-01 · Questions: 1 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 User handed design to Mr Claude: "just run this as a /flow:spec with your ideas, I just want something cool" — all three ideas (My Tasks page, week close-out, Tempo read-back) are candidates; spec picks scope. — user, Q1
## Not this
## Discretion
- Feature scope, layout, and AI touches — delegated by D-01.
## Assumptions
- A-01 Tickets already auto-pick onto blocks (event key, then AI from open assigned tickets); hand-set tickets are locked — evidence: .specs/archive/2026-10-01-011-tempo-ticket-auto-pick/spec.md:40 — confidence: high — unconfirmed
- A-02 Ticket lines (day × ticket) already have stored, editable, AI-regenerable text and union hours with override — evidence: rust/crates/worklog-core/src/tempo_lines.rs:166 — confidence: high — unconfirmed
- A-03 A local cache of the user's open assigned Jira tickets exists (`jira_tickets`) — evidence: rust/crates/worklog-core/src/estimate.rs:1115 — confidence: high — unconfirmed
- A-04 Jira writes today are create-ticket only; no status change, no comment — evidence: rust/crates/worklog-core/src/collectors/jira.rs:342 — confidence: medium — unconfirmed
- A-05 Tempo is write-only today (POST/PUT/DELETE worklogs); nothing reads back what is already in Tempo — evidence: rust/crates/worklog-core/src/collectors/tempo.rs:105 — confidence: medium — unconfirmed
- A-06 Sync runs one day at a time, by hand, behind a confirm click — evidence: web/components/ActionBar.tsx:65 — confidence: high — unconfirmed
- A-07 Billing ideas available to borrow: union hours (done), folder pins → default customer, keyword → deild matching — evidence: CLAUDE.md billing section — confidence: high — unconfirmed
## Verify
- On My Tasks, move one real ticket's status and post an AI-drafted comment (both show in Jira); on the week page, Pull from Tempo matches Tempo and a second Sync week reports 0 synced. — set by Mr Claude under D-01
## Open
