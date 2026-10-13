# 06 — Owner can install/refresh the Claude Code hook from the page

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner clicks "install hook" → `hook::install` runs and the page shows the hook installed with its event count; a re-run offers refresh instead of duplicating.

## Tag rationale
- **AFK** → No human decision required. Reuses the existing `hook::install` and idempotency rules; the web layer is a thin caller returning state.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 2 (happy path steps 1–3)
- Requirements: FR-008a, FR-011 (hook state), FR-010 (non-blocking)
- Seam: hook install — reuse existing `hook::install`; web layer is a thin caller returning state.

## Implementation note (endpoint contract)
FR-008a defines the returned data (installed event count) but — unlike FR-008c — leaves the wire shape unspecified. Mount the action under the `/setup` router (§7.1) in the same style as FR-008c; suggested response `{ installed: bool, event_count: u32, error?: string }`, idempotent (the same endpoint serves first-install and refresh). The implementer finalizes the exact path/shape.

## Acceptance criteria  (AC-NNN rows quoted verbatim; FR-/NFR-derived criteria traced, not invented)
- [ ] **AC-005** — Given the hook is not installed, When the Owner triggers hook install from the page, Then the daemon installs it and the page shows it installed with the event count.
- [ ] **Edge (Journey 2 narrative + FR-011 — derived, not an AC-table row)** — Given the step is already installed and up-to-date, When the Owner opens it, Then the page offers refresh/re-install rather than a blind duplicate.
- [ ] **FR-010 (non-blocking)** — Given the hook install fails, When the page shows the error, Then it surfaces a retry control and all other setup steps remain completable.

## Out of scope
- Bundled-skill install (slice 07) and daemon auto-start (slice 08).
