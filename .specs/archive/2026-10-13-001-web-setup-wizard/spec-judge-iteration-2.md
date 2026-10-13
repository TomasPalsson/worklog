# Spec Evaluation Report: Browser-Based Setup Wizard
**File**: `.specs/001-web-setup-wizard/spec.md` (v1.1)

## Summary

- **Total Score**: 103/120 (85.8%)
- **Grade**: B
- **Size Tier**: Medium (passed — author-declared)
- **Spec Format**: Hybrid (journey-based sections with Given/When/Then AC tables + IEEE-style FR-NNN tabular requirements, consistently applied throughout)
- **Buildability Ratio**: B:D:U = 42:5:0 (89%:11%:0%)
- **TBD Count**: 0 (clean)
- **Happy-Path:Error Ratio**: ~3:1 (good)
- **Verdict**: A well-structured, zero-TBD spec that a developer can largely build from without interviewing the author — the §4.1.1 KNOWN_KEYS table and AC-008's byte-for-byte comparison pin are exemplary specificity — but two requirements (FR-009, FR-008c) leave their response schemas unspecified, and the preflight failure state is entirely unspecced, creating implementation risk in those corners.

---

## Dimension Scores

| Dimension | Score | Max | Notes |
|-----------|-------|-----|-------|
| D1: Clarity & Unambiguity | 16 | 20 | Zero fog-of-adjectives; tight modal discipline; two FRs with unspecified response shapes (FR-009 "results", FR-008c "platform state") |
| D2: Completeness | 17 | 20 | Zero TBDs, explicit non-goals, all features have FRs; one gap: preflight rendering content unspecified |
| D3: Testability & Verifiability | 14 | 18 | 10 of 11 ACs are directly testable; FR-009 fails the write-a-test check; NFR-P1 relies on manual timing |
| D4: NFR Coverage & Quality | 14 | 16 | 8 NFR categories present (exceeds Medium minimum of 4); most SMART; NFR-O1 missing log format/level |
| D5: Structural Integrity | 10 | 12 | FR-001 and FR-013 are compound (3 behaviors each); no contradictions; no terminology drift; no implementation leakage beyond legitimate constraints |
| D6: Story/Requirement Form | 11 | 12 | Consistently applied hybrid format; correct MUST/SHOULD/MAY discipline; actors named; EARS-equivalent AC form throughout |
| D7: Traceability & Prioritization | 13 | 14 | Unique IDs, per-requirement priorities, explicit MVP cut line; goals→requirements linkage is informal (no traceability matrix) |
| D8: Error & Edge Case Coverage | 8 | 10 | Good coverage of 4/6 categories; missing: preflight failure state (what does the page show when docker/claude/git is absent?); resource exhaustion and concurrent access omitted with appropriate rationale |
| **Total** | **103** | **120** | |

---

## Critical Issues

**1. FR-009 is untestable and has no AC.** "The daemon MUST run and return the docker/claude/git preflight results to the page" specifies neither the response schema (pass/fail? version string? error message? per-tool struct?) nor what the owner sees on the page. No AC covers the preflight section. A developer will invent the schema; a QA engineer cannot write a test. This is the largest single gap.

**2. Preflight failure state is unspecified.** There is no requirement for what the page shows when docker, claude, or git is not found. Journey 1's happy path lists "preflight results" as step 3 but has no error path for preflight. NFR-E1–E5 covers credentials and installs but not tool-detection failures. The owner could encounter a blocked setup with no specified guidance.

**3. FR-008c "platform state" is undefined.** "The daemon performs the privileged write and returns installed/platform state" — "platform state" is not defined anywhere. A developer will choose their own representation; a QA engineer cannot verify the correct value without a definition.

---

## Top 3 Improvements

**1. Define the preflight response schema and add a preflight error AC.**

FR-009 currently reads: "MUST run and return the docker/claude/git preflight results to the page." Replace it with a schema-specifying requirement and add a corresponding AC:

```
FR-009  | Daemon | MUST run the docker/claude/git preflight checks and return, per
        |        | tool: { tool: "docker"|"claude"|"git", found: bool,
        |        | version: Option<String>, error: Option<String> }. The page MUST
        |        | render each tool as a labeled row: found → green checkmark +
        |        | version string; not found → red X + error string.    | MUST | AC-012

AC-012  | Preflight, docker not installed | Owner opens setup page
        | | The docker row shows a red X with the system's error string
        |   (e.g. "docker: command not found"); the credentials and installs
        |   sections remain usable | MUST
```

This fixes FR-009's testability failure and closes the preflight error-path gap in both D1 and D8 simultaneously.

**2. Define "platform state" in FR-008c and resolve FR-001/FR-013 compounding.**

FR-008c reads: "returns installed/platform state." Define the return type: `{ installed: bool, platform: "launchd"|"systemd"|"unsupported", service_name: Option<String> }`. Add this to §4.2 Data Requirements or inline in FR-008c.

For FR-001 ("start daemon, open browser, print URL") and FR-013 ("scaffold home, init/migrate DB, report state"), split each into atomic FRs so each can be independently verified. Example:

