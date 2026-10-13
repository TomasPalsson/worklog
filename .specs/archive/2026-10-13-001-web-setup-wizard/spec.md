# Spec: Browser-Based Setup Wizard

> **One-sentence summary**: `worklog setup` opens a local browser page where the owner answers every onboarding question instead of a TTY prompt flow.

**Status**: Draft
**Size**: Medium
**Author**: Tomas (solo author/maintainer)
**Created**: 2026-06-08
**Last updated**: 2026-06-08
**Version**: 1.2

---

## TL;DR

> Read this block. If it answers your question, stop here.

**Problem**: Today onboarding runs entirely through a TTY wizard (`worklog setup`) with ~20 serial dialoguer prompts — credentials, estimator choice, schedule interval, and four local installs. It is slow to navigate, easy to mis-paste a token into, and unwelcoming to anyone who'd rather not drive a terminal step-by-step.

**Solution**: `worklog setup` launches the native Rust daemon (no Docker), opens a single-page setup UI in the default browser, and lets the owner complete every step there — secret capture with live validation, estimator/schedule selection, and triggering the Claude hook / skill / scheduled-collection / daemon-auto-start installs (the daemon performs the privileged writes). The existing TTY wizard remains reachable behind a `--cli` flag.

**Who it's for**: The single local machine owner running worklog on their own laptop (the author and any future self-hosting user). One person, hands-on, technical-enough to obtain API tokens but who prefers a form over 20 terminal prompts.

**Non-goals (v1)**:
- No remote or multi-user access — the page is bound to loopback only, has no accounts, and is never hosted for other people.
- No interactive OAuth broker — Google client-id/secret/refresh-token are pasted as text; the page runs no "Sign in with Google" redirect dance.
- No data editing — setup configures credentials and installs only; editing blocks/events stays the review UI's job.
- No secret read-back — saved secrets show as set/unset only, never rendered in plaintext or exportable.
- No credential deletion — setup captures and replaces secrets but does not clear an already-stored one (the lone exception is resetting the estimator to the subprocess default, which clears only `worklog_estimator_provider`); removing other secrets stays a CLI concern.

**MVP cut line**: Everything in Section 4 tagged `MUST` ships for v1. The cut line is **full parity with the TTY wizard**: every step the TTY wizard performs MUST be completable from the browser. `SHOULD` items are v1.1 candidates. `MAY` items are backlog.

**Key decision**: The setup page is served by the native Rust daemon — mounted as `/setup/*` on its existing loopback router (one flat axum router over the unix socket + `127.0.0.1:9323`), NOT by the dockerized Next.js review UI — because setup is what configures Docker, so it cannot depend on Docker being up.

---

## 1. Context

### 1.1 Problem Statement

A new worklog install requires running `worklog setup`, a terminal wizard that walks the owner through database init, a docker/claude/git preflight, capture of 10+ secrets, estimator-provider choice, scheduled-collection interval, and four local-machine installs (Claude Code hook, bundled skill, scheduled collection, daemon auto-start). The flow is correct but tedious and terminal-bound: long secrets are pasted blind into password prompts, there is no overview of remaining steps, and re-running to fix one value means stepping past everything else.

**Current workaround**: Owners run the TTY `worklog setup` and tab through every prompt, or set secrets one at a time via `worklog secret` and hand-install each piece (`worklog hook install`, `worklog skill install`, etc.).

**Business rationale**: This is the first-run experience. A friendlier setup lowers the effort of a clean install or a re-configuration and reduces mis-pasted-credential support friction for the owner.

### 1.2 User Roles

> Every requirement must name one of these roles.

| Role | Description | Volume (approx.) | Key characteristic |
|------|-------------|------------------|--------------------|
| Owner | The person installing/configuring worklog on their own machine; obtains tokens and decides installs | 1 per machine | Technical enough to get API tokens; prefers a form to 20 TTY prompts |
| Daemon | The native Rust process serving the setup UI and executing privileged writes (keychain, launchd/systemd, hook/skill files) | 1 per machine | Trusted local actor; runs as the owner's user |

