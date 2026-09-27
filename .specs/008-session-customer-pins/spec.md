# Spec: Session customer pins

**Created**: 2026-09-27 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: In a shared repo (vitinn-infra), most Claude sessions never name their customer, so worklog leaves them unknown — on Friday 2026-09-25, 8 of 11 vitinn-infra sessions — and the Owner re-splits the hours by hand.

**Solution**: At session start in a shared repo, Claude is told to work out the customer (from the repo, the prompt, the files it touches) and pin it with one command — asking the Owner only when it truly can't tell. A pin is remembered per branch, drives the block split, and shows as "pinned" in the review UI, where the Owner can still change it.

**Who it's for**: The Owner, who reviews and bills the day.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Priority is the Owner's UI choice → the pin → the text guess → the fallback (D-05).

## 1. Context

### 1.1 Problem statement

The Owner runs Claude sessions for several customers inside one repo. Worklog guesses a session's customer from its prompt text and gives up when the prompts don't name one ("PR", "approved", "/flow:next"). Those minutes stay unknown or land on the wrong customer until the Owner fixes them. The in-session Claude usually knows the customer (the task it is doing) but has no way to say so.

**Current workaround**: The text guess (spec 007) plus hand-set customer splits per block.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day, bills customers | Runs parallel sessions in shared repos; often away (background jobs) |
| Session Claude | Does the work in a Claude Code session | Sees the repo, branch, files and prompts; may run commands |

**Primary actor**: Session Claude (pins); Owner (reviews/changes).
**Hidden stakeholders**: Billing export (reads block customer slices); the estimator (unchanged).

## 2. Scope

### 2.1 In scope

- A start-of-session instruction in shared repos telling Claude to work out and pin the session's customer.
- A command that pins a customer to a session, from a given time on (a later pin = a switch).
- Remembering a pin per repo + branch, so later sessions on that branch inherit it (not on the default branch).
- Pins driving the per-customer lanes and the block's customer line.
- Showing the pinned customer, marked as pinned, in the day view and the block detail view.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- Never ask outside `~/Desktop/Work` — anything in /Work is work, anything outside is not.
- Don't ask when the repo name (`apro-skills`, `sjukratryggingar-genai`) or the prompt ("Go into RU and do this") already makes the customer obvious — just assume it.
- Never guess when nobody is there to ask (background jobs, sub-agents, unattended runs) — leave it unknown.
- No after-the-fact model guess from prompts: on Friday 2026-09-25 it resolved the same 3 of 6 vitinn-infra sessions the existing text matcher already does, at more tokens.
- Never re-ask in a session that continues work whose customer is already known (e.g. `/flow:next` after `/clear` on the same feature) — user, Q1
- No change to how the Owner's hand-set split is saved or edited.
- No UI to edit or delete a pin: the Owner's Change corrects one block (saved split, wins over the pin); a wrong pin as a whole is replaced by pinning again with `--at` the session's start.
- No Tempo or Jira work.

## 3. Journeys

### Journey 1 — Pin a shared-repo session (Session Claude)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A new session in vitinn-infra on a feature branch with no pin; the prompt says "work on the Sjúkra config" | the session starts and Claude reads the instruction | Claude pins Sjúkra without asking; the session's blocks show "Sjúkra (pinned)" after the next rebuild |
| Error | Claude pins "Sjukra tryggingar" (not a known name or alias) | it runs the pin command | the command refuses, exits non-zero and prints the known customers; nothing is stored |
| Edge | A background job with no Owner present can't tell the customer | the session starts | nothing is pinned; the session stays unknown |

### Journey 2 — Continue on the same branch (Session Claude)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | The branch already has a Sjúkra pin | `/flow:next` starts a new session after `/clear` | the new session inherits Sjúkra; Claude is told the customer and never asks |
| Error | The session is on `main` | it starts | nothing is inherited; Claude works it out or asks |
| Edge | Mid-session the Owner says "now do the APRÓ thing" | Claude pins APRÓ | minutes from that moment go to APRÓ, earlier ones stay Sjúkra |

