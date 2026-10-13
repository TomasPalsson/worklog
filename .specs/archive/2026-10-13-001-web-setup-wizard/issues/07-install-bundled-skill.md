# 07 — Owner can install/refresh the bundled skill, with byte-for-byte "files differ" detection

> Slice of [spec](../spec.md) · **Tag**: `AFK`

## What ships
The Owner opens the skill step; if any bundled file's on-disk bytes differ from the embedded copy the page shows "files differ — re-install overwrites"; installing/refreshing returns the file count and an up-to-date state.

## Tag rationale
- **AFK** → No human decision required. The "differ" rule is pinned to byte-for-byte comparison against each `include_str!` payload (AC-008), and the slice reuses the existing `skill::*` install paths.

## Depends on
- Slice 01 — Owner runs `worklog setup` and a daemon-served setup page opens (walking skeleton)

## Spec trace
- Journey: Journey 2 (happy path step 4, skill; edge "files differ")
- Requirements: FR-008b, FR-011 (skill state)
- Seam: skill install — reuse existing `skill::*`; up-to-date/differs determined by byte comparison against each `include_str!` payload.

## Implementation note (endpoint contract)
FR-008b defines the returned data (file count + up-to-date state) but — unlike FR-008c — leaves the wire shape unspecified. Suggested response `{ file_count: u32, state: "up-to-date" | "differs" | "absent" }`, mirroring FR-008c's style and FR-011's three-state enumeration. The implementer finalizes the exact path/shape.

## Acceptance criteria  (AC-NNN rows quoted verbatim; FR-/NFR-derived criteria traced, not invented)
- [ ] **AC-008** — Given the skill is installed but ≥1 bundled file's on-disk bytes differ from the binary's embedded copy, When the Owner opens the skill step, Then the page shows the "files differ — re-install overwrites" state, where "differ" is determined by byte-for-byte content comparison of each file against its `include_str!` payload (not a version field), before any write.
- [ ] **FR-008b** — Given the Owner installs/refreshes the skill, When the write completes, Then the daemon returns the file count and up-to-date state.
- [ ] **FR-011 (absent state)** — Given the skill has never been installed, When the Owner opens the skill step, Then the page shows it as `absent` and offers an install action with no overwrite warning.

## Out of scope
- Claude Code hook install (slice 06) and daemon auto-start (slice 08).
