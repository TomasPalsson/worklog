# Prep — Tempo ticket auto-pick
Gathered: 2026-10-01 · Questions: 7 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 Two steps. Step 1: a block takes the Jira key its own events already carry (`events.jira_issue`, from prompt/cwd), no AI. Step 2: only blocks still without a key get an AI pick from the user's open assigned tickets. No SessionStart ask-the-model hook. — user, Q1
- D-02 Picks (step 1 key-copy AND step 2 AI pick) are written to the block automatically — no accept click. The user corrects wrong ones by changing the ticket. A ticket the user set or changed by hand is never overwritten by a later pick. (Replaces the Q2 answer "everything waits".) — user, Q6
- D-03 Each Tempo ticket line (day × jira_issue) gets the billing line treatment: a stored description generated ahead of time that the user can read, edit or regenerate, and hours = union of block intervals rounded to half hour. Sync sends exactly the stored text and hours shown. — user, Q3
- D-04 Time is adjusted on the ticket line: the user types an hours override on the line and that is what Tempo receives; the underlying blocks are not changed. Existing per-block minute edits stay as they are. — user, Q4
## Not this
- No SessionStart hook that asks the model or the user which ticket a session is (rejected in Q1: more effort, not less).
- No automatic Tempo sync; the user still presses "Sync to Tempo". — user, Q5
- Billing export is unchanged; nothing is removed from it. — user, Q5
- The AI never creates new Jira tickets; it only picks from existing open assigned ones. — user, Q5
## Discretion
- How an auto-picked ticket is marked in the UI (e.g. a small "auto" tag) and how "set by hand" is stored (e.g. a ticket-origin column).
- When step 2 runs (e.g. alongside the existing estimate/line-text run).
- What happens to a line-hours override when its blocks later change.
## Assumptions
- A-01 Tempo sync already works and skips any block with no `jira_issue` ("no jira_issue — assign one in the UI") — evidence: rust/crates/worklog-core/src/collectors/tempo.rs:176 — confidence: high — confirmed Q1
- A-02 A block's ticket is set by hand only (UI → POST /blocks/:id/ticket → block_service::assign_ticket); nothing suggests one today — evidence: rust/crates/worklog-core/src/block_service.rs:36, daemon.rs:1207 — confidence: high — confirmed Q1
- A-03 The Claude Code hook already pulls a Jira key from the prompt or cwd onto each event (`events.jira_issue`), but it is never copied up to the block — evidence: rust/crates/worklog-core/src/hook_run.rs:136 — confidence: high — confirmed Q1
- A-04 The "validator" (verdict classifier) only picks a folder for an event, via an optional local helper on :9324; it knows nothing about tickets — evidence: rust/crates/worklog-core/src/verdict.rs:29 — confidence: high — confirmed Q1
- A-05 The user's open assigned tickets are already fetched and cached (`assignee = currentUser() AND statusCategory != Done`, jira_tickets table) — evidence: rust/crates/worklog-core/src/collectors/jira.rs:22, :117 — confidence: high — confirmed Q1
- A-06 hook-run must never print to stdout, so a SessionStart "suggest a ticket to the model" hook would need a separate command — evidence: CLAUDE.md conventions, hook_run.rs:2 — confidence: high — confirmed Q1
- A-07 Billing lines get a stored, editable, regenerable description (line_text::generate_for_day, POST /billing/lines/text, /billing/lines/regenerate); Tempo instead builds one at sync time via summarize_descriptions and never stores it — evidence: rust/crates/worklog-core/src/daemon_line_text.rs:28,:65; collectors/tempo.rs:444,:657 — confidence: high — confirmed Q3
- A-08 Billing hours = union of block intervals rounded to half hour; Tempo sums durations per (day, jira_issue) then rounds, so overlaps double-count — evidence: collectors/tempo.rs:48,:410 — confidence: high — confirmed Q3
- A-09 Per-block minutes are already editable in both views (POST /blocks/:id/duration); start/end are read-only — evidence: daemon.rs:1193, block_service.rs:59, web BlockCard.tsx:216 — confidence: high — confirmed Q3
- A-10 Tickets and billing are the same `/[day]` page toggled by cookie; ticket groups render in TicketGroup.tsx — evidence: web/lib/view-mode.ts, web/components/TicketGroup.tsx:94 — confidence: high — confirmed Q3
## Verify
- Open a day in the Tickets view: every block has a ticket (auto-picked ones tagged), each ticket line shows a stored description and union-rounded hours; override one line to 2h, press "Sync to Tempo", and Tempo shows exactly that text and 2h; `cargo test --manifest-path rust/Cargo.toml` is green. — user, Q7
## Open
