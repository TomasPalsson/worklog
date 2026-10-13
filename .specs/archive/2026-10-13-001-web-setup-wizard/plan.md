# Build Plan — Browser-Based Setup Wizard (from .specs/001-web-setup-wizard/spec.md)

Size tier: Medium · Slices: 8 · 8 AFK / 0 HITL

> **Why all-AFK is honest here, not inflation:** this spec is a *parity rebuild* of an
> existing TTY wizard. Every piece of privileged logic already exists in core modules
> (`secrets`, `schedule`, `hook`, `skill`, `daemon_service`, `wizard.rs::validate`); every
> response shape is defined (FR-008c, FR-009, FR-013); the full credential validation/probe
> table is enumerated (§4.1.1); byte-diff semantics are pinned (AC-008); and the page design
> is fixed by reference — "single scrollable form mirroring the TTY wizard's section order"
> (Appendix B, which *explicitly waives mockups*) + "follow the existing review-UI styling"
> (NFR-A4). There is no unmade visual/UX/architecture decision and no unresolved
> `[NEEDS CLARIFICATION]`. Q-001 is post-v1 and touches none of these slices. The web layer
> is a thin caller over existing seams (§7.1). That is what makes every slice agent-shippable.
> **Soft checkpoint:** if the author wants to eyeball the page before the install slices land,
> slice 01 (the first visual surface) is the natural place to pause — it is not a blocking
> decision, so it is not tagged HITL.

## Dependency graph

```
01 ──┬── 02
     ├── 03 ── 04
     ├── 05
     ├── 06
     ├── 07
     └── 08
```

Shallow fan-out from the walking skeleton; only 04 (estimator) sits behind 03 (it reuses the
credential save path). Acyclic; no orphan dependencies.

## Issues

### 01 — Owner runs `worklog setup` and a daemon-served setup page opens; `--cli` still runs the TTY wizard
- **Tag**: AFK
- **depends-on**: —  (walking skeleton)
- **Spec trace**: Journey 1 (happy path, steps 1–2), Journey 3 (happy path + edge); FR-001, FR-002, FR-003; NFR-R2, NFR-E4, NFR-E5
- **Seam**: served-setup surface + CLI routing — `/setup/*` mounted on the existing flat
  `daemon::router()` (one router over the unix socket + `127.0.0.1:9323`, §7.1); `worklog setup`
  flag dispatch in `worklog-cli`.
