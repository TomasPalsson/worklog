# 02 — Setup page shows fresh-install bootstrap (home + DB) and preflight diagnostics on load

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
A fresh box with no home dir → `worklog setup` creates the home tree, initializes/migrates the SQLite DB, and the page shows schema version + event/block counts plus a docker/claude/git preflight panel with per-tool found/version/hint.

## Tag rationale
- **AFK** → No human decision required. Both response shapes are fully defined in the spec (FR-009 per-tool shape; FR-013 DB state). Pure read-only diagnostics over existing init logic.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 1 (happy path step 3; AC-011 + AC-012 edge cases)
- Requirements: FR-013, FR-009, NFR-P1
- Seam: read-only diagnostics — DB scaffold/migrate via existing home + SQLite init; preflight tool probes (`docker` / `claude` / `git`).

## Acceptance criteria  (verbatim from the spec — do not invent)
- [ ] **AC-011** — Given a fresh install with no worklog home / DB yet, When the Owner runs `worklog setup`, Then the daemon scaffolds the home tree, initializes/migrates the SQLite DB, and the page shows the DB state (schema version + event/block counts).
- [ ] **AC-012** — Given a preflight tool (e.g. `docker`) is not installed, When the Owner runs `worklog setup`, Then the page shows that tool as `found: false` with its install hint, and the other setup steps remain usable.
- [ ] **FR-009 (shape)** — Given the preflight runs, Then each tool returns `{ tool, found, version? (first line of `<tool> --version`), hint? (present when not found) }`.
- [ ] **NFR-P1** — Given the daemon is warm, When the page first renders, Then the `/setup` handler responds in < 2 s.

## Out of scope
- The set/unset state of credentials and installs — rendered by slices 03 and 06–08 respectively.
