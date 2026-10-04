# Spec: Jira assistant

**Created**: 2026-10-04 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The Owner must have every piece of work on a GENAI ticket, but finding, starting, creating and moving tickets happens by hand in Jira or through a separate LibreChat agent whose ticket text reads badly and whose account memory lives in a fragile sandbox file.

**Solution**: From any Claude Code chat the Owner says "let's work on GENAI-9129" or "work on the Innnes SSO thing" and worklog fetches, starts or creates the ticket — with a learned billing-account suggestion — and later suggests moving it to Blocked or Done; My Tasks on the web shows the same Done suggestion.

**Who it's for**: The Owner (one consultant logging time to GENAI tickets).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Claude Code is the brain and worklog is the hands and the memory — the skill writes ticket text and asks the questions; worklog does every Jira call, enforces the gates (GENAI only, account from the live list, Done never automatic) and stores the clue log.

## 1. Context

### 1.1 Problem statement

The Owner bills all work through GENAI tickets with a Tempo account set (field `customfield_11530`). Today a new ticket goes through a LibreChat agent that pastes chat text into a rigid emoji template and keeps its account clues in a sandbox file that gets lost. Moving tickets to In Progress, Blocked or Done is remembered by hand and often forgotten.

**Current workaround**: the LibreChat agent for creation; the Jira web UI for everything else.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Works in Claude Code, logs time with worklog, owns the GENAI tickets | Wants one "yes" per decision, never a surprise in Jira |
| Claude (via the worklog skill) | Turns the Owner's words into worklog commands and writes ticket text | Must ask before anything other than the one automatic move |

**Primary actor**: Owner.
**Hidden stakeholders**: teammates who watch the GENAI board (they see every status move); invoicing (the account decides who pays).

## 2. Scope

### 2.1 In scope