- FR-001a: `worklog setup` with no flag MUST start the daemon if not running, waiting up to 5 s for it to bind the loopback port before opening the browser.
- FR-001b: The daemon MUST print the loopback setup URL to the terminal immediately after binding, regardless of whether the browser-open call succeeds.
- FR-001c: `worklog setup` with no flag MUST open the loopback setup URL in the OS default browser after the daemon is ready.

This removes the two D5 compound-requirement flags and makes each step independently testable.

**3. Add a SMART log-format spec to NFR-O1 and an automated measurement method to NFR-P1.**

NFR-O1 states: "The daemon MUST log each setup action attempted and its outcome." Missing: log level, format, and destination. Add: "Each setup action MUST be logged at INFO level to the daemon's existing log path (`~/.local/share/worklog/daemon.log` or equivalent) as a single-line JSON object with fields: `{ ts, action, key_name_only, outcome, error? }`. Secret values MUST NOT appear; key names MUST appear." This makes NFR-O1 verifiable by log inspection in tests.

NFR-P1 measurement method is "Manual timing on the author's machine." Replace with an automated test assertion: "Measured by a test that spawns a pre-running daemon and asserts the `/setup` route returns HTTP 200 with a non-empty body within 2 s, timed from the HTTP request." Manual observation is not a test; this closes the D3 borderline flag.

---

## Detected Anti-Patterns

- **AP-04 (Happy Path Spec) — Medium severity** — Triggered by FR-009 and Journey 1's preflight step having no error path. The owner-visible behavior when docker/claude/git is not found is entirely unspecified. The pattern is localized to the preflight section only; the rest of the spec has strong error coverage. Severity is Medium (not Critical) because it is a single omission rather than a systemic pattern.

No other anti-patterns detected. AP-01 (Fog of Adjectives), AP-06 (NFR Graveyard), AP-03 (Wishlist), AP-02 (Implementation Leak), AP-05 (Missing Actor), AP-15 (Tense Soup), AP-13 (Scope Creep Incubator) — all clear.

---

## Detailed Analysis

### D1: Clarity & Unambiguity — 16/20

**Evidence for the score**:

- §4.1.1 KNOWN_KEYS table is a high-water mark: per-key rendering mode, local validation rule, and network probe behavior enumerated for all 16 keys. No ambiguity.
- AC-008: "where 'differ' is determined by byte-for-byte content comparison of each file against its `include_str!` payload (not a version field), before any write" — eliminates any interpretive freedom in the "differs" state.
- FR-004b: "for an already-set credential the page MUST offer keep / replace / skip (default keep); for an unset credential it MUST offer set / skip (default skip)" — two state machines, explicit defaults, no ambiguity.

**Where points were lost**:

- FR-009: "return the docker/claude/git preflight results" — "results" has no defined shape. A developer reading this will invent a struct. This is a [D]-category requirement that forces an implementation decision the spec should have made.
- FR-008c: "returns installed/platform state" — "platform state" undefined. Does it mean the OS (macOS/Linux)? The service manager (launchd/systemd)? The service name? An enum? This will generate a clarification question or a wrong implementation.

**Fix**: Define both return types inline in the FR or in §4.2. The KNOWN_KEYS table is the template — the same level of specificity applied to FR-009 and FR-008c would close both gaps.

---

### D3: Testability & Verifiability — 14/18

**Evidence for the score**:

Write-a-test check on 5 requirements:
1. AC-001 — PASS: spawn process, assert URL printed, assert browser-open invoked.
2. AC-008 — PASS: inject modified skill file, assert page reports "files differ" state before any write.
3. AC-003 — PASS (minor): assert inline error rendered, assert keychain not written, assert other fields enabled.
4. FR-009 — FAIL: cannot write a test without knowing the response schema or what the page renders.
5. NFR-P1 — BORDERLINE: "manual timing on the author's machine" is not an automated test assertion.

**Where points were lost**: FR-009 is untestable. NFR-P1's measurement method is manual. These two items account for the 4-point gap from the maximum.

**Concrete fix for NFR-P1**: Change measurement method from "Manual timing on the author's machine" to "Integration test: pre-running daemon fixture; assert `GET /setup` returns HTTP 200 with non-empty body within 2 000 ms." This is automated, repeatable, and runs in CI.

---

### D8: Error & Edge Case Coverage — 8/10

**Evidence for the score**:

Good coverage found:
- Invalid input: AC-003, FR-005, NFR-E1 — inline error, value not saved, "save anyway" option.
- External dependency failure: NFR-E2 (network probe timeout), NFR-E4 (daemon unreachable).
- Security/auth failure: NFR-S4 (malicious local process — accepted risk, documented).
- Boundary/edge conditions: daemon already running, browser close mid-flow, skill files differ, `--cli` on headless box.

Gap — preflight failure category: The spec names docker/claude/git preflight as a required step (FR-009, Journey 1 step 3) but specifies no behavior when any tool is absent. There is no error path in Journey 1 for "docker not found," no NFR-E row for it, and no AC. A developer will invent the error display; a QA engineer cannot test it.

The two-point deduction reflects this as a single category omission (one of six) in a spec that otherwise covers error cases well.
