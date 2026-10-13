# Spec Evaluation Report: 001-web-setup-wizard/spec.md

## Summary
- **Total Score**: 98/120 (81.7%)
- **Grade**: B
- **Size Tier**: Medium (passed by caller)
- **Spec Format**: Hybrid (user-journey narratives + Given/When/Then AC tables + FR MUST/SHOULD/MAY table)
- **Buildability Ratio**: B:D:U = 85:15:0
- **TBD Count**: 0 (clean)
- **Happy-Path:Error Ratio**: ~1.5:1 (excellent)
- **Verdict**: A disciplined, well-structured spec that a developer can largely build from without interviewing the author — but two specific deferred-to-code references (validation rules and interval options) leave a material implementation gap, and Q-001 (shared vs. dedicated port) is an unresolved architectural fork that will block the first PR.

---

## Dimension Scores

| Dimension | Score | Max | Notes |
|-----------|-------|-----|-------|
| D1: Clarity & Unambiguity | 16 | 20 | FR-004 defers validation rules to `wizard.rs`; FR-007 references "existing interval options" without enumerating them |
| D2: Completeness | 15 | 20 | KNOWN_KEYS not enumerated; DB-init step mentioned in §6.1 go/no-go but has no FR; interval set not listed |
| D3: Testability & Verifiability | 15 | 18 | All 10 ACs are Given/When/Then observable behavior; AC-008 "files differ" criterion unspecified |
| D4: NFR Coverage & Quality | 13 | 16 | 8 NFR sub-sections; Performance/Security/Error-handling SMART; Reliability uses "best effort" hedge; Observability log spec underspecified |
| D5: Structural Integrity | 10 | 12 | FR-008 compounds 4 distinct behaviors in one row; NFR entries have no IDs |
| D6: Story/Requirement Form | 10 | 12 | Hybrid format applied consistently; actor column in every FR; minor `shall` usage in narrative sections |
| D7: Traceability & Prioritization | 11 | 14 | FR→AC links present; MVP cut line explicit; NFR entries unreferenced (no IDs); Q-001 unresolved before implementation |
| D8: Error & Edge Case Coverage | 8 | 10 | 5 of 6 applicable categories covered; boundary conditions for individual empty-credential states underspecified |
| **Total** | **98** | **120** | |

---

## Critical Issues

**1. Q-001 is an implementation-blocking fork with no resolution deadline that has passed.**
The question "Does the setup page share the daemon's existing `9323` listener/router, or get a dedicated loopback port?" directly affects FR-003 ("bound to loopback only"), the routing surface, and whether setup endpoints require auth isolation from review-UI endpoints. It is tagged "Before implementation start" but remains open. A developer cannot write the first endpoint without resolving this. It must be closed in the spec before issuing any work.

**2. FR-004 defers the validation specification to source code.**
"Same validation/probe rules as the TTY wizard" requires a developer to read `rust/crates/worklog-cli/src/wizard.rs` to know what validation logic to implement. If the wizard changes, the web endpoint silently diverges. This is the opposite of a buildability contract — the spec points to an implementation instead of specifying the behavior.

**3. KNOWN_KEYS and interval options are referenced but not enumerated.**
FR-004 references "every credential in the existing `KNOWN_KEYS` set" and FR-007 references "existing interval options" — neither set is listed in the spec. A developer building the setup page endpoints must grep the codebase to discover scope. For a feature where "full parity" is the go/no-go bar (§6.1), the parity set must be in the spec.

---

## Top 3 Improvements

**1. Enumerate KNOWN_KEYS and interval options inline, and specify validation rules per key.**

Replace the reference-to-code pattern in FR-004 with a table:

```
| Key name        | Validation rule (local)              | Probe (network)              |
|-----------------|--------------------------------------|------------------------------|
| GITHUB_TOKEN    | Prefix ghp_ or github_pat_ (min 40c)| GET /user → 200 or 401 error |
| JIRA_TOKEN      | Non-empty, min 8 chars               | GET /myself → 200 or 401     |
| ...             | ...                                  | ...                          |
```

And for FR-007, add: "The interval options are: 15m, 30m, 1h, 4h, 24h (matching the values accepted by `schedule::install`)."

This closes FR-004 and FR-007 as D-class requirements and eliminates the parity-gap risk in §6.1.

**2. Resolve Q-001 and fold the decision into FR-003 before the spec is shipped.**

Q-001 ("shared `9323` vs. dedicated port") is not a post-spec question — it is a pre-condition for implementing FR-003. The spec already states the key tradeoff (routing/auth surface). Make the call, record it as a constraint in §7.1, and remove Q-001 from §8. Example resolution text: "The setup endpoints SHALL be mounted on the existing `9323` router under the `/setup` path prefix. No additional port is opened. Setup-specific endpoint authorization relies solely on loopback isolation (per A-001)."

