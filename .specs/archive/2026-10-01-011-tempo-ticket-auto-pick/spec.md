# Spec: Tempo ticket auto-pick

**Created**: 2026-10-01 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The team is moving back to logging everything through Tempo. The Owner still checks every block's ticket by hand. They also can't trust the scheduled AI ticket pick, because it can replace a ticket they set themselves. The ticket view doesn't show what will be sent: the description is made at sync time, and hours are summed, so overlapping blocks count twice.

**Solution**: Tickets land on blocks by themselves and are tagged "auto". A hand-set ticket is never replaced. Each ticket line gets a stored description the Owner can edit or regenerate, fair (union) hours, and an hours override. Sync sends exactly what the line shows.

**Who it's for**: The Owner, who logs their own work to Tempo through worklog.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Picks are written straight to the block with no accept step (D-02). The safety net is the "auto" tag plus the hand-set lock, not a review queue.

## 1. Context

### 1.1 Problem statement

Ticket picking already runs on its own. Step 1 copies the one Jira key a block's events share (`infer.rs:176`). Step 2 has the estimator pick from cached open tickets (`estimate.rs:662`), and `worklog day` runs it every 15 min. But nothing records *who* set a ticket. So a re-estimate can replace a hand-set ticket, and the UI can't show which tickets were guessed. The Tempo line description exists only inside sync (`tempo.rs:444`). Hours are a plain sum (`tempo.rs:410`), unlike billing's union (`billing.rs:520`).

**Current workaround**: The Owner assigns tickets by hand in TicketCombobox, edits block descriptions one by one, and runs a dry-run sync to guess what Tempo will get.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews a day's ticket lines, fixes wrong tickets, presses Sync | Wants the fewest clicks possible |
| Scheduler | `worklog day` every 15 min (infer + estimate) | Unattended; must never undo the Owner's edits |

**Primary actor**: Owner.
**Hidden stakeholders**: The team lead reading Tempo worklogs, who sees the descriptions and hours.

## 2. Scope

### 2.1 In scope

- Record each block ticket's origin: event, auto or manual.
- Never let infer or estimate replace a manual ticket.
- Show an "auto" tag on blocks whose ticket was not set by hand.
- Stored, editable, regenerable description per ticket line (day × ticket).
- Ticket-line hours = union of block intervals rounded to half hour, with a per-line hours override.
- Tempo sync sends the line's stored description and hours.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No SessionStart hook that asks the model or the user which ticket a session is (rejected in Q1: more effort, not less). — user, Q1
- No automatic Tempo sync; the user still presses "Sync to Tempo". — user, Q5
- Billing export is unchanged; nothing is removed from it. — user, Q5
- The AI never creates new Jira tickets; it only picks from existing open assigned ones. — user, Q5
- No accept/reject queue for picks (D-02).
- No start/end time editing on blocks; per-block minutes editing stays as it is (D-04).

## 3. Journeys

### Journey 1 — Day review (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Scheduler has run; blocks carry event/auto tickets | Owner opens `/[day]?view=tickets` | Every ticketed block shows its ticket, and non-manual ones show "auto". Each ticket line shows a stored description and union hours. |
| Error | The AI's pick is wrong | Owner picks another ticket in TicketCombobox | Block shows the new ticket with no "auto" tag. The next scheduler run leaves it unchanged. |
| Edge | Owner clears a ticket by hand | Scheduler runs again | Block stays without a ticket (a manual clear is also locked). |

