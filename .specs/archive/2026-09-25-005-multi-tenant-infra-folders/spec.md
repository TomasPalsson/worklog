# Spec: Multi-tenant infra folders

**Created**: 2026-09-25 · **Route**: dispatch

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: `vitinn-infra` and `genai-infra` hold many customers' tenants, but each folder bills one pinned customer. On 2026-09-24 about 2.5 h of Sjúkra work (blocks 4098, 4101, 4102) went to APRÓ.

**Solution**: In a folder marked multi-tenant, each block is split between customers by clock time, using clues from edited tenant paths, branch names and the block summary. The owner can hand-adjust any block's split.

**Who it's for**: the Owner (the single person who bills their own hours).

**MVP cut line**: everything tagged `MUST` in §4.1 ships. `SHOULD` is v1.1. `MAY` is backlog.

**Key decision**: APRÓ is always the lowest priority. If another customer has any clue in a block, APRÓ gets 0% of that block (D-01).

## 1. Context

### 1.1 Problem statement

Worklog maps one work folder to one customer. The infra repos serve ~20 tenants each (`vitinn-infra/tenants/<t>`, `genai-infra/terraform/workspaces/<ws>/<t>`). Every block in them bills the folder pin, APRÓ, so customer work is under-billed and APRÓ is over-billed.

**Current workaround**: the Owner edits invoice lines by hand after export, or doesn't notice.

### 1.2 Roles

| Role | What they do | Key characteristic |
|------|--------------|--------------------|
| Owner | Reviews the day, exports billing lines to the invoicing form | Wants it automatic, fixes only the odd block |

**Primary actor**: Owner.
**Hidden stakeholders**: the customers on the invoice (they must never be billed for invented time).

## 2. Scope

### 2.1 In scope

- A per-folder "multi-tenant" flag, with tenant roots, seeded for `vitinn-infra` and `genai-infra`.
- Automatic tenant → customer mapping by alias; unmatched tenants listed for a one-time link.
- A time-based customer split of each block in a multi-tenant folder, used by the billing export.
- A per-block hand-set split in the review UI that survives re-inference and re-estimation.

### 2.2 Non-goals

> Binding. A change here is an amendment, not an interpretation.

- No guessing `Verkefni` — it still comes only from an explicit folder pin. — user, Q8
- No change to Tempo sync. — user, Q8
- No change to browser/Slack event routing. — user, Q8
- No fixing blocks filed under the wrong folder (e.g. block 4099 "Release worklog 0.12.0" under vitinn-infra) — separate problem. — user, Q8
- No global setting for the split method (D-03).

## 3. Journeys

### Journey 1 — Export a mixed day (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | 2026-09-24 data; `vitinn-infra` multi-tenant | Owner opens the billing export | A Sjúkra line holds blocks 4098, 4101, 4102 (~2.5 h); an APRÓ line holds the rest of the vitinn-infra time |
| Error | A tenant clue names an Unmatched tenant | Owner opens the export | That clue is ignored (never guessed); the tenant shows in the Billing panel as Unmatched |
| Edge | A multi-tenant block has no timestamped clue | Owner opens the export | The block bills as today: summary alias match, else folder pin |

### Journey 2 — Link a tenant once (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | `byko-datalake` is Unmatched | Owner links it to a customer in the Billing panel | It shows as Link; the next export uses it |
| Error | Owner links to a customer that no longer exists | Owner saves | Save is refused with "Customer no longer exists"; the tenant stays Unmatched |
| Edge | `uat` is not a customer | Owner marks it "not a customer" | It shows as Ignored and gives no clues |

### Journey 3 — Adjust one block (Owner)

| Path | Given | When | Then |
|------|-------|------|------|
| Happy | Block 4104 auto-split 70/30 | Owner sets 50/50 and saves | Export shows the 50/50 hours; re-infer of the day keeps 50/50 |
| Error | Shares don't sum to 100% | Owner saves | Save is refused with "Shares must add up to 100%"; the old split stays |
| Edge | Owner clears the hand-set split | Owner saves "reset" | The block goes back to the automatic split |

## 4. Requirements

### 4.1 Functional requirements

