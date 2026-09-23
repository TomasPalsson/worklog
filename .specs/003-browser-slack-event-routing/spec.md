# Spec: Browser + Slack events routed to the right block

**Created**: 2026-09-23 · **Route**: dispatch · **Prep**: PREP.md (D-01…D-16, user-decided)

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Worklog only sees Claude Code prompts, commits and calendar meetings. Time with no code trail, above all cert study on sites like https://aws.tomasari.is/, never becomes a block. The user reconstructs it from memory when filling the invoice form.

**Solution**: Worklog records browser time (active tab, work hours only) and sent Slack messages, labels each with a project, and its normal block-building turns them into blocks. A fix the user makes teaches it; a local model guesses the rest.

**Who it's for**: The consultant (single user) who fills the invoicing form from worklog every week.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: The router labels events with a **project**, never a block id; blocks stay a product of the existing inference (D-01).

## 1. Context

### 1.1 Problem statement

Invoice logs (2026-06-22 → 2026-09-22) show many hours per week of "Námskeið" cert study and one customer split across several Verkefni on one day. Worklog cannot see the study time at all, and when browser or chat signals exist it has no way to tell which of a customer's projects they belong to.

**Current workaround**: Recalling study and chat time from memory, then typing lines into the form by hand.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Consultant | Works in Firefox, Slack and Claude Code; reviews blocks; copies lines into the invoice form | Checks every line while copying (D-12) |

**Primary actor**: Consultant.
**Hidden stakeholders**: None identified (single-user local tool).

## 2. Scope

### 2.1 In scope

- A Firefox add-on that records active-tab time inside work hours (D-03, D-04, D-05, D-06, D-08, D-13)
- Collecting the consultant's own sent Slack messages, at any hour (D-09, D-14, D-15)
- Labelling each browser/Slack event with a project: hard rule → local model (≥90%) → unsorted (D-10, D-11)
- Named projects with no folder, pinned like folders (D-02)
- Fixing a label in the review UI, optionally as an "always" rule (D-10)
- Showing every collected event, its source and its label origin in the UI (D-16)

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Removing Jira from worklog (separate cleanup).
- Fixing the export's Teg. vinnu / Hvað / Akstur fields (Núll verð, Námskeið, Veikindi).
- Fine-tuning Laya.
- Browsers other than Firefox.
- Reading Slack messages the user received.
- Importing Firefox's existing history.

## 3. Journeys

### Journey 1 — Study time becomes a block (Consultant)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A rule "aws.tomasari.is → AWS cert" exists | The consultant studies on the site for 60 min, Tuesday 10:00–11:00 | That day shows a block of 60 ± 5 min labelled AWS cert, events tagged "rule" |
| Error | The local model is not installed and no rule matches | The consultant browses a new site for 20 min | The events appear in the day's Unsorted list; no block is invented; nothing else fails |
| Edge | It is 18:30 or the add-on is paused, or the tab is private or in the Personal container | The consultant browses | Nothing is stored |

### Journey 2 — A Slack message joins the right project (Consultant)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Slack is connected; the model scores "sjukra" ≥ 90% | The consultant sends a message in a Sjúkra channel at 14:10 | After the next collect, the message sits in the sjukra block, tagged "guess" with its score |
| Error | The Slack token is missing or revoked | A collect runs | The collect reports the Slack error; other collectors still run |
| Edge | The message is sent at 22:00 | A collect runs | It is collected (Slack ignores work hours, D-15) |

