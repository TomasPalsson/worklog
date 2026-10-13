# 03 — Owner can enter, live-validate, and save any credential, with inline errors, save-anyway, and re-run state

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner pastes a credential (GitHub is the worked case) → it validates and saves to the keychain and shows "set"; a malformed value → inline error, not saved, the rest of the page still works; "save anyway" persists an unvalidated value; a re-run shows already-set credentials as done; no secret value is ever echoed back.

## Tag rationale
- **AFK** → No human decision required. The full `KNOWN_KEYS` parity set with per-key local-validation and network-probe rules is enumerated in §4.1.1; validation reuses `wizard.rs::validate()`. Deterministic acceptance criteria.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 1 (happy path step 4; error path "invalid credential"; edge "re-running resumes")
- Requirements: FR-004, FR-004b, FR-005, FR-011 (credential portion), FR-012, §4.1.1 (full `KNOWN_KEYS` table), NFR-E1, NFR-E2, NFR-O1, NFR-P2, NFR-P3
- Seam: credential capture — shared `wizard.rs::validate()` + `secrets::*` keychain writes behind a thin `/setup` write endpoint; set/unset state read-back only (never plaintext).

## Scope note (binding)
The agent MUST implement **every row in §4.1.1**, not just GitHub. GitHub is the worked example in tests (richest: prefix check + network probe + save-anyway); every other `KNOWN_KEYS` row MUST also apply its distinct local-validation rule (or explicit `none`) and carry a test fixture exercising it.

## Acceptance criteria  (AC-NNN rows quoted verbatim; FR-/NFR-derived criteria traced, not invented)
- [ ] **AC-002** — Given the setup page is open and a valid GitHub token is entered, When the Owner submits it, Then the daemon validates it, saves it to the keychain, and the page shows that credential as set.
- [ ] **AC-003** — Given a token failing validation, When the Owner submits it, Then the page shows the validation error inline, the value is NOT saved, and other steps remain usable.
- [ ] **AC-004** — Given one or more steps already applied, When the Owner re-runs `worklog setup`, Then the page renders those steps as already-done without re-prompting destructively.
- [ ] **FR-004 / §4.1.1** — Given any key in the `KNOWN_KEYS` parity set, When the Owner submits it, Then that row's local validation and network-probe rules apply (GitHub: `ghp_`/`github_pat_` prefix + `GET api.github.com/user` 5 s bearer probe → 2xx ok, non-2xx "GitHub returned HTTP n", network/build error → warning only).
- [ ] **FR-005 / NFR-E2** — Given a credential fails validation or its network probe, When the Owner chooses "save anyway", Then the unvalidated value persists (a failed probe is a warning that triggers "save anyway?", never a hard block).
- [ ] **FR-004b** — Given an already-set credential the page offers keep / replace / skip (default keep); given an unset credential it offers set / skip (default skip), mirroring the TTY wizard.
- [ ] **FR-012** — Given any saved secret, When the page reads state, Then only set/unset is returned — no secret value is ever returned in plaintext.
- [ ] **NFR-O1** — Given any credential save action, When the daemon processes it, Then it emits exactly one log line (`info` on success, `warn` on validation/probe failure) carrying `action`, the `key` name, and `outcome`, and that line contains no secret value, token, or URL credential.
- [ ] **NFR-P2** — Given a credential whose validation does not hit the network (length / prefix / format), When the Owner submits it, Then the daemon returns the validation result in < 500 ms at p95 (asserted by endpoint timing in tests).
- [ ] **NFR-P3** — Given a credential whose validation triggers a network probe (GitHub / LiteLLM), When the probe runs, Then it is bounded by the existing 5 s `reqwest` client timeout — a slower probe degrades to "save anyway?", it never hangs.

## Out of scope
- The estimator-specific keys (`litellm_*`, `worklog_estimator_provider`) are driven by slice 04's estimator step (which reuses this slice's save path).
