# Issue-Breakdown Evaluation Report: .specs/001-web-setup-wizard/plan.md

> **MODE**: Issue-Breakdown (tracer-bullet vertical-slice rubric, 5 dimensions, 100 points)
> **Source spec**: .specs/001-web-setup-wizard/spec.md (version 1.2, Medium tier)
> **Plan**: plan.md — 8 issues, all tagged AFK, shallow fan-out dependency graph

---

## Summary

- **Total Score**: 82/100
- **D-Slice Cap Triggered**: NO — zero horizontal issues detected
- **Size Tier (spec)**: Medium (passed by author)
- **Plan Format**: Tracer-bullet vertical slices with coverage check table
- **AFK/HITL split**: 8 AFK / 0 HITL
- **NFR-to-slice orphans**: 2 (NFR-E4, NFR-O1)
- **Verdict**: A disciplined vertical breakdown with a clean dependency graph and comprehensive coverage table — no horizontal masquerade, all issues are demoable end-to-end. Loses points on three specific deficiencies: aggressive zero-HITL claim (one genuine unmade UI decision lurks), two spec NFRs orphaned from any slice's ACs, and Issue 03's mega-scope (16 credential types × 4 behavioral modes in a single agent task).

---

## Dimension Scores

| Dimension | Score | Max | Notes |
|-----------|-------|-----|-------|
| D-Slice — Vertical Integrity | 31 | 35 | All 8 vertical; walking skeleton (01) produces only a page shell — thin but observable |
| D-AFK — Classification Accuracy | 15 | 20 | Zero HITL is aggressive; error-display component is unmade visual decision; soft checkpoint correctly not tagged HITL |
| D-Dep — Dependency Soundness | 13 | 15 | Acyclic, all references resolve; 04→03 rationale is sound; no gaps in ordering |
| D-Trace — Spec Coverage | 16 | 20 | NFR-E4 (connection-lost page state) and NFR-O1 (log fields/levels) are orphaned — no slice covers them |
| D-Gran — Granularity (INVEST) | 7 | 10 | Issue 03 is a mega-issue (16 credential keys × 4 modes × 7 explicit ACs); issues 04–08 are well-sized |
| **Total** | **82** | **100** | |

---

## Horizontal-Masquerade Detector Results

The detector was applied to every issue title and demoable result.

| Issue | Title pattern | Merged-alone observable? | Verdict |
|-------|--------------|--------------------------|---------|
| 01 | "Owner runs `worklog setup` and a daemon-served setup page opens; `--cli` still runs the TTY wizard" | Yes — browser opens a rendered shell; URL is printed; `--cli` runs TTY wizard | **VERTICAL** |
| 02 | "Setup page shows fresh-install bootstrap (home + DB) and preflight diagnostics on load" | Yes — page shows schema version + event/block counts + per-tool preflight panel | **VERTICAL** |
| 03 | "Owner can enter, live-validate, and save any credential, with inline errors, save-anyway, and re-run state" | Yes — full credential lifecycle visible in the browser | **VERTICAL** |
| 04 | "Owner can choose the estimator provider (subprocess `claude -p` or LiteLLM)" | Yes — provider toggle + optional health probe visible | **VERTICAL** |
| 05 | "Owner can set or disable the scheduled-collection interval" | Yes — scheduler entry installed; page reflects interval | **VERTICAL** |
| 06 | "Owner can install/refresh the Claude Code hook from the page" | Yes — hook installed with event count | **VERTICAL** |
| 07 | "Owner can install/refresh the bundled skill, with byte-for-byte "files differ" detection" | Yes — differs/up-to-date state rendered; file count returned | **VERTICAL** |
| 08 | "Owner can install/refresh daemon auto-start, with a non-blocking failure + retry" | Yes — failed step + retry control visible alongside working steps | **VERTICAL** |

**No horizontal issues.** D-Slice cap (total ≤ 50) is NOT triggered.

---

## Critical Issues