**Primary actor**: Owner.

**Hidden stakeholders**: None — single-user, single-machine. The dockerized review UI is a downstream consumer of the configuration setup produces, but it does not interact with the setup flow.

### 1.3 Prior Art & Alternatives Considered

| Option | Status | Why rejected / why not this |
|--------|--------|-----------------------------|
| Keep TTY wizard only (`wizard.rs`) | Retained as `--cli` fallback | Works for headless/SSH but is the friction this spec removes for the default path |
| Setup as a route inside the dockerized Next.js review UI | Rejected | Requires Docker already running; setup is what configures Docker — circular |
| Setup page served by the native Rust daemon on loopback | Selected | Daemon already binds `127.0.0.1:9323` and performs privileged writes; needs no Docker |

---

## 2. Scope

### 2.1 In Scope

- `worklog setup` (no flag) starts the daemon if needed and opens the setup page in the default browser; `worklog setup --cli` runs today's TTY wizard unchanged.
- A single-page setup UI served by the daemon over loopback presenting: preflight diagnostics, secret capture with live validation, estimator-provider choice, scheduled-collection interval, and the four local installs.
- Daemon HTTP endpoints (loopback only) that read current setup state and apply each change: write a secret (after validation), set estimator provider, set/clear schedule, install/refresh hook, install/refresh skill, install daemon auto-start, run preflight.
- Per-step success/failure feedback in the browser mirroring today's "validated / save anyway / skip" semantics.
- Handoff to the review UI after setup (existing `worklog serve` behaviour), unchanged.

### 2.2 Out of Scope (Non-Goals)

> Binding. If asked for one of these, point here.

- **Remote / multi-user access**: loopback bind only; no auth accounts, no TLS, no hosting for others. Single deferred concern, not a v2 promise.
- **Interactive OAuth broker flow**: no Google sign-in redirect; OAuth values are pasted. Separate initiative if ever wanted.
- **Editing collected data**: blocks/events editing stays in the review UI.
- **Secret read-back / export**: saved secret values are never returned to the browser in plaintext.
- **Credential deletion**: setup does not clear an already-stored secret (only the estimator reset clears `worklog_estimator_provider`); use the CLI to remove a secret.

### 2.3 Adjacent Systems

| System | Relationship | Constraint |
|--------|-------------|------------|
| OS keychain (`secrets::*`) | Writes to | Existing `KNOWN_KEYS` set and `SERVICE` name must stay the store of record |
| Scheduler (launchd/systemd, `schedule::*`) | Writes to | Reuse existing install/uninstall; no new scheduling mechanism |
| Claude Code hook + bundled skill (`hook::*`, `skill::*`) | Writes to | Reuse existing install paths and idempotency rules |
| Dockerized review UI | Hands off to | Setup must complete without Docker; review UI consumes the resulting config |
| TTY wizard (`wizard.rs`) | Shares logic with | Validation/probe/install logic should be shared, not duplicated, to avoid drift |

---

## 3. User Journeys

> Every journey specifies happy path, primary error path, and at least one edge case.

### Journey 1 — First-run setup in the browser (Priority: P1)

**Actor**: Owner
**Starting condition**: Fresh install; no secrets set; daemon not necessarily running.
**Goal**: Complete all configuration from the browser and reach a working install.

**Happy path**:
1. Owner runs `worklog setup`.
2. Daemon starts (if not already running) and binds its loopback port; the CLI opens the setup URL in the default browser and prints the URL to the terminal.
3. The page shows preflight results, a credentials section, estimator/schedule choices, and the four installs, each with current state.
4. Owner fills credentials; each is validated on submit and saved to the keychain on success.
5. Owner sets estimator provider and schedule interval, and triggers the hook/skill/daemon/schedule installs.
6. Page shows every step as complete; Owner proceeds to the review UI.