| ID | Priority | Requirement | Acceptance |
|----|----------|-------------|------------|
| FR-01 | MUST | Owner MUST be able to mark a folder multi-tenant in the Billing panel | Toggle persists across daemon restart |
| FR-02 | MUST | The system MUST seed `vitinn-infra` (root `tenants`) and `genai-infra` (root `terraform/workspaces/*`) as multi-tenant | Fresh DB lists both folders flagged with those roots |
| FR-03 | MUST | The system MUST map a tenant to a customer when its name matches exactly one customer name or alias | `sjukra` → Sjúkra, `apro-prod` → APRÓ in a test |
| FR-04 | MUST | The system MUST list tenants with no match as Unmatched and give them no customer | `byko-datalake` has `customer: None` |
| FR-05 | MUST | Owner MUST be able to link an Unmatched tenant to a customer, or mark it Ignored | Linked tenant resolves; Ignored gives no clue |
| FR-06 | MUST | For a block in a multi-tenant folder, the export MUST give each second of the block to the customer of the nearest timestamped clue; on an exact tie the earlier clue wins | Block with clues Sjúkra@13:40, MMS@15:30 splits at the midpoint; the midpoint second goes to Sjúkra |
| FR-07 | MUST | Rules run in this order: FR-07, FR-08, FR-06, then FR-09 only if no timestamped clue is left. FR-07 works on timestamped clues only: when any non-APRÓ timestamped clue exists in a block, the export MUST drop that block's APRÓ timestamped clues | Block with APRÓ + Sjúkra clues → 100% Sjúkra |
| FR-08 | MUST | After FR-07, when clues of different strength fall in the same minute, the export MUST keep only the strongest (path > branch > summary) | `mms` branch + `sjukra` path in one minute → Sjúkra; `sjukra` branch + `apro-prod` path → APRÓ dropped by FR-07 first, so Sjúkra |
| FR-09 | MUST | A block with no timestamped clue MUST bill by its summary clue (a non-APRÓ customer named in the summary beats APRÓ there too), else by the folder's current resolution | Zero-clue block bills the folder pin; summary naming APRÓ and Sjúkra → Sjúkra |
| FR-10 | MUST | Owner MUST be able to set a block's split by percentage in the review UI | 50/50 saved and shown |
| FR-11 | MUST | A hand-set split MUST survive re-inference and re-estimation of the day when the block's start is unchanged; if the start moves, the split MUST be dropped, never applied to a different block | Re-infer test keeps shares; moved-start test shows the auto split |
| FR-12 | MUST | A block's slices MUST sum exactly to the block's duration, and each line's hours MUST stay the union of its intervals | Property test over random clues |
| FR-13 | MUST | Folders not marked multi-tenant MUST bill exactly as today | Existing billing tests pass unchanged |
| FR-14 | SHOULD | The block card SHOULD show the computed split and its origin (clues / manual / fallback) | Card renders "Sjúkra 70% · MMS 30% · auto" |

## 5. Non-functional requirements

| Dimension | Number | How it is measured |
|-----------|--------|--------------------|
| Export latency | `rows_for_day` for a 30-block, 2 000-event day < 500 ms | Timed test on a seeded fixture |
| Reliability / security | N/A — single-owner local daemon; writes are serialised by the daemon's one DB connection | — |
| Split exactness | 0 seconds lost or duplicated per block | FR-12 property test, 1 000 random cases |

## 6. Launch criteria

- [ ] Re-run 2026-09-24 and open the billing export: a Sjúkra line covering blocks 4098, 4101, 4102 (~2.5h) and an APRÓ line with the rest of the vitinn-infra time; then change one block's split in the review UI, re-estimate the day, and the hand-set split is still there. — user, Q9
- [ ] Every MUST in §4.1 has a passing test.
- [ ] The error path of each journey is exercised.
- [ ] The §5 latency number is measured.

## 7. Assumptions

| # | Assumption | Confidence | Blast radius if wrong |
|---|------------|-----------|-----------------------|
| A1 | `genai-infra`/`vitinn-infra` are pinned to APRÓ in the live DB, and a pin wins today | High | None — the flag bypasses the pin for split blocks |
| A2 | An unpinned folder falls back to alias match on description + Jira summary | High | FR-09 fallback differs |
| A3 | `claude_work.details` carries `branch <b>` and `edited <paths>`; `git_reflog` titles carry `checkout <b>`; shell rows carry no clue | High | Fewer clues → more fallback blocks |
| A4 | Worktree names in `project_path` (`.claude/worktrees/<name>`) count as a Branch clue | Medium | Worktree-only sessions fall back |
| A5 | Some workspace subdirs are not tenants (`builds`, `uat`, `google-drive`, `tomas`); they surface Unmatched | Medium | A few extra one-time clicks |
| A6 | Seeding via migration fits the `SEED_CUSTOMERS` pattern and the "UI-only registry edits" rule | Medium | Seed moves to a first-run step |
| A7 | `mark_exported` marks the whole day, so a split block can't be half-exported | High | Purge could drop an unbilled slice |
| A8 | Hand-set shares keyed by `(day, started_at)` match the infer carry's strict key; a moved block start drops the shares silently | Medium | Owner re-enters a split after a big re-infer |

## 8. Open questions

1. None — PREP's storage question is settled in `design.md` (shares table keyed by day + start).

## Appendix A — Glossary

| Term | Means |
|------|-------|
| Multi-tenant folder | A work folder flagged to hold many customers' tenants |
| Tenant root | Path inside the folder whose child dirs are tenants |
| Clue | A timestamped hint that work was for a customer |
| House customer | APRÓ — always lowest priority |
| Slice | One customer's part of one block |
