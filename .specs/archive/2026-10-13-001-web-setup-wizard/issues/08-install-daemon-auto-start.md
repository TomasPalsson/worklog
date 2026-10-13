# 08 — Owner can install daemon auto-start, with a non-blocking failure + retry

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner clicks "install daemon auto-start"; on success the page shows it installed with the platform + unit path; on an unsupported OS or denied write the step shows failed-with-reason + a retry control while every other step stays completable.

## Tag rationale
- **AFK** → No human decision required. The response shape (FR-008c) and failure behaviour (AC-007) are fully defined, and the slice reuses the existing `daemon_service::*` privileged write. The write is reversible (an uninstall exists), so it is not a destructive action requiring a checkpoint.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 2 (happy path step 4, daemon; error path "privileged write fails")
- Requirements: FR-008c, FR-010, FR-011 (daemon-service state), NFR-E3
- Seam: daemon-service install — reuse existing `daemon_service::*` privileged write (launchd/systemd); failure is isolated to this step.

## Acceptance criteria  (AC-NNN rows quoted verbatim; FR-/NFR-derived criteria traced, not invented)
- [ ] **AC-007** — Given an unsupported OS or denied write, When the Owner triggers daemon-auto-start install, Then the page shows that step as failed with a reason and a retry control, and remaining steps stay usable.
- [ ] **FR-008c (response-shape constraint — not a Journey AC row)** — On install the daemon returns `{ installed: bool, platform: "launchd" | "systemd" | "unsupported", unit_path?: string }`.
- [ ] **FR-010 (non-blocking)** — Given any step fails, When the failure is shown, Then that step shows an error and a retry control while all other steps remain completable — each step fails independently and non-blockingly.

## Out of scope
- Scheduled-collection interval (slice 05), hook (slice 06), and skill (slice 07) installs.