**Error path — invalid credential**:
1. Owner submits a token that fails validation (e.g. GitHub token without `ghp_`/`github_pat_` prefix, or a 401 from GitHub).
2. The daemon returns the validation error; the page shows it inline next to that field and does not save the value.
3. Owner corrects it and resubmits, or chooses "save anyway" to persist the unvalidated value (mirrors TTY semantics).

**Edge cases**:
- Daemon already running: setup reuses it rather than starting a second instance.
- Owner closes the browser mid-flow: every already-applied step persists; re-running `worklog setup` resumes with those steps shown as done.
- `--no-serve`/headless intent: `worklog setup --cli` bypasses the browser entirely.

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-001 | Fresh install, no daemon running | Owner runs `worklog setup` | Daemon starts, browser opens the loopback setup URL, and the URL is printed to the terminal | MUST |
| AC-002 | Setup page open, valid GitHub token entered | Owner submits the token | Daemon validates it, saves it to the keychain, and the page shows that credential as set | MUST |
| AC-003 | Setup page open, token failing validation | Owner submits the token | The page shows the validation error inline, the value is NOT saved, and other steps remain usable | MUST |
| AC-004 | One or more steps already applied | Owner re-runs `worklog setup` | The page renders those steps as already-done without re-prompting destructively | MUST |
| AC-011 | Fresh install, no worklog home / DB yet | Owner runs `worklog setup` | The daemon scaffolds the home tree, initializes/migrates the SQLite DB, and the page shows the DB state (schema version + event/block counts) | MUST |
| AC-012 | A preflight tool (e.g. `docker`) is not installed | Owner runs `worklog setup` | The page shows that tool as `found: false` with its install hint, and the other setup steps remain usable | MUST |

---

### Journey 2 — Triggering local installs from the browser (Priority: P1)

**Actor**: Owner
**Starting condition**: Setup page open; hook/skill/daemon/schedule not yet installed.
**Goal**: Install the Claude hook, bundled skill, scheduled collection, and daemon auto-start from the page.

**Happy path**:
1. Owner clicks install for the Claude Code hook.
2. Daemon runs the existing `hook::install`, returns the new state.
3. Page shows the hook as installed with its event count.
4. Owner repeats for skill, scheduled collection (choosing an interval), and daemon auto-start.

**Error path — privileged write fails**:
1. Owner triggers daemon-auto-start install on an unsupported OS or where the launchd/systemd write is denied.
2. Daemon returns the failure with a human-readable reason.
3. Page marks that one step failed with a retry control; all other steps remain completable (non-blocking).

**Edge cases**:
- Step already installed and up-to-date: page offers refresh/re-install rather than a blind duplicate.
- Skill installed but bundled files differ: page surfaces the "files differ — re-install overwrites" state, matching TTY behaviour.

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-005 | Hook not installed | Owner triggers hook install from the page | Daemon installs it and the page shows it installed with event count | MUST |
| AC-006 | Schedule section, interval chosen | Owner sets collection to "every 15m" | Daemon installs the scheduler entry and the page reflects the chosen interval | MUST |
| AC-007 | Unsupported OS or denied write | Owner triggers daemon-auto-start install | The page shows that step as failed with a reason and a retry control, and remaining steps stay usable | MUST |
| AC-008 | Skill installed but ≥1 bundled file's on-disk bytes differ from the binary's embedded copy | Owner opens the skill step | The page shows the "files differ — re-install overwrites" state, where "differ" is determined by byte-for-byte content comparison of each file against its `include_str!` payload (not a version field), before any write | MUST |

---

### Journey 3 — Headless / SSH fallback (Priority: P2)

**Actor**: Owner
**Starting condition**: No browser available (SSH session, CI, headless box).
**Goal**: Complete setup without a browser.

**Happy path**:
1. Owner runs `worklog setup --cli`.
2. The existing TTY wizard runs exactly as today.
3. Setup completes in the terminal.