### Journey 2 — Adjust and sync (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A ticket line shows 2.5h | Owner types 2h on the line, edits the text, presses Sync | Tempo worklog has exactly that text and 7200 s. The line keeps showing 2h. |
| Error | Line was already synced | Owner edits text or hours | That line's synced blocks become dirty. The next Sync updates the existing worklog (PUT), not a new one. |
| Edge | Owner clears the override (empty field) | Line re-renders | Hours fall back to the union-rounded value. |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST have every hand assignment or clear of a block ticket recorded as manual | unit test: assign_ticket sets origin manual for Some and None |
| FR-02 | MUST | Scheduler MUST record a ticket copied from events as event-origin | infer test: fresh block with one shared key has origin event |
| FR-03 | MUST | Scheduler MUST record an estimator-picked ticket as auto-origin | estimate test with FixedInvoker: origin auto |
| FR-04 | MUST | Scheduler MUST NOT change a manual-origin ticket during rebuild or estimate | infer + estimate tests: manual ticket (and manual NULL) survive |
| FR-05 | MUST | Owner MUST see an "auto" tag on every ticketed block whose origin is not manual (event, auto, or NULL pre-upgrade) | BlockCard test |
| FR-06 | MUST | Owner MUST see a stored description on each ticket line | TicketGroup test + daemon route test |
| FR-07 | MUST | Owner MUST be able to edit a ticket line's description | route test + TicketGroup test |
| FR-08 | MUST | Owner MUST be able to regenerate a ticket line's description | route test |
| FR-09 | MUST | Owner MUST see ticket-line hours as the union of block intervals rounded to half hour | unit test with two overlapping blocks |
| FR-10 | MUST | Owner MUST be able to set and clear an hours override per ticket line | route test |
| FR-11 | MUST | Sync MUST send the line's stored description and effective hours | tempo.rs test with httpmock body match |
| FR-12 | MUST | Editing a synced line's text or hours MUST mark its synced blocks dirty | unit test |
| FR-13 | MUST | Scheduler MUST generate missing or stale line descriptions in the same run that estimates blocks (`worklog day` and the Estimate button) | run_estimate + cli test |
| FR-14 | MUST | Owner MUST see the no-AI fallback summary on a line that has no stored description yet | TicketGroup test |
| FR-15 | MUST | Sync MUST, for a line with no stored description, generate it, store it, then send that stored text | tempo.rs test |
| FR-16 | MUST | Owner MUST get an inline error, and nothing saved, for an hours value that is not a positive multiple of 0.5 h | TicketGroup test + route 400 test |
| FR-17 | MUST | Regenerate MUST replace the line's text, including an Owner-edited one (the Owner asked for it) | route test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Owner effort for a correct day | 1 click (Sync) | Journey 1 happy path needs no clicks before Sync |
| Description length sent to Tempo | ≤ 250 chars | existing `cap_description` applied to stored text |
| Hours granularity | 0.5 h | `round_to_half_hour`; override stored as seconds, multiple of 1800 |

## 6. Launch criteria

- [ ] Open a day in the Tickets view: every block has a ticket (auto-picked ones tagged), each ticket line shows a stored description and union-rounded hours; override one line to 2h, press "Sync to Tempo", and Tempo shows exactly that text and 2h; `cargo test --manifest-path rust/Cargo.toml` is green. — user, Q7
- [ ] Every MUST in §4.1 has a passing test.
- [ ] `cd web && bun test && bun run typecheck && bun run build` green.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Step 1 and step 2 of D-01 already exist (`infer.rs:176`, `estimate.rs:662`), so this spec adds origin tracking rather than new pickers | High | Picks would need building from scratch |
| A2 | Existing tickets on upgrade get origin NULL, treated like auto (overwritable, tagged) | Med | Old hand-set tickets could be replaced once; mitigated because the estimator skips already-estimated blocks |
| A3 | The generated line description reuses `summarize_descriptions` (same output sync makes today), stored instead of thrown away | High | A new prompt would be needed |
| A4 | MixedLegacy groups (differing old worklog ids) keep their per-block sync path and ignore stored line values | Med | Legacy days sync old-style text; rare |
| A5 | Tempo is reached through the existing `tempo.rs` client; no new auth | High | — |
| A6 | An hours override stays when the line's blocks change later; the Owner clears it to fall back | Med | Override hides new work until cleared |
| A7 | A failed generation leaves the line on its fallback summary; a failed Tempo call leaves blocks dirty and reports the error (today's behaviour) | High | — |
| A8 | A manual write and a scheduler write racing: the manual one wins (scheduler SQL guards on `ticket_origin`) | High | Hand-set ticket lost |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Ticket line | All non-personal blocks of one day sharing one `jira_issue`; one Tempo worklog |
| Ticket origin | Who set a block's ticket: `event` (key in its events), `auto` (estimator), `manual` (Owner), NULL (pre-upgrade) |
| Hours override | Owner-typed hours for a ticket line that replace the union-rounded value |