### 1. NFR-E4 is orphaned — no slice has an AC for "daemon drops during an active session"

The spec states: `NFR-E4 | Daemon not reachable from the page | Page shows a connection-lost state`. No slice's acceptance criteria address what the browser renders when the daemon becomes unreachable mid-session. The coverage check table lists every FR-NNN and AC-NNN but omits all NFR-to-slice mappings, so this was not caught by the table. An agent building all 8 slices can ship without implementing the connection-lost state.

### 2. NFR-O1 is orphaned — no slice verifies the log contract

The spec defines a specific log contract: `action` field, `key`/`step` name field, `outcome` field, `info` on success, `warn` on validation/network-probe failure, no secret value emitted. No issue AC tests this. An agent will implement logging ad-hoc without verification. For a spec that calls "no secret value emitted" a security requirement, this is a gap with production consequences.

### 3. Issue 03 mega-scope without an explicit agent scoping note

Issue 03 asks one agent to implement all 16 `KNOWN_KEYS` credential types × 4 behavioral modes (set/replace/save-anyway/skip) × 7 explicit ACs including a full live-validation and network-probe path. The KNOWN_KEYS table in §4.1.1 alone spans 16 rows with heterogeneous validation rules. This is the widest single issue in the plan; without a scoping note telling the agent to implement all 16 rows from the table (rather than just the GitHub canonical case), there is a real risk of partial implementation.

---

## Top 3 Improvements

1. **Add NFR-E4 and NFR-O1 to the coverage check table and assign each to a slice AC.** For NFR-E4, add to Issue 01 (the skeleton): `AC — Given the daemon process dies while the setup page is open, When the browser polls or the owner submits a form, Then the page shows a "connection-lost" state with a reload/retry link.` For NFR-O1, add to Issue 03 (first write endpoint to touch the daemon): `AC — Given any credential save action, When the daemon processes it, Then the daemon log emits exactly one line at `info` (success) or `warn` (validation/probe failure) carrying `action`, `key`, and `outcome` fields, and the line contains no secret value or token string.` These two ACs close the orphaned spec requirements that the current 8-issue set leaves unverifiable.

2. **Add an explicit scoping sentence to Issue 03 instructing the agent to implement every row in §4.1.1, not just the canonical GitHub case.** The current issue body says "§4.1.1 (full `KNOWN_KEYS` table)" in the spec trace, but the AC reads: `Given any key in the KNOWN_KEYS parity set, When the Owner submits it, Then that row's local validation and network-probe rules apply (GitHub is the canonical case…)`. The parenthetical "(GitHub is the canonical case)" risks an agent treating GitHub as the only required implementation. Add a directive: "The agent MUST implement all 16 rows in §4.1.1 with their distinct validation rules. GitHub is the worked example in tests; every other row MUST also have a test fixture exercising its local validation rule (or explicit `none` for keys with no local check)."

3. **Tag Issue 01 as HITL with a specific blocking decision: "does the rendered page shell match the owner's visual expectations before dependent slices build on it?"** The preamble's soft-checkpoint note — "if the author wants to eyeball the page before the install slices land, slice 01 is the natural place to pause — it is not a blocking decision, so it is not tagged HITL" — is sound reasoning, but it leaves agents 02–08 building on a UI shell the owner has never approved. Since 02–08 all depend on 01's page structure, a mismatch discovered at slice 06 costs more to fix than one caught after slice 01. The specific blocking decision to name: "Owner eyeballs the page shell layout (preflight panel placeholder, credential section skeleton, install section skeleton) and confirms the section order and styling match expectations before dependent slices proceed." If the plan's author truly accepts zero visual feedback before all 8 slices land, replace the HITL tag with an explicit AFK justification sentence: "The page layout is fully determined by 'mirroring the TTY wizard's section order' (Appendix B) and 'existing review-UI styling' (NFR-A4); no author eyeball is required." The current preamble implies this but does not state it as a binding decision for the agent — bridging that gap removes ambiguity.