**Error path — browser cannot open on default path**:
1. Owner runs `worklog setup` on a box with no oping-capable browser.
2. The daemon still serves the page and the CLI prints the loopback URL the owner can open elsewhere (e.g. via SSH port-forward of their own making) or rerun with `--cli`.

**Edge cases**:
- `--cli` must not require the daemon's web setup endpoints to exist/run.

**Acceptance criteria**:

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| AC-009 | Headless environment | Owner runs `worklog setup --cli` | The TTY wizard runs to completion with no browser launch and no dependency on the web setup endpoints | MUST |
| AC-010 | Browser fails to auto-open | Owner runs `worklog setup` | The CLI prints the loopback setup URL so the owner can open or forward it manually | SHOULD |

---

## 4. Functional Requirements

> Every requirement: named actor, observable behavior, testable outcome. MUST/SHOULD/MAY discipline.

### 4.1 Core Requirements

| ID | Actor | Requirement | Priority | Acceptance Link |
|----|-------|-------------|----------|-----------------|
| FR-001 | Owner | `worklog setup` with no flag MUST start the daemon if needed, open the setup page in the default browser, and print the loopback URL to the terminal | MUST | AC-001, AC-010 |
| FR-002 | Owner | `worklog setup --cli` MUST run the existing TTY wizard and MUST NOT require the web setup endpoints | MUST | AC-009 |
| FR-003 | Daemon | MUST serve the setup UI and its endpoints on the existing daemon router under a `/setup` path prefix, reachable only over the daemon's loopback transports (unix socket + `127.0.0.1:9323`), never on a non-loopback interface (see §7.1) | MUST | AC-001 |
| FR-013 | Daemon | MUST scaffold the worklog home directory tree and initialize/migrate the SQLite database on setup start, and report the resulting state (schema version, event count, block count) to the page | MUST | AC-011 |
| FR-004 | Owner | MUST be able to enter, live-validate, and save every credential in the `KNOWN_KEYS` parity set enumerated in §4.1.1, applying that table's per-key validation and network-probe rules | MUST | AC-002, AC-003 |
| FR-004b | Owner | For an already-set credential the page MUST offer keep / replace / skip (default keep); for an unset credential it MUST offer set / skip (default skip), mirroring the TTY wizard | MUST | AC-004 |
| FR-005 | Daemon | MUST reject saving a credential that fails validation unless the Owner explicitly chooses "save anyway", mirroring TTY semantics | MUST | AC-003 |
| FR-006 | Owner | MUST be able to choose the estimator provider — subprocess `claude -p` (clears the persisted provider override) or LiteLLM (captures `litellm_base_url`/`litellm_api_key`/`litellm_model` with an optional `/health` probe) — from the page | MUST | AC-002 |
| FR-007 | Owner | MUST be able to set or disable the scheduled-collection interval from the page, choosing from: off, every 5m, every 15m (default), hourly, every 4h, daily (the `schedule::Interval` set) | MUST | AC-006 |
| FR-008a | Owner | MUST be able to install/refresh the Claude Code hook from the page; the daemon performs the write and returns the installed event count | MUST | AC-005 |
| FR-008b | Owner | MUST be able to install/refresh the bundled Claude Code skill from the page; the daemon performs the write and returns the file count and up-to-date state | MUST | AC-008 |
| FR-008c | Owner | MUST be able to install daemon auto-start (the login service) from the page; the daemon performs the privileged write and returns `{ installed: bool, platform: "launchd" \| "systemd" \| "unsupported", unit_path?: string }` | MUST | AC-007 |
| FR-009 | Daemon | MUST run the docker/claude/git preflight and return, per tool, `{ tool: string, found: bool, version?: string (first line of `<tool> --version`), hint?: string (install hint, present when not found) }` to the page | MUST | AC-001, AC-012 |
| FR-010 | System | Each step MUST fail independently and non-blockingly — a failed step shows an error and retry while other steps remain completable | MUST | AC-007 |
| FR-011 | Daemon | MUST report each step's current state (credential set/unset, hook installed + event count, skill up-to-date/differs/absent, schedule interval, daemon-service installed/not) so a re-run renders progress without destructive re-prompting | MUST | AC-004, AC-008 |
| FR-012 | Daemon | MUST NOT return any saved secret value in plaintext to the browser; secret state is set/unset only | MUST | — |

