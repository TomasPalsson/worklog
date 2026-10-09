# Spec: Verdict routing

**Created**: 2026-09-23 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: The local model that files browser and Slack events into projects is confidently wrong — on 2026-09-23 it got 0 of 4 clear cases right and put ~17 events into the wrong project at ≥75% claimed confidence.

**Solution**: A different local model picks the project, and an event is filed only when the pick clearly beats both "not enough evidence" and the runner-up; everything else stays in Unsorted.

**Who it's for**: the worklog owner, who sorts their own day's events and bills from the result.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: trust the model's *relative* scores (winner vs abstain, winner vs runner-up), never its absolute confidence number.

## 1. Context

### 1.1 Problem statement

The owner opens the day page and finds browser tabs and Slack messages filed under projects they never touched, with 99% claimed confidence. Wrong filings are worse than none: they reach blocks and invoices unless the owner spots and fixes each one. A replacement model tested on the same day picked all 4 clear cases right and abstained on the rest, but its absolute confidence is low (0.06–0.39), so today's single threshold cannot use it.

**Current workaround**: leave the model off and sort every event by hand, using "always" rules.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | runs worklog, sorts events, bills | one person, one Mac, no GPU |

**Primary actor**: Owner.
**Hidden stakeholders**: the invoicing form downstream — a wrong filing becomes a wrong invoice line.

## 2. Scope

### 2.1 In scope

- A local helper that answers "which project is this?" with the winner's score, the runner-up's score and the "not enough evidence" score
- A filing rule on those three scores with two owner-tunable ratios
- Replacing the old helper, its start/status command, its setting and its status field

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No cloud or hosted model; nothing about an event leaves the machine
- No training, fine-tuning or re-calibration of the model
- No "maybe it's X?" hints in the Unsorted list — an event is filed or left alone

## 3. Journeys

### Journey 1 — Clear clue gets filed (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | the helper is running; a Slack message links `github.com/<org>/vitinn-infra/pull/802` and `vitinn-infra` is a project | routing runs for that day | the event is filed under `vitinn-infra` with origin `guess` |
| Error | the helper is not running | routing runs | the event stays in Unsorted; the day page loads normally; stderr notes the helper is unreachable |
| Edge | the winner beats the runner-up by ×1.29 but only ties "not enough evidence" | routing runs | the event stays in Unsorted |

### Journey 2 — No clue stays unsorted (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a DM says "Skoða" | routing runs | the event stays in Unsorted |
| Error | the helper returns a folder that is not a project | routing runs | the event stays in Unsorted |
| Edge | there are more than 24 projects | routing runs | every project can still be picked |

### Journey 3 — Tune the rule (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Settings is open | the owner sets the runner-up ratio to 1.20 and saves | the next routing run uses 1.20 |
| Error | the owner enters 0.9 | saves | the save is refused with a message; the old value stays |
| Edge | the old single-threshold setting is still in the envfile | the daemon starts | it is ignored; defaults apply |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The system MUST file an event only when winner ≥ abstain × abstain-margin (default 1.05) | unit test with fixed scores on both sides of the line |
| FR-02 | MUST | The system MUST file an event only when winner ≥ runner-up × runner-up-ratio (default 1.10) | unit test with fixed scores on both sides of the line |
| FR-03 | MUST | The system MUST leave the event unsorted when the helper is unreachable, errors, or names a non-project | unit tests for each |
| FR-04 | MUST | The helper MUST consider every project even when there are more than 24 | helper self-test with 44 projects |
| FR-05 | MUST | The Owner MUST be able to read and change both ratios in Settings | daemon + web tests |
| FR-06 | MUST | The system MUST refuse a ratio below 1.0 or above 5.0 | daemon test: 400 and value unchanged |
| FR-07 | MUST | The Owner MUST be able to start the helper with one command | CLI test asserts the start command carries the pinned revisions |
| FR-07b | MUST | The Owner MUST be able to check the helper with one command | CLI test asserts status reports reachable / not reachable |
| FR-08 | MUST | The helper MUST use a fixed version of the model code and model files | pins visible in the start command |
| FR-09 | MUST | The old helper, its command and its setting MUST be gone | grep finds no `laya` in code |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Per-event decision, warm | ≤ 0.5 s | helper timing on the owner's Mac, 25+ events |
| Helper cold start, first run | ≤ 90 s after download | wall clock from start to healthy |
| Helper start, later runs | ≤ 10 s | wall clock from start to healthy |
| Per-call wait before giving up | 10 s | client timeout |
| One-time model download | ≈ 1.2 GB | size on disk |
| Network egress per event | 0 bytes (local-only; see §2.2) | helper binds 127.0.0.1 only; no outbound call after the model download |
| Degradation on helper failure | 100% of affected events land in Unsorted; day page still loads | FR-03 tests |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] Each journey's error path is exercised by a test
- [ ] On a real day, the owner sees clear-clue events filed and no-clue events unsorted, with 0 wrong filings
- [ ] Warm per-event time and start times measured on the owner's Mac

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | Defaults ×1.05 (abstain) and ×1.10 (runner-up) hold with the model repo's published calibrator; they were measured with the git copy, which differs | Med | wrong or missing filings until retuned in Settings |
| A2 | Comparing group winners directly (no final round) is good enough; it gave 4/4 right, 0 wrong on 27 events | Med | a near-tie across groups picks the wrong one — the runner-up ratio guards it |
| A3 | 27 events from one day represent normal traffic | Low | thresholds need tuning after a week |
| A4 | The model code stays installable from its git repo at the pinned commit | High | helper will not start; events stay unsorted |

## 8. Open questions

1. [NEEDS CLARIFICATION: none — all positions accepted 2026-09-23]

## Appendix A — Glossary

| Term | Means |
|------|-------|
| abstain score | the model's probability for "not enough evidence to pick any project" |
| runner-up | the second-highest-scoring project for the same event |
| abstain margin | how many times higher than the abstain score the winner must be; default ×1.05 |
| runner-up ratio | how many times higher than the runner-up the winner must be; default ×1.10 |
| filed | the event gets a project with origin `guess` |
| Unsorted | a firefox/slack event with no project |

## Amendment 2026-09-23

Real-day run through the daemon (keys serialised in sorted order, the model repo's published
calibrator) showed the ratios cannot separate a wrong pick ("The Morning Checkup", claude.ai →
claude-3p-config, ×1.16 vs abstain) from right ones (×1.06–×1.23). Every right pick named the repo
outright. Changes:
- FR-10 (MUST): an event whose title or details name exactly one project as
  `github.com/<org>/<project>` or `Desktop/Work/<project>` is filed by rule (origin `rule`) before
  the model is asked; two different named projects → no match.
- DEFAULT abstain margin raises from ×1.05 to ×1.20 (runner-up stays ×1.10). On 2026-09-23: 4 right
  by FR-10, 0 filed by the model, 0 wrong.
- A lone candidate option is never guessed (measured: the model favours a lone named option
  whatever the text says).