---

## Detailed Analysis

### D-Slice (31/35) — Minor deduction on walking skeleton thinness

Issue 01's demoable result is "opens `/setup` (a rendered shell) in the default browser." A QA engineer merging only Issue 01 can verify: daemon starts, browser opens, URL is printed, `--cli` runs the TTY wizard. What the QA *cannot* verify is that any content is visible on the page — the shell may be an empty frame. This is by design (content comes in slices 02–07), but it means Issue 01's merged state is a skeleton, not a working feature increment. This is the standard tracer-bullet trade-off and is not a defect — the deduction (−4) reflects that the "demoable result" description could more precisely state what the shell renders (e.g., "a page with placeholder sections for preflight, credentials, and installs — all showing loading/empty states") so a QA reviewer knows exactly what to check.

### D-AFK (15/20) — Zero HITL is defensible but one gap remains

The preamble's defense of all-AFK is substantive and well-argued. Mr Claude finds one unmade decision the defense does not explicitly close: the inline error display component. NFR-E1 says "Inline error next to the field" — this pins placement but not the visual component (red label text, bordered input, toast, tooltip). The existing review-UI has established patterns, so an agent following NFR-A4 ("follow existing review-UI styling") will make a reasonable choice. But "reasonable choice" is by definition a D (Debatable) item per the buildability rubric. The fix is one sentence in Issue 03's seam note: "Inline errors MUST use the existing review-UI field-error component (red label beneath the input, as seen in [existing component reference])." If no such component exists yet, this is the decision that warrants a HITL tag on Issue 03.

### D-Trace (16/20) — Two NFR orphans

The coverage check table maps every FR-NNN, AC-NNN, and Journey to a slice — this is excellent practice and rare in breakdowns at this size. The gap is that the table includes only functional items, not NFRs. The two orphaned NFRs are:

- **NFR-E4** ("Page shows a connection-lost state") — maps to no slice AC. Closest coverage is Issue 01's `Edge — Given the daemon is already running...` which handles daemon reuse, not daemon loss.
- **NFR-O1** ("daemon MUST log... each setup action is one line carrying at least `action`, `key`/`step`, `outcome`; never emits secret values") — maps to no slice AC. This is a security-adjacent requirement (no secret in logs) that is currently trust-based only.

The fix is to extend the coverage check table with an NFR row section and add the two ACs described in the Critical Issues section above.

### D-Gran (7/10) — Issue 03 is the plan's load-bearing mega-issue

Issue 03 covers:
- FR-004 (full KNOWN_KEYS parity — 16 credential types)
- FR-004b (keep/replace/skip/set/skip mode logic)
- FR-005 (save-anyway semantics)
- FR-012 (no plaintext return)
- NFR-E1 (inline errors)
- NFR-E2 (network probe warning)
- NFR-P2/P3 (latency bounds)

That is 7 explicit ACs plus an implicit obligation to implement all 16 KNOWN_KEYS rows. The INVEST "Small" criterion is strained. A reasonable split: Issue 03a covers the credential form infrastructure (one credential end-to-end: GitHub, with local validation + network probe + save-anyway + re-run state), and Issue 03b covers parity for the remaining 15 keys (applying the same infrastructure to each row in §4.1.1). This split preserves vertical integrity (03a is demoable alone) and reduces per-issue scope. It also surfaces the parity obligation as an explicit deliverable rather than an implication of the spec trace.

---

## Detected Anti-Patterns

| Pattern | Severity | Issue | Evidence |
|---------|----------|-------|----------|
| NFR Graveyard in breakdown (AP-06 variant) | Medium | Coverage table | The coverage check maps FRs and ACs but omits all NFR-to-slice assignments; NFR-E4 and NFR-O1 fall through |
| Mega-issue (INVEST-S violation) | Medium | Issue 03 | "16 credential keys × 4 modes × 7 ACs" — widest issue in the plan by ~3x; risks partial implementation of the KNOWN_KEYS table |