- Read any ticket by key; start a GENAI ticket (auto ToDo/Backlog → In Progress).
- Find a ticket from free text; create a GENAI ticket when nothing matches.
- Suggest a billing account from a learned clue log; learn from every decision.
- Suggest Done (merged PR, or the Owner says it's finished) and Blocked (the Owner says they're waiting) — applied only on a yes.
- Show the Done suggestion on the web My Tasks card.
- Ticket text written properly in English, rendered as real headings and lists in Jira.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Never post to Slack (from the user's LibreChat prompt: "a post can't be taken back").
- Never delete a Jira ticket. — user, Q10
- Never assign a ticket to anyone other than the user unless the user names them. — user, Q10
- Never write to tickets outside the GENAI project (reading any ticket key is fine). — user, Q10
- No emojis in ticket text. No copy-pasting the user's chat message as the description. — user, Q8
- No automatic move to Done, ever. — user, Q3
- No automatic move triggered by logged time or blocks (D-05). No Blocked suggestion from idle time (D-11).
- No Notion fetching in v1 (the LibreChat skill's Notion step is not carried over; a pasted link is kept as a reference line).
- No change to the existing web transition menu or AI draft for non-GENAI tickets.

## 3. Journeys

### Journey 1 — Work on a known ticket (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | GENAI-9129 is in ToDo | Owner says "let's work on GENAI-9129" | Claude shows the ticket (title, status, description, comments, account) and Jira shows it In Progress |
| Error | Jira is unreachable or the key does not exist | same | Claude shows the Jira error in one line; nothing is moved |
| Edge | GOJ-1310 (not GENAI) is in ToDo | Owner says "let's work on GOJ-1310" | the ticket is shown; status is not touched; output says "not GENAI — status left alone" |

### Journey 2 — Work with no ticket yet (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | no open ticket matches "Innnes SSO" | Owner says "work on the Innnes SSO thing" | Claude shows one confirm: title + account (+ clues and past-ticket count); on "yes" a GENAI Story exists with the account set, in In Progress |
| Error | the Owner says "no, it's account X" | — | the ticket is created on X with no second confirm (D-12 rule 4); the decision log records the guess as wrong and which clue misled |
| Edge | one or more tickets match | same | Claude lists up to 3 matches + "or create new"; nothing is created until the Owner picks |

### Journey 3 — Status suggestions (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | GENAI-9129 is not Done and its PR merged | Owner opens a Claude Code chat, or opens My Tasks | "GENAI-9129: PR #12 merged — move to Done?" appears; on yes Jira shows Done |
| Error | the Done transition is not offered by Jira | Owner says yes | the Owner sees "Jira offers no Done transition from <status>"; status unchanged |
| Edge | Owner says "waiting for the customer on 9129" | — | Claude asks "Move GENAI-9129 to Blocked?"; nothing moves without yes |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to read any ticket by key from the CLI: key, title, status, type, account, description as text, last comments, and the link `{jira_base_url}/browse/<key>` | CLI test against a mocked Jira prints every field |
| FR-02 | MUST | Starting a GENAI ticket whose status category is To Do (ToDo and Backlog are both in it) MUST move it to the In Progress transition found by name, fetched fresh | mocked Jira receives exactly one transition POST with the In Progress id |
| FR-03 | MUST | Starting MUST NOT move a ticket that is outside GENAI or whose status category is not To Do (In Progress, Blocked, Done) | mocked Jira receives zero transition POSTs |
| FR-04 | MUST | No worklog code path MUST move a ticket to a Done-category status without an explicit Owner command naming that move | test: start, hints and the web load never POST a Done transition |
| FR-05 | MUST | Owner MUST be able to move a GENAI ticket to a named status (Blocked, Done, ToDo, In Progress) with one command | mocked Jira receives the transition whose target name matches, case-insensitive; a non-GENAI key is refused with zero Jira writes |
| FR-06 | MUST | Owner MUST be able to search tickets by free text and get up to 5 matches, open assigned ones first | CLI test |
| FR-07 | MUST | Owner MUST be able to create a GENAI Story with title, description and account; it then moves to In Progress | mocked Jira sees create with `customfield_11530` as a bare number, then one In Progress transition |
| FR-08 | MUST | Creation MUST refuse an account id that is not in the fresh allowed-account list for GENAI Story | error names the id; Jira create never called |
| FR-09 | MUST | Creation MUST refuse a project other than GENAI | error; Jira never called |
| FR-10 | MUST | The description MUST reach Jira as structured rich text: headings, bullet lists, checkbox lists and paragraphs keep their shape | converter unit test on a sample with all four |
| FR-11 | MUST | Creation MUST refuse a title or description containing emoji | unit test |
| FR-12 | MUST | Owner MUST get up to 3 account suggestions (an empty clue log runs a relearn first) for a piece of text, best first, each with its matched clues and past-ticket count, drawn only from the fresh allowed list | unit test on a seeded clue log |
| FR-13 | MUST | A relearn MUST read up to 200 GENAI tickets with an Account set and rebuild clues and ticket counts per account | mocked Jira search; clue table rebuilt |
| FR-14 | MUST | Every creation MUST log date, title, picked account, guessed account, right/wrong and the deciding clues | DB row asserted |
| FR-15 | MUST | A clue that was wrong 2 times for an account MUST stop counting for that account | unit test: 2 wrong decisions → clue gone from suggestions |
| FR-16 | MUST | Owner MUST be able to list Done suggestions: GENAI tickets not in the Done category with a merged PR whose title or body names the key | unit test on seeded events |
| FR-17 | MUST | The GitHub collector MUST record whether and when a PR was merged | collector test with a merged PR fixture |
| FR-18 | MUST | My Tasks MUST show a "Move to Done?" chip on a card with a Done suggestion; one click plus one confirm moves it | web test |
| FR-19 | MUST | The worklog skill MUST route "work on <KEY>", "work on <text>", "I'm waiting on…", "that's done", "what can I close?" (→ hints) and "create a ticket" to the commands above, and say what it will do before any non-automatic move or creation | skill test greps each routing phrase; live Verify |
| FR-20 | MUST | The skill MUST tell Claude how to write ticket text: English, no emoji, no pasted chat, sections only when they have content | skill test greps "English" and "no emoji" |
| FR-21 | MUST | A new Claude Code session MUST print pending Done suggestions at start | session-hint test |
| FR-22 | MAY | The web MAY show the clue log read-only | — |
| FR-23 | MUST | When the Owner names the account (up front or as a correction), creation MUST use it without another account question and still log the decision | unit test: decision row with correct = 0 and the misleading clues |
| FR-24 | MUST | If the In Progress move fails after a successful create, the Owner MUST see the new key plus the error, and create MUST NOT be retried | mocked Jira: create 201, transition 500 → output has key, exit 1, one create call |
| FR-25 | MUST | If the allowed-account list is empty or unavailable, creation MUST stop with a one-line error and never create | mocked createmeta 500 and empty → zero create calls |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Account suggestion latency (local, after allowed list fetched) | < 200 ms for 50 accounts × 30 clues | unit bench in test, `Instant` |
| Relearn size | ≤ 200 tickets, 1 paged search | mocked Jira call count |
| Suggestions shown | ≤ 3 accounts, ≤ 5 search matches | tests |
| Clue drop threshold | 2 wrong decisions | FR-15 test |
| Session-start hint cost | 0 network calls (DB only) | test asserts no client is built |
| Jira call timeout | 30 s total, 10 s connect (existing `http::client`) | on timeout the command exits 1 with one line and sends no further write |
| Secret exposure | 0 occurrences of the Jira token in CLI output, logs or DB rows | test greps output + DB dump for the token |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test or a CHK.
- [ ] Error path of each journey exercised (mocked Jira 4xx/5xx).
- [ ] Fresh Claude Code chat: "let's work on GENAI-9129" → Claude runs `worklog ticket get GENAI-9129` (via start), shows the ticket, and it moves to In Progress in Jira. Then "work on the Innnes SSO thing" (no ticket) → one yes/no with title + account → a new, well-formatted GENAI ticket appears in Jira (In Progress, account set). — user, Q14
- [ ] The Owner picked the ticket text format from 2 rendered samples.
- [ ] `cargo test`, `cargo clippy -D warnings`, `cd web && bun test && bun run typecheck` all green.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Jira read/write calls already exist in core: search, create (with account field), transitions, transition, status, detail, comment — `collectors/jira.rs:213,386,492,516,619,760` | High | more Jira plumbing to write |
| A2 | Daemon already exposes them over HTTP — `daemon.rs:147-217` | High | new routes instead of reuse |
| A3 | The spec 012 AI draft (web) stays as is — `task_draft.rs:1` | High | none |
| A4 | No `worklog ticket` CLI exists — `worklog-cli/src/cli.rs:86-538` | High | none |
| A5 | The bundled skill is installed by `worklog skill install` from `skills/worklog/` — `skill.rs:1` | High | skill not picked up |
| A6 | `jira_account_field_id` secret holds `customfield_11530` — `daemon.rs:1055` | High | account not written |
| A7 | Descriptions today go to Jira as one flat paragraph — `jira.rs:353` | High | FR-10 is a no-op |
| A8 | Jira's create-metadata for GENAI Story lists the Account field's allowed values (the LibreChat connector got them) | Medium | FR-08/FR-12 fall back to the Tempo account list (`GET /accounts`) |
| A9 | GitHub issue search returns `pull_request.merged_at`; a second query `merged:<window>` catches PRs created earlier | Medium | Done hints miss PRs opened before the collect window |
| A10 | "Never write outside GENAI" binds the new commands and the new web chip only; the existing My Tasks transition menu keeps working for all projects | Medium | existing GOJ workflows break |
| A11 | Clues are extracted without a model: the summary's customer prefix (text before " - ") plus frequent non-stopword words; Claude refines by adding clues | Medium | weaker first suggestions |
| A12 | New tickets are type Story (the LibreChat skill's type), assigned to nobody unless named | High | wrong issue type |

## 8. Open questions

1. [NEEDS CLARIFICATION: ticket text look — the Owner picks from 2 rendered samples during CHK001; the skill's format guide follows that pick]

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Account | Tempo billing account, Jira field `customfield_11530`, sent as a bare number |
| Clue | A word or phrase that points a piece of text to an account |
| Start | Fetch a ticket and, if GENAI and To Do, move it to In Progress |
| Done suggestion | A prompt to move a ticket to Done; never applied without a yes |
| GENAI | The only Jira project this feature writes to |