### Journey 3 — Review and change (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | A block whose session is pinned to Sjúkra | the Owner opens the day view or the block detail | "Customer: Sjúkra" with a "pinned" tag |
| Error | The pin is wrong | the Owner uses Change on that block | the Owner's choice wins for that block ("set by you"); the pin is untouched |
| Edge | A block with no pinned session | the Owner looks | today's line (auto / guess) is unchanged |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Session Claude MUST be given a pin instruction at session start only when the session's folder is under `~/Desktop/Work` and is marked multi-tenant, and the branch has no inherited pin | unit test: multi-tenant folder → instruction; single-customer folder, personal folder → no output |
| FR-02 | MUST | Session Claude MUST be able to pin a customer to its session from a given time on, with the pin command `worklog pin <customer> --session <session-id> [--at <time>]` (time defaults to now); a later pin for the same session supersedes earlier ones from its own time on | CLI test: pin, re-pin with `--at` earlier, events follow the latest applicable pin |
| FR-03 | MUST | A pin MUST resolve to a known customer name or alias (case-insensitive); anything else MUST be refused with the known customers listed and nothing stored (D-03) | CLI test on a typo |
| FR-04 | MUST | A new session on a non-default branch that already has a pin MUST inherit that branch's latest pin and receive the customer in its start text instead of the instruction (D-01) | unit test |
| FR-05 | MUST | A session on the default branch (`main`/`master`) MUST NOT inherit (D-02) | unit test |
| FR-06 | MUST | A pinned session's events MUST take its pinned customer as their lane, over the text guess, from the pin's time on (D-05) | unit test incl. a mid-session switch |
| FR-07 | MUST | A block's customer line MUST be: Owner's saved split, else pinned customer (when the block's pinned sessions name exactly one), else today's clues/fallback (D-05) | unit test on slice origin |
| FR-08 | MUST | The day view and the block detail view MUST show a pinned block's customer tagged "pinned", with Change available (D-04) | web test |
| FR-09 | MUST | Installing worklog's hooks MUST install the start instruction alongside the existing recorder, and uninstall MUST remove both, idempotently | unit test on settings file |
| FR-10 | MUST | The start instruction MUST never block or fail a session: any error prints nothing and exits 0 | unit test |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Start-instruction runtime | ≤ 300 ms p95 on the Owner's Mac | time the hook command 20× in a multi-tenant folder |
| Start-instruction size | ≤ 600 characters printed | unit test on output length |
| Pin command | ≤ 1 s, exit 0 on success / 2 on unknown customer | CLI test |
| Data leaving the machine | 0 bytes — pins live only in the local database; no network call | code review of the new modules |
| Pin names reaching an invoice | 0 names outside the Owner's customer list | FR-03 test |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised by a test
- [ ] In `vitinn-infra` on a feature branch, a new session told "work on the Sjúkra config", then `/clear` + `/flow:next` on the same branch; after the day rebuild both sessions' blocks show "Sjúkra (pinned)" in the day view, and Claude never asked the Owner. — user, Q8
- [ ] `scripts/verify-inference.sh` still passes (no pins exist for past days, so nothing changes)

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | "Shared repo" = a folder marked multi-tenant in the billing registry (today vitinn-infra, genai-infra); every other /Work folder is obvious from its folder pin | High | Hook speaks in the wrong repos; Owner flips the flag |
| A2 | The in-session Claude can almost always work out the customer, so asking stays rare (PREP A-05) | Low | More questions than wanted; still correct |
| A3 | The start hook's printed text reaches Claude's context (Claude Code SessionStart semantics) | High | Instruction unseen; nothing pins; nothing breaks |
| A4 | A UI change saves the Owner's split on that block only and never rewrites the pin (PREP open item; see §2.2) | Med | Owner expects the branch to follow; reply "branch" at approval to change |
| A5 | The session id reaches Claude via the start text, which includes the exact pin command | High | Claude can't pin mid-session without it |
| A6 | Any new reader of event raw data goes through `raw_json::decode_raw_json` (PR #56) | High | Compressed rows fail to read |

## 8. Open questions

1. None beyond A4, stated as a position.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Pin | A statement "session S is for customer C from time T on" |
| Inherit | A new session on the same repo + branch copies the branch's latest pin |
| Shared repo | A /Work folder marked multi-tenant in the billing registry |
| Start instruction | Text printed by the session-start hook into Claude's context |