- **AFK justification (binding)**: Page layout is fully determined by Appendix B ("a single
  scrollable form mirroring the TTY wizard's section order", which explicitly waives mockups) and
  NFR-A4 ("follow the existing review-UI styling"); no author eyeball is required before dependent
  slices build on the shell. This issue is AFK — there is no unmade visual decision.
- **Acceptance criteria** (from spec):
  - [ ] AC-001 — Given a fresh install with no daemon running, When the Owner runs `worklog setup`, Then the daemon starts, the browser opens the loopback setup URL, and the URL is printed to the terminal.
  - [ ] AC-009 — Given a headless environment, When the Owner runs `worklog setup --cli`, Then the TTY wizard runs to completion with no browser launch and no dependency on the web setup endpoints.
  - [ ] AC-010 (SHOULD) — Given the browser fails to auto-open, When the Owner runs `worklog setup`, Then the CLI prints the loopback setup URL so the Owner can open or forward it manually.
  - [ ] NFR-E4 — Given the daemon becomes unreachable while the setup page is open, When the browser polls or the Owner submits a form, Then the page shows a connection-lost state (and the CLI path remains: rerun `worklog setup`, or use `--cli`).
  - [ ] Edge — Given the daemon is already running, When the Owner runs `worklog setup`, Then it reuses the running daemon rather than starting a second instance.
  - [ ] FR-003 — Given the setup routes are mounted, When a request arrives off-loopback, Then it is not served (the page is reachable only over the unix socket + `127.0.0.1:9323`).
- **Demoable result**: On a fresh box, `worklog setup` boots the daemon, opens `/setup` (a rendered shell) in the default browser, and prints the URL; `worklog setup --cli` runs today's TTY wizard unchanged. Headless onboarding is never broken.

### 02 — Setup page shows fresh-install bootstrap (home + DB) and preflight diagnostics on load
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 1 (happy path step 3; AC-011, AC-012 edge); FR-013, FR-009; NFR-P1
- **Seam**: read-only diagnostics — DB scaffold/migrate via existing home+SQLite init; preflight tool probes (`docker`/`claude`/`git`).
- **Acceptance criteria** (from spec):
  - [ ] AC-011 — Given a fresh install with no worklog home / DB yet, When the Owner runs `worklog setup`, Then the daemon scaffolds the home tree, initializes/migrates the SQLite DB, and the page shows the DB state (schema version + event/block counts).
  - [ ] AC-012 — Given a preflight tool (e.g. `docker`) is not installed, When the Owner runs `worklog setup`, Then the page shows that tool as `found: false` with its install hint, and the other setup steps remain usable.
  - [ ] FR-009 shape — Given the preflight runs, Then each tool returns `{ tool, found, version? (first line of `<tool> --version`), hint? (present when not found) }`.
  - [ ] NFR-P1 — Given the daemon is warm, When the page first renders, Then the `/setup` handler responds in < 2 s.
- **Demoable result**: A fresh box with no home dir → `worklog setup` creates the home tree, migrates the DB, and the page shows schema version + counts plus a docker/claude/git preflight panel with per-tool found/version/hint.

### 03 — Owner can enter, live-validate, and save any credential, with inline errors, save-anyway, and re-run state
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 1 (happy path step 4; error path; edge "re-running resumes"); FR-004, FR-004b, FR-005, FR-011 (credential portion), FR-012; §4.1.1 (full `KNOWN_KEYS` table); NFR-E1, NFR-E2, NFR-O1, NFR-P2, NFR-P3
- **Seam**: credential capture — shared `wizard.rs::validate()` + `secrets::*` keychain writes behind a thin `/setup` write endpoint; set/unset state read-back only (never plaintext).
- **Scope note (binding)**: the agent MUST implement every row in §4.1.1, not just GitHub. GitHub
  is the worked example in tests (richest: prefix check + network probe + save-anyway); every other
  `KNOWN_KEYS` row MUST also apply its distinct local-validation rule (or explicit `none`) and carry
  a test fixture exercising it.
- **Acceptance criteria** (from spec):
  - [ ] AC-002 — Given the setup page is open and a valid GitHub token is entered, When the Owner submits it, Then the daemon validates it, saves it to the keychain, and the page shows that credential as set.
  - [ ] AC-003 — Given a token failing validation, When the Owner submits it, Then the page shows the validation error inline, the value is NOT saved, and other steps remain usable.
  - [ ] AC-004 — Given one or more steps already applied, When the Owner re-runs `worklog setup`, Then the page renders those steps as already-done without re-prompting destructively.
  - [ ] FR-004 / §4.1.1 — Given any key in the `KNOWN_KEYS` parity set, When the Owner submits it, Then that row's local validation and network-probe rules apply (GitHub is the canonical case: `ghp_`/`github_pat_` prefix + `GET api.github.com/user` 5s bearer probe).
  - [ ] FR-005 / NFR-E2 — Given a credential fails validation or its network probe, When the Owner chooses "save anyway", Then the unvalidated value persists (probe failure is a warning, never a hard block).
  - [ ] FR-004b — Given an already-set credential the page offers keep / replace / skip (default keep); given an unset credential it offers set / skip (default skip).
  - [ ] FR-012 — Given any saved secret, When the page reads state, Then only set/unset is returned — no secret value is ever returned in plaintext.
  - [ ] NFR-O1 — Given any credential save action, When the daemon processes it, Then it emits exactly one log line (`info` on success, `warn` on validation/probe failure) carrying `action`, the `key` name, and `outcome`, and that line contains no secret value, token, or URL credential.
- **Demoable result**: The Owner pastes a GitHub token → it validates and saves and shows "set"; a malformed token → inline error, not saved, rest of the page still works; "save anyway" persists an unvalidated value; a re-run shows already-set credentials as done; no secret value is ever echoed back.
- **Out of scope**: estimator-specific keys (`litellm_*`, `worklog_estimator_provider`) are driven by slice 04's estimator step.

### 04 — Owner can choose the estimator provider (subprocess `claude -p` or LiteLLM)
- **Tag**: AFK
- **depends-on**: 03  (LiteLLM keys reuse the credential save path; provider persists via a secret key)
- **Spec trace**: Journey 1 (happy path step 5, estimator); FR-006; §4.1.1 (`litellm_*`, `worklog_estimator_provider` rows); Non-goal "resetting the estimator clears only `worklog_estimator_provider`"; NFR-E2
- **Seam**: estimator config — provider toggle persisted via existing secret key; LiteLLM trio captured through slice 03's save path with an optional `/health` probe.
- **Acceptance criteria** (from spec):
  - [ ] FR-006 (subprocess) — Given the estimator step, When the Owner selects subprocess `claude -p`, Then the persisted provider override (`worklog_estimator_provider`) is cleared.
  - [ ] FR-006 (LiteLLM) — Given the estimator step, When the Owner selects LiteLLM, Then `litellm_base_url` / `litellm_api_key` / `litellm_model` are captured (model defaults to `DEFAULT_LITELLM_MODEL`) and the provider is set to litellm.
  - [ ] §4.1.1 + NFR-E2 — Given LiteLLM is chosen, When the optional `GET <base_url>/health` probe fails, Then the page warns with "save anyway?" rather than hard-blocking.
- **Demoable result**: The Owner picks LiteLLM, enters base-url/key/model, and (optionally) the `/health` probe runs with a save-anyway fallback; picking subprocess instead resets the estimator to default by clearing only `worklog_estimator_provider`.

### 05 — Owner can set or disable the scheduled-collection interval
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 1 (happy path step 5, schedule); Journey 2 (happy path step 4); FR-007; FR-010 (non-blocking)
- **Seam**: schedule config — reuse existing `schedule::*` install/uninstall; no new scheduling mechanism.
- **Acceptance criteria** (from spec):
  - [ ] AC-006 — Given the schedule section with an interval chosen, When the Owner sets collection to "every 15m", Then the daemon installs the scheduler entry and the page reflects the chosen interval.
  - [ ] FR-007 — Given the schedule step, When the Owner chooses from {off, 5m, 15m (default), hourly, 4h, daily}, Then that `schedule::Interval` is applied (off disables collection).
- **Demoable result**: The Owner selects "every 15m" → the launchd/systemd scheduler entry is installed and the page shows the active interval; selecting "off" disables it.

### 06 — Owner can install/refresh the Claude Code hook from the page
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 2 (happy path steps 1–3); FR-008a, FR-011 (hook state); FR-010 (non-blocking)
- **Seam**: hook install — reuse existing `hook::install`; web layer is a thin caller returning state.
- **Acceptance criteria** (from spec):
  - [ ] AC-005 — Given the hook is not installed, When the Owner triggers hook install from the page, Then the daemon installs it and the page shows it installed with the event count.
  - [ ] Edge (FR-011) — Given the hook is already installed and up-to-date, When the Owner opens the step, Then the page offers refresh/re-install rather than a blind duplicate.
- **Demoable result**: The Owner clicks "install hook" → `hook::install` runs and the page shows the hook installed with its event count; a re-run offers refresh instead of duplicating.

### 07 — Owner can install/refresh the bundled skill, with byte-for-byte "files differ" detection
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 2 (happy path step 4, skill; edge "files differ"); FR-008b, FR-011 (skill state)
- **Seam**: skill install — reuse existing `skill::*`; up-to-date/differs determined by byte comparison against each `include_str!` payload.
- **Acceptance criteria** (from spec):
  - [ ] AC-008 — Given the skill is installed but ≥1 bundled file's on-disk bytes differ from the binary's embedded copy, When the Owner opens the skill step, Then the page shows the "files differ — re-install overwrites" state, where "differ" is determined by byte-for-byte content comparison of each file against its `include_str!` payload (not a version field), before any write.
  - [ ] FR-008b — Given the Owner installs/refreshes the skill, When the write completes, Then the daemon returns the file count and up-to-date state.
- **Demoable result**: The Owner opens the skill step; if any bundled file's on-disk bytes differ from the embedded copy the page shows "files differ — re-install overwrites"; installing/refreshing returns the file count and an up-to-date state.

### 08 — Owner can install daemon auto-start, with a non-blocking failure + retry
- **Tag**: AFK
- **depends-on**: 01
- **Spec trace**: Journey 2 (happy path step 4, daemon; error path "privileged write fails"); FR-008c, FR-010, FR-011 (daemon-service state); NFR-E3
- **Seam**: daemon-service install — reuse existing `daemon_service::*` privileged write (launchd/systemd); failure is isolated to this step.
- **Acceptance criteria** (from spec):
  - [ ] AC-007 — Given an unsupported OS or a denied write, When the Owner triggers daemon-auto-start install, Then the page shows that step as failed with a reason and a retry control, and remaining steps stay usable.
  - [ ] FR-008c shape — Given the install runs, Then the daemon returns `{ installed: bool, platform: "launchd" | "systemd" | "unsupported", unit_path?: string }`.
  - [ ] FR-010 — Given any step fails, When the failure is shown, Then it does not block other steps — each step fails independently and non-blockingly.
- **Demoable result**: The Owner clicks "install daemon auto-start"; on success the page shows it installed with the platform + unit path; on an unsupported OS or denied write the step shows failed-with-reason + a retry control while every other step stays completable.

## Coverage check

| Spec item | Slice |
|---|---|
| FR-001, FR-002, FR-003 | 01 |
| FR-013, FR-009 | 02 |
| FR-004, FR-004b, FR-005, FR-012 | 03 |
| FR-006 | 04 |
| FR-007 | 05 |
| FR-008a | 06 |
| FR-008b | 07 |
| FR-008c, FR-010 | 08 |
| FR-011 | 03 (credentials) + 06/07/08 (per-step state) |
| NFR-E4 (connection-lost) | 01 |
| NFR-O1 (setup-action logging) | 03 |
| AC-001, AC-009, AC-010 | 01 |
| AC-011, AC-012 | 02 |
| AC-002, AC-003, AC-004 | 03 |
| AC-005 | 06 |
| AC-006 | 05 |
| AC-007 | 08 |
| AC-008 | 07 |
| Journey 1 | 01,02,03,04,05 |
| Journey 2 | 06,07,08,05 |
| Journey 3 | 01 |

Every MUST acceptance criterion, every functional requirement, and all three journeys map to ≥1 slice.
