# Spec: Session lanes and fresh descriptions

**Created**: 2026-09-27 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: One repo that serves two customers (vitinn-infra: Sjúkra and APRÓ) becomes one mixed block, and a block that a rebuild grows a lot (Friday 15:09 → 15:09–18:11, 182 min) keeps the old short block's description.

**Solution**: Sessions in one repo that clearly belong to different customers get their own lane; a block that changed a lot on rebuild is described again, and a long block is described as the few tasks it holds.

**Who it's for**: The Owner reviewing and billing a day.

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: Split a repo by session **only** when the sessions resolve to different customers under the existing rules — never by session alone.

## 1. Context

### 1.1 Problem statement

The Owner runs several Claude sessions at once, sometimes in the same repo for different customers. Lanes are keyed by repo folder only, so those sessions share a lane and one block can mix two customers. On rebuild, a block's description is carried from the prior block with the same start (or an overlapping one) regardless of how much the block changed, so a 3-hour block can read like a 30-minute one.

**Current workaround**: The Owner hand-splits customer shares and rewrites descriptions.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day, bills customers | Runs parallel sessions in shared repos |

**Primary actor**: Owner.
**Hidden stakeholders**: Billing export (reads block folder/customer), estimator (writes descriptions).

## 2. Scope

### 2.1 In scope

- Resolve each Claude session's customer from its own events with the existing customer rules.
- Give sessions of one repo separate lanes when (and only when) they resolve to different customers.
- Drop a carried, non-manual description when the rebuilt block changed a lot, so it is described again.
- Describe blocks ≥ 90 min as up to 3 tasks.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No new UI.
- No change to billing hours math (union of intervals, rounding).
- No per-session customer picker for the Owner.
- No merging of adjacent same-project blocks split by a saved customer split (Friday 12:18/12:34 stays two blocks).

## 3. Journeys

### Journey 1 — Two customers in one repo (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | vitinn-infra has session A naming Sjúkra and session B naming APRÓ, interleaved | the day is inferred | A's minutes and B's minutes land in separate blocks |
| Error | session B names no customer (or two) | the day is inferred | B stays in the repo's main lane; nothing is guessed |
| Edge | two sessions in one repo both resolve to the same customer | the day is inferred | one lane, blocks identical to today |

### Journey 2 — A block grows on rebuild (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | a 30-min block described by the estimator becomes 182 min on rebuild | the day is re-inferred and estimated | its description is regenerated and names up to 3 tasks |
| Error | the grown block's description was written by the Owner (`manual`) | the day is re-inferred | the description is kept unchanged |
| Edge | the block changed by < 30 min and ≤ 1.5× either way | the day is re-inferred | the description carries over as today |
| Edge | a mixed 60-min block is split by session into 25 + 35 min | the day is re-inferred | both pieces start undescribed and are described again |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | The system MUST resolve a session's customer from that session's own event text using the existing single-customer match; zero or 2+ matches mean "unknown" | unit test: one-match, zero-match, two-match sessions |
| FR-02 | MUST | The system MUST give a session its own lane only when its repo has sessions resolving to 2+ different customers that day | unit test: interleaved two-customer sessions → separate blocks |
| FR-03 | MUST | A session with unknown customer MUST stay in its repo's main lane | unit test |
| FR-04 | MUST | Repos whose sessions resolve to one or no customer MUST produce exactly today's blocks | existing infer tests stay green; Friday totals unchanged within 8.35–8.65 h |
| FR-05 | MUST | A split-off block MUST keep its repo as its billing folder | unit test on the block's folder |
| FR-06 | MUST | On rebuild, a carried non-manual description MUST be dropped (`description` and `estimated_by` set to NULL) when the new block's length differs from the prior block's by ≥ 30 min, or the longer of the two is > 1.5× the shorter — growth and shrink alike, so a mixed block cut by a session split is described again | unit test: grow ≥30, grow >1.5×, shrink >1.5×, just-under case |
| FR-07 | MUST | A `manual` description MUST always carry over | unit test |
| FR-08 | MUST | The estimator MUST ask for a description of up to 3 tasks, joined by "; ", for blocks ≥ 90 min | unit test on the prompt/user message sent for a 90-min vs 89-min block |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Friday 2026-09-25 work total | 8.35–8.65 h | `scripts/verify-inference.sh` |
| Friday block count | ≤ 18 blocks, ≤ 2 under 10 min | `scripts/verify-inference.sh` |
| Long-block description | ≤ 3 tasks, ≤ 140 chars for the whole joined description | unit test on the estimator schema/prompt |

## 6. Launch criteria

- [ ] Every MUST in §4.1 has a passing test
- [ ] The error path of each journey is exercised by a test
- [ ] `scripts/verify-inference.sh` passes with the §5 numbers
- [ ] Live Friday: 15:09–18:11 (or its successors) carries a fresh multi-task description

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | A session's prompts/branches/tickets name its customer often enough for `customer_in_text` to resolve it | Med | Split never fires; blocks stay as today (safe) |
| A2 | Billing already resolves a split-off block's customer from its own text or the Owner's shares; no billing change needed | Med | A split block bills to the folder pin; Owner fixes with a share |
| A3 | Build starts after PR #54 merges | High | Merge conflicts in infer_lanes / infer |
| A4 | Thresholds 30 min / 1.5× / 90 min are right for the Owner's days | Med | Too many or too few re-descriptions; tune constants |

| A5 | A failed or malformed long-block model call is handled by the estimator's existing error path (block stays undescribed, retried next run) | High | One block left undescribed |

## 8. Open questions

None.

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Lane | The track a repo's events compete in for owning each minute (`infer_lanes`) |
| Carry | Copying a prior block's description/ticket onto the rebuilt block |
| Session | One Claude Code session (`session_id` on events) |