### 4.1.1 Credential Parity Set (`KNOWN_KEYS`)

> The complete, ordered set FR-004 refers to. "Rendering" is masked unless the key is `_url`-suffixed or in the public allowlist `{jira_email, google_client_id, jira_base_url}`. Validation/probe rules mirror `wizard.rs::validate()`; a failed network probe is a warning that triggers "save anyway?", never a hard block (FR-005).

| Key | Rendering | Local validation | Network probe |
|-----|-----------|------------------|---------------|
| `jira_email` | plaintext | must contain `@` | none |
| `jira_api_token` | masked | min 10 chars | none |
| `jira_base_url` | plaintext | min 3 chars | none |
| `jira_account_id` | masked | none | none |
| `jira_account_field_id` | masked | none — config field, not a credential | none |
| `tempo_api_token` | masked | min 10 chars | none |
| `github_token` | masked | prefix `ghp_` or `github_pat_` | `GET api.github.com/user` (bearer, 5s); 2xx → ok, non-2xx → "GitHub returned HTTP n", network/build error → warning only |
| `github_user` | masked | none | none |
| `google_client_id` | plaintext | min 3 chars | none |
| `google_client_secret` | masked | min 3 chars | none |
| `google_refresh_token` | masked | min 3 chars | none |
| `anthropic_api_key` | masked | prefix `sk-ant-`, then min 30 chars | none |
| `litellm_base_url` | plaintext | none | optional `GET <base_url>/health` (in the estimator step) → "save anyway?" on failure |
| `litellm_api_key` | masked | none (empty allowed for unauthed proxy) | none |
| `litellm_model` | masked | none (defaults to `DEFAULT_LITELLM_MODEL`) | none |
| `worklog_estimator_provider` | set via the estimator step, not free-text | none | none |

### 4.2 Data Requirements

> Logical entities, not schemas.

| Entity | Description | Key attributes (logical) | Relationships |
|--------|-------------|--------------------------|---------------|
| Credential | One configurable secret/value the owner supplies | key (from KNOWN_KEYS), is-set flag, validation status | Stored in OS keychain; never read back to browser |
| Setup step state | Current state of an install/config step | step id, status (done/not/failed/differs), human detail | Reflects keychain / scheduler / hook / skill / daemon-service state |
| Estimator config | The chosen estimation backend | provider (subprocess/litellm), litellm base-url/model (key is a Credential) | Provider persisted via existing secret key |

**Data retention**: Credentials persist in the OS keychain until the owner overwrites or deletes them (no change to today's lifecycle). Setup-step state is derived/transient, computed on each page load.

**Data sensitivity**: Credentials are secrets — written to the keychain, transmitted only over the loopback HTTP connection during entry, and never returned to the browser thereafter.

---

## 5. Non-Functional Requirements

### 5.1 Performance

| ID | Metric | Target | Condition | Measurement method |
|----|--------|--------|-----------|-------------------|
| NFR-P1 | Setup page first render | < 2 s after browser open | Daemon already running (warm), single local user | Automated test asserting the `/setup` handler responds in < 2 s with the daemon warm (CI-repeatable) |
| NFR-P2 | Credential validation round-trip (local checks) | < 500 ms at p95 | Validators that don't hit the network (length/prefix/format) | Endpoint timing in tests |
| NFR-P3 | Credential validation round-trip (network probe) | Bounded by existing 5 s client timeout | GitHub/LiteLLM live probe | Reuse existing 5 s `reqwest` timeout |

**Performance budget decision**: A network-probe validation slower than the existing 5 s timeout is a known limitation (it degrades to "save anyway?"), not a defect. A local-only validation exceeding 500 ms p95 is a defect.

### 5.2 Security

**NFR-S1 — Authentication**: None beyond loopback isolation — the page is served only on the daemon's loopback transports (unix socket + `127.0.0.1:9323`) and is not reachable off the machine (per FR-003 and the owner's chosen posture). No accounts, no tokens on the URL.

