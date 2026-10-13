# 05 — Owner can set or disable the scheduled-collection interval

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner selects "every 15m" → the launchd/systemd scheduler entry is installed and the page shows the active interval; selecting "off" disables it.

## Tag rationale
- **AFK** → No human decision required. The interval set is enumerated (FR-007) and the slice reuses the existing `schedule::*` install/uninstall with no new scheduling mechanism.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 1 (happy path step 5, schedule) · Journey 2 (happy path step 4)
- Requirements: FR-007, FR-010 (non-blocking)
- Seam: schedule config — reuse existing `schedule::*` install/uninstall; no new scheduling mechanism.

## Acceptance criteria  (AC-NNN rows quoted verbatim; FR-/NFR-derived criteria traced, not invented)
- [ ] **AC-006** — Given the schedule section with an interval chosen, When the Owner sets collection to "every 15m", Then the daemon installs the scheduler entry and the page reflects the chosen interval.
- [ ] **FR-007 (set)** — Given the schedule step, When the Owner chooses any of {every 5m, every 15m (default), hourly, every 4h, daily} (the `schedule::Interval` set), Then that interval is applied via the existing `schedule::*` install and the page reflects it.
- [ ] **FR-007 (off)** — Given a schedule is active, When the Owner chooses "off", Then scheduled collection is disabled (the scheduler entry is removed) and the page reflects the disabled state.

## Out of scope
- Daemon auto-start (login service) install — that is slice 08.