**3. Add NFR IDs and break FR-008 into atomic requirements.**

FR-008 currently reads: "MUST be able to install/refresh the Claude Code hook, install/refresh the bundled skill, and install daemon auto-start from the page." This is three independently failable, independently testable behaviors compounded into one requirement. Split into FR-008a (hook install/refresh), FR-008b (skill install/refresh), FR-008c (daemon auto-start install). This matters because AC-005 and AC-007 only cover hook and daemon-auto-start respectively — the skill install (the "files differ" state in AC-008) is SHOULD and risks being dropped without a dedicated MUST FR.

Separately, assign IDs to NFR rows (NFR-P1, NFR-S1, NFR-R1, etc.) so test plans and ACs can reference them — currently there is no way to write "this AC verifies NFR-5.1 row 2" in machine-traceable form.

---

## Detected Anti-Patterns

| Pattern | Severity | Evidence |
|---------|----------|----------|
| AP-13: Scope Creep Incubator (partial) | Medium | Q-001 is an architectural question disguised as an open question rather than a binding scope decision; leaving it open allows port-choice disagreement to surface mid-implementation |
| AP-02: Implementation Leak (minor) | Medium | FR-004 "same validation/probe rules as the TTY wizard" leaks a code file as the specification source; AC-006 "every 15m" is an example not a constraint, masking an unspecified interval set |
| AP-09: Untestable Criterion (minor) | Low | AC-008 "files differ — re-install overwrites state before any write" — the spec does not define which files are compared or by what method (hash, mtime, content?), making a deterministic test impossible to write |

---

## Detailed Analysis

### D1: Clarity & Unambiguity — 16/20

**Evidence driving the score:**

FR-004: *"MUST be able to enter, live-validate, and save every credential in the existing `KNOWN_KEYS` set from the page, with the same validation/probe rules as the TTY wizard"* — "same as the TTY wizard" is not a specification. A developer who has never seen `wizard.rs` cannot implement this without reading it. This is a D-class requirement.

FR-007: *"MUST be able to set or disable the scheduled-collection interval from the page using the existing interval options"* — which options? The spec says "every 15m" in AC-006 as an example but never states the option set. D-class.

Strong evidence (these work): AC-001 through AC-007 are precise and unambiguous. NFR 5.1 gives numeric thresholds with measurement methods. NFR 5.4 table names actor-visible behavior, system behavior, and recovery path per error condition.

**Fix**: Add an inline KNOWN_KEYS credential table and an interval-options enumeration (see Top 3 #1).

---

### D2: Completeness — 15/20

**Evidence driving the score:**

§6.1 go/no-go includes: *"Full parity: every TTY-wizard step (DB init, preflight, all KNOWN_KEYS credentials, estimator, schedule, hook, skill, daemon auto-start) is completable from the browser."* — "DB init" appears in the launch criteria but has no FR in §4.1. FR-009 covers preflight (docker/claude/git checks), but database initialization is not explicitly required. This is a named-feature-without-requirements gap for the go/no-go bar.

FR-007 / FR-004 reference scope that is not stated in the spec (see D1 analysis above).

Credential clearing: a credential that is already set but needs to be replaced with a different value is covered (FR-011 "re-run renders progress without destructive re-prompting"), but clearing/removing a wrongly-set secret (setting it back to unset) has no FR. The non-goals say "no data editing" — but credential clearing during initial setup is arguably within setup scope, not data editing. This boundary is ambiguous.

**Fix**: Add an FR for DB-init step state display/execution. Enumerate KNOWN_KEYS in the spec (see Top 3 #1). Explicitly address credential-clear semantics (either add an FR or add "credential deletion" to the non-goals).

---

### D7: Traceability & Prioritization — 11/14

**Evidence driving the score:**

Positive: FR table has an Acceptance Link column. ACs have explicit MUST/SHOULD priority. MVP cut line is stated: *"Everything in Section 4 tagged MUST ships for v1."* Journey priorities (P1/P2) are present.

Gap 1: NFR sub-sections 5.1–5.8 have no requirement IDs. They cannot be referenced from ACs or test-plan entries. FR-012 mentions "FR-012 test" in §6.1 but the NFR entries have no equivalent handle.

Gap 2: Q-001 is listed with deadline "Before implementation start" — this is the evaluation date (2026-06-08), meaning the deadline has passed and the question is still open. A developer starting implementation today hits this fork immediately.

**Fix**: Assign NFR IDs (NFR-P1/P2, NFR-S1, etc.). Resolve Q-001 before issuing any implementation work.