**NFR-S2 — Authorization**: Single role. The Owner (any process able to reach loopback on this machine) may perform every setup action; there is no privilege separation within the page.

| Action | Allowed roles | Denied behavior |
|--------|---------------|-----------------|
| Read setup-step state | Owner (loopback) | Non-loopback requests are not served (no listener off-loopback) |
| Write credential / trigger install | Owner (loopback) | Non-loopback requests are not served |

**NFR-S3 — Data protection**: Credentials travel only over the loopback connection during entry and are stored in the OS keychain. No secret value is ever returned to the browser (FR-012). Nothing is written to logs in plaintext.

**NFR-S4 — Threat surface**: Highest-risk vector is a malicious local process on the same machine reaching the loopback setup endpoints to write credentials or trigger installs. This is an accepted risk for v1 per the owner's "localhost-bound only" decision (A-001); a future one-time-token guard is noted as a backlog mitigation, not a v1 requirement.

### 5.3 Reliability & Availability

**NFR-R1 — Uptime target**: No formal SLA — the setup surface runs only while the owner is configuring; best effort.
**NFR-R2 — Graceful degradation**: If the web setup endpoints are unavailable, `worklog setup --cli` MUST still complete a full setup. The browser path failing MUST NOT block the CLI fallback.
**NFR-R3 — Recovery behavior**: All applied steps persist (keychain/scheduler/hook/skill/daemon-service). A crash or browser close loses no completed step; re-running resumes from current state (FR-011).

### 5.4 Error Handling

| ID | Error condition | Actor-visible behavior | System behavior | Recovery path |
|----|----------------|----------------------|-----------------|---------------|
| NFR-E1 | Credential fails validation | Inline error next to the field; value not saved | Daemon returns the existing validator error string | Owner corrects value or chooses "save anyway" |
| NFR-E2 | Network probe unreachable (GitHub/LiteLLM) | Warning that the probe failed, with "save anyway?" | Probe times out at 5 s; treated as warning, not hard fail | Owner saves anyway or skips |
| NFR-E3 | Privileged install write denied / unsupported OS | That step marked failed with a reason + retry | Daemon returns the failure; other steps untouched | Owner retries or installs later via CLI (`worklog <step> install`) |
| NFR-E4 | Daemon not reachable from the page | Page shows a connection-lost state | CLI prints the URL; owner can rerun `worklog setup` | Rerun, or use `worklog setup --cli` |
| NFR-E5 | Browser cannot auto-open | CLI prints the loopback URL | Daemon keeps serving | Owner opens URL manually or uses `--cli` |

### 5.5 Scalability

**NFR-SC1 — Concurrency target**: One owner, one browser tab. No multi-client requirement.
**NFR-SC2 — Growth assumption**: Single-user tool; no growth dimension. Re-evaluate only if worklog ever becomes multi-tenant (explicitly a non-goal).
**NFR-SC3 — Bottleneck hypothesis**: None at this scale; the only blocking calls are the bounded network probes.

### 5.6 Observability

**NFR-O1 — Required logging**: The daemon MUST log to its existing log destination (`~/.local/share/worklog/logs/daemon.{out,err}.log` / stderr), never stdout. Each setup action is one line carrying at least `action` (e.g. `secret_set`, `hook_install`, `preflight`), the `key`/`step` name (credential events log the key name only — never the value), and `outcome`. Level: `info` on success, `warn` on a validation or network-probe failure. No secret value, token, or URL credential is ever emitted.
**NFR-O2 — Required metrics**: None required for v1 (single-user, manual flow).
**NFR-O3 — Alerting threshold**: None — interactive flow, no unattended alerting.

### 5.7 Accessibility

