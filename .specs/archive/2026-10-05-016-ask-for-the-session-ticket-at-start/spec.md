# Spec: Ask for the session ticket at start

**Created**: 2026-10-05 · **Route**: dispatch · **Design**: none

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: In work folders a Claude Code session often starts and runs with no Jira ticket, so its time lands as "unassigned" and the Owner fixes it by hand at the end of the day. The current SessionStart hint only asks Claude to ask "once the task is clear", and Claude often does not.

**Solution**: At the start of every fresh work-folder session the Owner gets one pick-one question — the branch's ticket, a recent ticket of theirs, create a new one, or skip — and the answer is recorded for the session before any work happens.

**Who it's for**: the Owner (the one person whose time worklog tracks).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: the worklog Claude Code mod asks the question itself (a real dialog at session start), instead of hook text asking Claude to ask. The SessionStart hint stops asking, so the Owner is never asked twice.

## 1. Context

### 1.1 Problem statement

The Owner bills time per Jira ticket. A session with no recorded ticket leaves its blocks unassigned, or tagged by a branch-name guess nobody confirmed. Today's hint is advice to the model, not a question to the Owner, so whether the Owner is asked depends on the model.

**Current workaround**: the Owner assigns tickets to blocks in the review UI after the fact, or types `worklog ticket use` by hand.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Starts Claude Code sessions in work folders and bills the time | Wants one quick question, never a second one |
| Claude | Works in the session; creates a ticket when the Owner chose "create" | Must not create a ticket without one Owner confirm |

**Primary actor**: Owner.
**Hidden stakeholders**: the invoicing export, which reads block tickets.

## 2. Scope

### 2.1 In scope

- Ask the Owner once at the start of a fresh work-folder session, and once after `/clear`.
- Offer the branch's ticket, recent tickets from the Owner's task list, "Create a new ticket", "Skip", and free text under "Other".
- Record a chosen ticket for the session immediately.
- Hand a "create" choice to Claude with the session id, so it creates and records the ticket once the first request makes the task clear.
- Use the chosen ticket for the context Claude is given and for the ticket line on commits and PRs.
- The SessionStart hint stops asking; it only names a recorded or branch ticket.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No question outside work folders (`~/Desktop/Work/` or a work-org git origin, the mod's existing rule).
- No change to a ticket already recorded for the session; no re-asking it.
- No ticket is created without the Owner's one confirm in chat.
- No question in non-interactive runs (`claude -p`, SDK).

## 3. Journeys

### Journey 1 — Pick a ticket at start (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a new session in a work folder, branch `feat/GENAI-42-x` | the session starts | within 2 s a dialog offers `GENAI-42`, a recent ticket, "Create a new ticket", "Skip"; picking `GENAI-42` records it and a toast says `worklog: GENAI-42` |
| Error | the worklog daemon is down | the session starts | the dialog still offers the branch ticket, "Create", "Skip"; if recording fails, one `$.ui.log` line `worklog: could not record KEY — <stderr>` appears and nothing else breaks |
| Edge | the Owner presses Esc | the dialog closes | nothing is recorded and the session is not asked again |

### Journey 2 — Create or skip (Owner, Claude)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | the Owner picked "Create a new ticket" | the Owner sends the first request | Claude is told to create a ticket for it (one confirm) and record it with the session id |
| Error | ticket creation fails | Claude reports it in chat | nothing is recorded; the create instruction stays in context; the mod shows no dialog |
| Edge | the Owner picked "Skip", then resumes the session later | the session resumes | no dialog |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The mod MUST ask the Owner one question at the start of an interactive work-folder session with no answer stored for that session | mod test: work cwd, fresh session → one dialog |
| FR-02 | MUST | The mod MUST NOT ask outside work folders, in non-interactive runs, or when the session already has a stored answer | mod tests for each case → no dialog |
| FR-03 | MUST | The mod MUST ask again after `/clear` (new session id) | mod test: clear → one dialog for the new id |
| FR-04 | MUST | The question MUST offer at most 4 choices, in order: the branch ticket (if any), then recent Owner tickets (2 without a branch ticket, 1 with one, fewer if the list is short), "Create a new ticket", "Skip" | unit test of the choice builder |
| FR-05 | MUST | Choosing a ticket, or typing a ticket key under "Other", MUST record it for the session | mod test: record command run with key and session id |
| FR-06 | MUST | "Skip", Esc, or text that is not a ticket key MUST record nothing and stop further asking in that session | mod tests |
| FR-07 | MUST | After "Create", Claude MUST be told on each prompt, until a ticket is recorded, to create one with one confirm and record it with the session id | mod test: context block text |
| FR-08 | MUST | The chosen ticket MUST replace the branch guess in Claude's context and in the commit/PR ticket line | mod tests |
| FR-09 | MUST | The SessionStart hint MUST NOT tell Claude to ask the Owner for a ticket | cargo test on the hint text |
| FR-10 | SHOULD | Free text under "Other" that is not a key SHOULD be passed to Claude as "find the ticket for: <text>" | mod test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Time to dialog | ≤ 2 s after session start, daemon lookup capped at 1.5 s | mod test with a slow daemon mock: dialog still shown, without recent tickets |
| Questions per session | ≤ 1 | mod tests across start, resume, compact |
| Security | 0 new network targets: only the local daemon (127.0.0.1:9323) and the local `worklog` binary | review of ticket.ts |
| Session start blocked | 0 ms waiting on the Owner's answer | the start hook returns before the dialog resolves (test) |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test (`claude plugin test mods/worklog`, `cargo test`)
- [ ] Journey 1 error path (daemon down) is exercised in a test
- [ ] The Owner starts a real session in `~/Desktop/Work/<repo>`, sees the dialog, picks a ticket, and `worklog block list` later shows it on the session's blocks

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | `$.ui.ask(question, { options, header })` opens the engine's AskUserQuestion dialog and rejects when dismissed or with nobody to ask (types doc, `$.ui.ask`); it may be started from `session.start` without awaiting | High | dialog never shows: the rejection is treated as Skip and one `$.ui.log` line says `worklog: no ticket question shown — run worklog ticket use KEY --session <id>` |
| A2 | After `/clear` no `session.start` fires; `session.end` with `reason: 'clear'` is the signal and `$.session.id()` then reads the new id (types doc, `SessionEndInput`) | High | no question after clear |
| A3 | A session the mod asked is remembered per session id in the mod's cross-session store | High | a resume re-asks |
| A4 | "Recent tickets" = `GET http://127.0.0.1:9323/tasks` → `{ tasks: [{ key, summary, status, assigned, last_worked_day, ... }] }` (checked live 2026-10-05; the My Tasks board's source, `web/lib/daemonHub.ts` `tasks()`), filtered to `assigned`, newest `last_worked_day` first, null last | High | weaker choices only |
| A5 | Every Claude Code that runs the worklog hook also loads the mod; older ones lose the nudge | Med | no question on old Claude Code |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Work folder | `~/Desktop/Work/` or a repo whose origin is in the work org — the mod's `workContext().isWork` |
| Session ticket | the key stored by `worklog ticket use KEY --session ID`; it tags the session's events |
| Mod | the worklog Claude Code plugin in `mods/worklog/`, installed by `worklog hook install` |