### Journey 3 — A fix teaches the router (Consultant)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | An unsorted github.com event | The consultant picks "lighthouse" without ticking "always" | That event is labelled lighthouse ("fix"); later github.com events are not forced to lighthouse |
| Error | The chosen project no longer exists | The consultant saves | The picker stays open with the inline text "Project no longer exists"; the event's label is unchanged |
| Edge | The consultant ticks "always for this site" | Saves | Every current and future unsorted/guessed event on that domain gets the project ("rule"); earlier "fix" labels stay |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The add-on MUST send a heartbeat about once a minute while a normal-window tab is focused and the consultant is not idle | Unit test on the heartbeat decision; manual run shows ~60 events per hour |
| FR-02 | MUST | Each heartbeat MUST carry full address, page title and container name | Stored event shows all three |
| FR-03 | MUST | Nothing MUST be stored from private windows or the "Personal" container | Test: such heartbeats are rejected or never sent |
| FR-04 | MUST | Heartbeats outside the work-hours window (default Mon–Fri 09:00–17:00, editable) MUST NOT be stored | Test at 08:59 and 17:01 |
| FR-05 | MUST | The add-on MUST have a pause toggle that stops heartbeats until turned back on | Manual check |
| FR-06 | MUST | Worklog MUST accept heartbeats only when the browser-stamped request origin is an add-on origin (a web page cannot set this); any other origin is refused and nothing is stored | Test: a request whose origin is a web page, or missing, is refused with "forbidden" and stores nothing |
| FR-07 | MUST | Collect MUST fetch the consultant's own sent Slack messages with channel name and text, idempotently | Re-collect creates no duplicates |
| FR-08 | MUST | Each browser/Slack event MUST be labelled by the first match of: hard rule, model guess ≥ threshold (default 90%, editable), else unsorted | Tests per branch |
| FR-09 | MUST | Unsorted events MUST NOT enter block inference until labelled | Test: unsorted event absent from blocks |
| FR-10 | MUST | Labelled browser/Slack events MUST reach the project's block through the existing inference | End-to-end test |
| FR-11 | MUST | The consultant MUST be able to create a named project with no folder and pin it to a customer and Verkefni | Existing Billing panel row with a folder-less name routes and bills |
| FR-12 | MUST | The consultant MUST be able to relabel any browser/Slack event; the event is stored as a labelled example | Test |
| FR-13 | MUST | Relabelling with "always" MUST create a hard rule keyed on the domain, channel or container | Test |
| FR-14 | MUST | The review UI MUST list each day's unsorted events | UI test |
| FR-15 | MUST | Every browser/Slack event MUST show its source (Firefox + container, or Slack + channel) and label origin (rule, fix, guess + score) | UI test |
| FR-16 | MUST | Worklog MUST keep working when the local model is absent: every unmatched event goes unsorted | Test with the model unreachable |
| FR-17 | MUST | A container that names exactly one customer MUST restrict the model's choices to that customer's projects | Test |
| FR-18 | SHOULD | The consultant SHOULD see and delete hard rules | UI test |
| FR-19 | SHOULD | Settings SHOULD show the last heartbeat time, last Slack collect and whether the model helper is reachable | UI test |
| FR-20 | MAY | The model MAY receive up to 5 recent fixes with the same domain, channel or container as hints | Test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Heartbeat cadence | 1 per 60 s ± 5 s while active | Add-on unit test with fake timers |
| Idle cut-off | No heartbeat after 120 s without input | Add-on unit test |
| Labelling a full day | ≤ 60 s for 600 events on the consultant's Mac with the model loaded | Timed `worklog collect all` |
| Heartbeat ingest | ≤ 50 ms p95 at the daemon | Handler test timing |
| Data locality | 0 bytes of browser/Slack content sent off the machine | Model helper binds loopback only; review |

## 6. Launch criteria

- [ ] Study on https://aws.tomasari.is/ for 1 h in a normal tab → that day's review UI shows a ~1 h block under "AWS cert"
- [ ] A Slack message in a customer channel lands in that customer's block
- [ ] Every browser/Slack event and its source is visible in the UI
- [ ] `cargo test --manifest-path rust/Cargo.toml` and `cd web && bun test` are green; clippy and fmt clean
- [ ] Each journey's error path is exercised by a test

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Setting an event's project path to the work root + project name makes inference, personal classification and billing treat it like a folder, with no change to them (PREP A-02, A-05) | High | Billing/personal need edits |
| A2 | Blocks are rebuilt on re-inference, so project labels survive and block ids do not (PREP A-03) | High | — |
| A3 | Per-minute heartbeat events fit the 30-min gap clustering without changing inference | High | Study blocks fragment |
| A4 | Slack's message search can return the user's own messages by date with a user token the consultant creates (D-14) | Medium | Slack collector needs another endpoint |
| A5 | Laya runs on the consultant's Mac CPU fast enough for §5 (card: 193–464 ms per call on CPU) | Medium | Labelling slower; batch per day |
| A6 | A fix is stored on the event itself; purge rules may later remove old examples (PREP Discretion) | Medium | Fewer hints after purge |
| A7 | Account containers map to customers by name (ru-prod → Háskólinn í Reykjavík …) via existing customer aliases (PREP A-15) | Medium | FR-17 narrows nothing until aliases added |
| A9 | Single user, so two "always" rules racing on one domain resolve by the database's unique key (last write wins, no error) | High | Duplicate rules |
| A8 | Mozilla signs the add-on as unlisted with the consultant's own API keys (D-13) | High | Manual install per restart |

## 8. Open questions

None — every category was settled in PREP.md.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Project | A billable project key: a folder under `~/Desktop/Work` or a named project; carries a customer + Verkefni pin |
| Label | The project assigned to a browser/Slack event |
| Label origin | `rule` (hard rule), `fix` (consultant chose), `guess` (model ≥ threshold) |
| Unsorted | A browser/Slack event with no label; excluded from blocks |
| Heartbeat | One add-on report: this tab was focused and the consultant active in this minute |
| Hard rule | "always" mapping from a domain, Slack channel or container to a project |
