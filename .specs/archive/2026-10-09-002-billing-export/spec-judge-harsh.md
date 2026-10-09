# Spec Judge Report — Billing Export (repo-centric per-day line items)

**Spec**: `.specs/002-billing-export/spec.md` (v1.0, Draft, 2026-07-24)
**Evaluated**: 2026-07-24, cold, maximally harsh (single-pass, no refinement loop)
**Size Tier**: Medium (self-declared; confirmed by content — 388 lines, 1 role, 3 journeys, 16 FRs, 8 NFR sub-categories, 1–8wk-scale effort)
**Spec Format**: Hybrid EARS-style FR table + Given/When/Then acceptance criteria + narrative journeys

---

## Summary

| | |
|---|---|
| **Total Score** | **94 / 120** |
| **Grade** | **C** (70–79%, specifically 78.3%) |
| **Buildability Ratio** | 14 B / 2 D / 0 U across 16 FRs = 87.5% B, 12.5% D, 0% U (nominally "Good" band by ratio alone — but the 2 fabricated technical-reuse claims are a severity outlier the ratio doesn't capture; see Critical Issues) |
| **TBD Count** | 1 unresolved (Q-001, self-annotated low-impact) |
| **Happy:Error Ratio** | ≈1:1 to 1.5:1 at journey level (all 3 journeys have Happy Path + Error Path + Edge Cases) — healthy, better than the 2:1–3:1 "good" benchmark |
| **Verdict** | **Not ship-ready as written.** The functional requirements, acceptance criteria, and NFR coverage are genuinely strong for a Medium-tier spec — but two load-bearing technical claims in the Constraints and Accessibility sections are **fabricated**: they cite specific, named code (`round_to_half_hour`) and a specific, named UI component (`settings-dialog` modal shell) as already existing in the codebase and instruct the implementer to reuse rather than rebuild them. Neither exists anywhere in this worktree. This fails Wiegers' buildability test in the worst way — not because the requirement is unbuildable, but because the spec actively misdirects the developer toward code that isn't there. |

---

## Dimension Scores

| Dim | Score | / Max | Medium-tier pass threshold | Result |
|---|---|---|---|---|
| D1 — Clarity & Unambiguity | 12 | 20 | ≥16 | **FAIL** |
| D2 — Completeness | 16 | 20 | ≥16 | PASS (at threshold) |
| D3 — Testability & Verifiability | 15 | 18 | ≥14 | PASS |
| D4 — NFR Coverage & Quality | 15 | 16 | ≥12 (≥4 categories) | PASS |
| D5 — Structural Integrity | 6 | 12 | ≥10 | **FAIL** |
| D6 — Story/Requirement Form | 11 | 12 | ≥10 | PASS |
| D7 — Traceability & Prioritization | 11 | 14 | ≥11 | PASS (at threshold) |
| D8 — Error & Edge Case Coverage | 8 | 10 | ≥8 | PASS (at threshold) |
| **Total** | **94** | **120** | | **2 of 8 dimensions fail** |

Both failing dimensions (D1, D5) are driven substantially by the same root cause: the two fabricated code/component reuse claims.

---

## Critical Issues

### SHIP-BLOCKER 1 — `round_to_half_hour` does not exist; the spec instructs reuse of nonexistent code

**Location**: §7.1 Technical Constraints (line 322) — *"`round_to_half_hour` already exists (Rust + `format.ts`) | Consistency with what UI shows as billable | Reuse it; do not reimplement rounding"* — and Appendix C (line 385) — *"tempo.rs | The sync/rollup patterns this mirrors (grouping, round_to_half_hour, LLM join)"*.

**Evidence**: Exhaustive search (`grep -rn "round_to_half_hour" rust/ web/` → zero matches; full read of `web/lib/format.ts`, all 13 exported functions listed — `formatClock`, `formatRange`, `formatDuration`, `formatTotalHours`, `todayISO`, `shiftDay`, `mondayOf`, `shiftWeek`, `weekDays`, `formatWeekRange`, `shortWeekday`, `shortMonthDay`, `formatDayHeading` — none rounds to a half hour; the only Rust rounding function found, `round_up_minutes` in `rust/crates/worklog-core/src/estimate.rs`, does unrelated 15-minute time-estimation rounding, not 0.5h billing rounding) confirms no such function exists in this worktree in either language.

**Why it blocks**: FR-002 itself ("round to the nearest 0.5h") is buildable text, but the spec's own Technical Constraints table — the section a developer reads specifically to know what NOT to build — tells them this is already solved and to reuse it. They will search for it, not find it, and either waste time hunting or (worse) build inconsistent rounding logic assuming a canonical implementation exists elsewhere that they simply couldn't locate.

### SHIP-BLOCKER 2 — `settings-dialog` modal shell does not exist; the spec instructs reuse of a nonexistent component

**Location**: §5.7 Accessibility (line 278) — *"Match the existing review UI (the Export panel reuses the `settings-dialog` modal shell: `role="dialog"`, `aria-modal`, Escape-to-close, focusable controls)"* — and Appendix B (line 378) — *"The Export panel reuses the existing `settings-dialog` modal shell."*

**Evidence**: `grep -rln "settings-dialog|settings_dialog|SettingsDialog" web/` → zero matches; `find web -iname "*dialog*"` → zero matches; `find web -iname "*settings*" -not -path "*/node_modules/*" -not -path "*/.next/*"` → zero matches. No dialog or settings component of any name exists anywhere in this worktree's `web/` directory.

**Why it blocks**: This is the spec's entire accessibility strategy for the Export panel — "match the existing UI by reusing X" — and X is not present. The developer gets zero accessibility guidance in practice: no ARIA pattern, no focus-trap implementation, no Escape-to-close logic to copy. They must design this from scratch despite the spec explicitly telling them not to.

---

## Top 3 Improvements

1. **Verify or retract both reuse claims before handoff.** For §7.1/Appendix C: either point to an actual existing rounding utility (none currently exists — if the intent is "FR-002 introduces new rounding logic," say so explicitly and drop "reuse it; do not reimplement") or add the function first. For §5.7/Appendix B: either point to an actual existing modal component (none currently exists — if the Export panel needs a new modal, say "no existing shell; implement `role="dialog"`, `aria-modal`, Escape-to-close, and focus trap as new code") or build the shared component first. As written, both claims will send an implementer on a dead-end search through the codebase.

2. **Resolve the `exported_at` field-shape conflict between FR-008 and §4.2.** FR-008 (line 207) defines the daemon response as a single day-level `{day, rows[], exported_at}`, while §4.2 (line 222) defines `exported_at` as a per-block attribute ("when the block was last exported/billed"). No aggregation rule is given for the case where only some of a day's blocks carry `exported_at` (e.g., a new block was added to an already-exported day). Add an explicit rule — is the day-level field `null` unless every block is exported, the max/most-recent block value, or omitted in favor of per-row `exported_at`? — and cover the partial-export case with a new acceptance criterion.

3. **Fix the FR-012 → AC-022 priority mismatch.** FR-012 (line 211) is tagged `MUST` and cites `AC-020, AC-022` as its acceptance links, but AC-022 (line 190) is tagged `SHOULD` in its own Journey 3 table. Under the spec's own MVP cut line ("Everything tagged MUST in Section 4 ships for v1"), this leaves it ambiguous whether the idempotent-non-overwrite / already-exported-reporting behavior is required for v1 launch. Either promote AC-022 to `MUST` to match its MUST-tagged parent, or split FR-012 into a MUST "set `exported_at` idempotently" requirement (→ AC-020 only) and a separate SHOULD "report already-exported" requirement (→ AC-022 only, matching FR-015 which already covers this ground).

---

## Additional Findings

### SHOULD-FIX

- **FR-016 is an architecture directive dressed as a testable MUST, with a mismatched acceptance link.** "The renderer set MUST be extensible — adding a new output format is a localized change (new renderer), not a change to row computation" (line 215) is a design-quality claim, not something a QA engineer can write a single deterministic test for. Its cited link, AC-006, actually tests that `--format json` output parses as a JSON array — unrelated to extensibility. Either reformulate FR-016 as a testable constraint (e.g., "adding a renderer MUST NOT require changes to `worklog-core`'s row-computation function — enforced by [specific test/lint]") or move it to §7.1 Technical Constraints where design directives belong.
- **No requirement addresses concurrent writes to the new `exported_at` canary.** FR-012 requires setting it "idempotently," implying awareness of repeated calls, but nothing covers overlapping calls (e.g., CLI `--mark` and a web "Mark exported" click racing on the same day, or a block being edited in the review UI at the same instant it's exported). §5.5 Scalability's "single user, local tool" framing implicitly de-risks this, but the connection is never stated for this specific new mutable field.

### NITs

- §1.2's persona row ("Wants a clean, minimal, copy-pasteable breakdown; hates re-transcribing") uses informal/unmeasured adjective language — harmless as persona color rather than a testable requirement, but a stylistic outlier against the otherwise precise FR/AC/NFR tables.
- Q-001 (exact text-line spacing, `hrs` vs `h`, whether to show the ticket) remains an explicitly unresolved open question at spec-finalization time. Self-annotated as cosmetic/low-impact, and resolvable at first test per the doc — correctly scoped, but still a live TBD.

---

## Detailed Analysis (dimensions scoring below 80% of max)

### D1 — Clarity & Unambiguity: 12/20 (60%) — FAIL

The FR/AC/NFR tables themselves are precise: explicit actors (System/Developer), consistent MUST/SHOULD modal discipline matching the Priority column, numeric performance thresholds (§5.1: "< 500 ms... ≤ 200 blocks"), and a defined fallback chain for ambiguous cases (FR-003's dominant-repo derivation: "most-frequent `events.repo`, else the basename of the most-frequent `events.project_path`, else none"). No "and/or" constructs were found; no hedge words ("typically", "as appropriate") were found; passive voice is largely absent since most requirements name an explicit actor.

The dimension fails not because of the standard vague-adjective/passive-voice patterns the rubric's detection techniques target, but because of a more severe variant: two factual claims about the existing codebase, presented as settled constraints in the most authoritative section of the document (§7.1 Technical Constraints, whose stated purpose is to tell the reader what already exists so they don't rebuild it), are false. A developer reading this spec forms exactly one interpretation — "this exists, I should call it" — and that interpretation is wrong. This is worse than an ambiguous requirement, which at least prompts a clarifying question; a confidently false one prompts wasted implementation work before the gap is even noticed. This pattern recurs twice, cited across four locations (§7.1, Appendix C, §5.7, Appendix B), which is why this lands in the lower-middle of the scale rather than as an isolated deduction.

### D5 — Structural Integrity: 6/12 (50%) — FAIL

IDs are consistent and unique throughout (FR-NNN/AC-NNN); terminology is disciplined and reinforced by a formal Glossary (Appendix A); requirements are largely atomic (FR-012's CLI+panel dual-trigger is a minor, acceptable exception since it describes one behavior — idempotent marking — via two entry points).

The dimension fails on the "non-contradictory" and "implementation freedom" sub-criteria. Naming an existing pattern to reuse is normally *good* structural practice — it reduces duplication risk. But here it's inverted: the spec asserts as fact that specific reusable code exists (`round_to_half_hour`, the `settings-dialog` shell) when it does not. This is a direct, unresolved contradiction between the spec's stated technical reality and the actual state of the codebase it's additive to — nowhere in the document is this hedged, caveated, or flagged as "verify before use." Per the skill's own implementation-leak test ("if we used different technology to achieve this, would the requirement still be satisfied?"), these aren't ordinary implementation leaks (naming a real existing utility is fine); they're fabricated leaks that will only be discovered during implementation, at which point the developer must either re-interview the author or make an undocumented unilateral decision — precisely the failure mode D5 exists to catch.

---

## Buildability Scan (all 16 FRs)

| FR | Class | Note |
|---|---|---|
| FR-001 | B | Clear grouping key with explicit fallback chain |
| FR-002 | B | "Round to nearest 0.5h" is buildable text; the *associated* §7.1 constraint claiming reusable code is a separate, severe defect (see SHIP-BLOCKER 1) — does not make the FR itself unbuildable |
| FR-003 | B | Explicit 3-level fallback (repo → project_path basename → `—`) |
| FR-004 | B | |
| FR-005 | B | Exact output format given with examples |
| FR-006 | B | |
| FR-007 | B | |
| FR-008 | **D** | Field-shape ambiguity vs. §4.2 (day-level vs per-block `exported_at`, no aggregation rule) — see SHOULD-FIX / Improvement 2 |
| FR-009 | B | |
| FR-010 | B | |
| FR-011 | B | |
| FR-012 | B | Core behavior buildable; the AC-022 priority-tag mismatch is a D7 issue, not a buildability blocker |
| FR-013 | B | |
| FR-014 | B | No AC link, but covered by explicit Launch Criteria regression check (§6.1) |
| FR-015 | B | |
| FR-016 | **D** | Untestable architecture directive framed as a MUST; mismatched AC-006 link — see SHOULD-FIX |

**Result**: 14 B / 2 D / 0 U = 87.5% B, 12.5% D, 0% U.

---

## Structure Analysis Checklist

- [x] TL;DR present (Problem/Solution/Who/Non-goals/MVP cut line/Key decision)
- [x] Explicit in-scope (§2.1) and out-of-scope/non-goals (§2.2, plus TL;DR) — both directions stated
- [x] Roles table (1 role: Developer — appropriate for single-user local tool)
- [x] User journeys with Happy Path + Error Path + Edge Cases (all 3 journeys)
- [x] FR table with unique IDs, actor, priority, acceptance links (16 FRs)
- [x] Data requirements section (§4.2)
- [x] NFR section with 8 sub-categories (exceeds Medium-tier's 4-category minimum, meets Large-tier's 7+ bar)
- [x] Success/launch criteria with explicit go/no-go checklist (§6.1)
- [x] Constraints & Assumptions section, with assumptions individually confidence-rated and owned (§7.2)
- [x] Open Questions section (1 unresolved: Q-001; 1 resolved in-doc: Q-002)
- [x] Revision history
- [x] Glossary (Appendix A) — reinforces terminology consistency
- [ ] Constraints table free of factual errors — **fails**: 2 of 5 rows in §7.1 make unverifiable/false claims about existing code (`round_to_half_hour`) or are echoed falsely elsewhere (`settings-dialog`, via §5.7/Appendix B rather than §7.1 itself)

---

*End of report.*