**NFR-A1 — Standard**: No formal WCAG conformance required for v1 (single technical owner). The page SHOULD use standard form semantics so it is keyboard-operable.
**NFR-A2 — Screen reader**: Not a v1 requirement; native form controls give baseline support.
**NFR-A3 — Keyboard navigation**: All fields and install actions SHOULD be reachable and operable by keyboard.
**NFR-A4 — Contrast / sizing**: Follow the existing review-UI styling; no additional requirement.

### 5.8 Browser / Platform Support

> NFR-B1 — platform support matrix.

| Platform | Support level | Notes |
|----------|--------------|-------|
| Default browser on macOS (author's primary) | Full | Primary target |
| Default browser on Linux | Full | Daemon serving + scheduler via systemd |
| Windows | Not supported | Consistent with the existing scheduler/daemon-service support matrix |
| Headless / no browser | Degraded → `--cli` | Browser path not available; TTY wizard is the supported path |

---

## 6. Success Criteria

### 6.1 Launch Criteria (go/no-go)

- [ ] All `MUST` AC-NNN pass locally on macOS and Linux.
- [ ] Full parity: every TTY-wizard step (DB init, preflight, all `KNOWN_KEYS` credentials, estimator, schedule, hook, skill, daemon auto-start) is completable from the browser.
- [ ] Security: the listener is verified loopback-only; no secret value is ever returned to the browser (FR-012 test).
- [ ] Error states: each error condition in Section 5.4 produces the correct page/CLI behaviour, and `worklog setup --cli` completes a full setup with the web endpoints disabled.

### 6.2 Post-Launch Health Metrics

| Metric | Target | Measurement | Review trigger |
|--------|--------|-------------|----------------|
| Setup completion path | Browser path completes a fresh setup without falling back to `--cli` | Author dogfooding a clean install | If the author falls back to `--cli` to finish |
| Mis-pasted credential rate | Fewer re-entries than the TTY flow | Author observation | If browser entry is no better than TTY |

### 6.3 What "Failure" Looks Like

This ships, passes every AC, yet still fails if the browser flow ends up slower or more error-prone than the TTY wizard it replaces — for instance if validation feedback is laggy or step-state is confusing — so the owner habitually reaches for `--cli`. That would mean the convenience assumption (A-002) was wrong.

---

## 7. Constraints & Assumptions

### 7.1 Technical Constraints

| Constraint | Rationale | Impact on design |
|-----------|-----------|-----------------|
| Setup page served by the native Rust daemon, not Docker | Setup configures Docker; cannot depend on it | New loopback HTTP endpoints + static page in `worklog-core`/daemon |
| Setup endpoints mounted on the existing `daemon::router()` under a `/setup` path prefix — no new port (resolves former Q-001) | The daemon already serves one flat axum router identically over its unix socket and `127.0.0.1:9323` from a single shared `Arc<AppState>`; a dedicated port would add a second listener, `SocketAddr` arg, UI env var, and compose entry for zero isolation gain on an unauthenticated loopback daemon | Add `/setup/*` routes to the existing `router()`; instantly reachable by the dockerized UI at the current `WORKLOG_DAEMON_URL` base, no compose change |
| Reuse existing `secrets`, `schedule`, `hook`, `skill`, `daemon_service`, and validation/probe logic | Avoid drift between TTY and web flows | Shared core functions; the web layer is a thin caller, like the TTY wizard |
| Stdout is reserved (esp. for hook-run); daemon logs to stderr/log path | Project convention | Setup logging goes to the existing log path |
| Loopback-only listener | Owner's chosen security posture | No off-loopback bind, no TLS, no auth |

### 7.2 Assumptions

| ID | Assumption | Confidence | Owner | How to validate |
|----|-----------|------------|-------|-----------------|
| A-001 | Loopback isolation is sufficient protection for the secret-capturing page on a single-user machine | Medium | Owner | Accept for v1; revisit if worklog runs on shared hosts |
| A-002 | A browser form is genuinely faster/less error-prone for the owner than the TTY wizard | Medium | Owner | Dogfood a clean install; compare against `--cli` |
| A-003 | The daemon can be started and serve the setup page before any other setup step completes (no hidden bootstrap dependency on a configured secret) | High | Owner | Start daemon on a fresh machine with empty keychain |
| A-004 | `--cli` is the right escape-hatch surface name and behaviour for headless users | High | Owner | Confirmed in discovery |

**Validated assumptions**: A-004 (scope/flag) and the serving approach (7.1) were confirmed during discovery. The router-mounting decision (former Q-001) was resolved against the codebase: `daemon::router()` is one flat axum router served over both transports, so `/setup` routes need no new listener.

### 7.3 Dependencies

| Dependency | Type | Owner | Status | Risk if delayed |
|-----------|------|-------|--------|-----------------|
| Existing daemon HTTP server (axum, `127.0.0.1:9323`) | Blocking | Tomas | Available | None — already present |
| Existing `secrets`/`schedule`/`hook`/`skill`/`daemon_service` modules | Blocking | Tomas | Available | None — already present |
| Default-browser open capability (`open::` or equiv.) | Informational | Tomas | Available | Falls back to printed URL |

---

## 8. Open Questions

> Maximum 3. Each has an owner and resolution deadline.

| ID | Question | Impact if unresolved | Owner | Deadline |
|----|---------|----------------------|-------|----------|
| Q-001 | Should the setup page and the dockerized review UI eventually merge into one served surface, or stay separate? | Affects long-term architecture, not v1 (v1 is daemon-served under `/setup`) | Tomas | Post-v1 |

---

## 9. Revision History

| Version | Date | Author | Changes | Reason |
|---------|------|--------|---------|--------|
| 1.0 | 2026-06-08 | Tomas | Initial draft | — |
| 1.1 | 2026-06-08 | Tomas | Enumerated the `KNOWN_KEYS` parity set (§4.1.1) with per-key validation/probe; resolved Q-001 (mount `/setup` on existing router) into a §7.1 constraint; split FR-008 into FR-008a/b/c; added bootstrap FR-013 + AC-011 and credential-capture FR-004b; added NFR IDs; pinned AC-008 "files differ" to byte-for-byte comparison; added credential-deletion non-goal | spec-judge iteration-1 feedback |
| 1.2 | 2026-06-08 | Tomas | Defined preflight (FR-009) and daemon-service (FR-008c) response shapes; added preflight-absent AC-012; specified NFR-O1 log level/destination/fields; made NFR-P1 measurement CI-automatable | spec-judge iteration-2 feedback |

---

## Appendix

### A. Glossary

| Term | Definition in this spec |
|------|-------------------------|
| Owner | The single local user installing/configuring worklog on their own machine |
| Daemon | The native Rust axum process bound to `127.0.0.1:9323` that serves the setup UI and performs privileged writes |
| TTY wizard | Today's terminal onboarding flow in `rust/crates/worklog-cli/src/wizard.rs`, reached via `worklog setup --cli` |
| Local installs | The Claude Code hook, bundled skill, scheduled collection, and daemon auto-start — installs that touch the local machine |
| Preflight | The docker/claude/git presence checks run before configuration |
| KNOWN_KEYS | The existing set of configurable secret keys in `secrets::KNOWN_KEYS` |

### B. Mockups / Wireframes

> No mockups — behavior is fully specified by acceptance criteria in Section 3. The page is a single scrollable form mirroring the TTY wizard's section order: preflight → credentials → estimator → schedule → local installs → handoff.

### C. Reference Documents

| Document | What it answers | Link |
|----------|----------------|------|
| CLAUDE.md | Stack, daemon/web architecture, conventions | ./CLAUDE.md |
| Existing TTY wizard | The full set of steps parity is measured against | rust/crates/worklog-cli/src/wizard.rs |
| Web serving helpers | How the dockerized UI is brought up (the contrast case) | rust/crates/worklog-core/src/web.rs |
