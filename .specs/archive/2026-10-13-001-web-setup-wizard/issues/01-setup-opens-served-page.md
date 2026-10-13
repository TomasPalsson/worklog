# 01 — Owner runs `worklog setup` and a daemon-served setup page opens; `--cli` still runs the TTY wizard

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
On a fresh box, `worklog setup` boots the native Rust daemon (if needed), serves `/setup` on loopback, opens it in the default browser, and prints the URL; `worklog setup --cli` runs today's TTY wizard unchanged — headless onboarding is never broken.

## Tag rationale
- **AFK** → No human decision required. Page layout is fully determined by Appendix B ("a single scrollable form mirroring the TTY wizard's section order", which explicitly waives mockups) and NFR-A4 ("follow the existing review-UI styling"); the router-mounting approach is resolved in §7.1. There is no unmade visual/architecture decision, so no author eyeball is required before dependent slices build on the shell.

## Depends on
- none (walking skeleton)

## Spec trace
- Journey: Journey 1 (happy path, steps 1–2) · Journey 3 (happy path + edge)
- Requirements: FR-001, FR-002, FR-003, NFR-R2, NFR-E4, NFR-E5
- Seam: served-setup surface + CLI routing — `/setup/*` mounted on the existing flat `daemon::router()` (one router over the unix socket + `127.0.0.1:9323`, §7.1); `worklog setup` flag dispatch in `worklog-cli`.

## Acceptance criteria  (verbatim from the spec — do not invent)
- [ ] **AC-001** — Given a fresh install with no daemon running, When the Owner runs `worklog setup`, Then the daemon starts, the browser opens the loopback setup URL, and the URL is printed to the terminal.
- [ ] **AC-009** — Given a headless environment, When the Owner runs `worklog setup --cli`, Then the TTY wizard runs to completion with no browser launch and no dependency on the web setup endpoints.
- [ ] **AC-010** (SHOULD) — Given the browser fails to auto-open, When the Owner runs `worklog setup`, Then the CLI prints the loopback setup URL so the Owner can open or forward it manually.
- [ ] **NFR-E4** — Given the daemon becomes unreachable while the setup page is open, When the browser polls or the Owner submits a form, Then the page shows a connection-lost state (recovery: rerun `worklog setup`, or use `--cli`).
- [ ] **Edge (Journey 1)** — Given the daemon is already running, When the Owner runs `worklog setup`, Then it reuses the running daemon rather than starting a second instance.
- [ ] **FR-003** — Given the setup routes are mounted, When a request arrives off-loopback, Then it is not served (the page is reachable only over the unix socket + `127.0.0.1:9323`, never a non-loopback interface).

## Out of scope
- Page *content* (bootstrap/DB state, preflight, credentials, installs) — those are slices 02–08. This slice ships the served shell + CLI routing only.
