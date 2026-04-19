# Feature: v0.6 — events submenu, ticket-flow fix, design polish, upgrade daemon-restart

## Description

Four-item bundle that closes out the review UI for daily use:

1. **Events submenu per block** — each block card grows a collapsed `▶ N events` chevron that expands in-place into a vertical list: `[source-icon] [title] · [time]`, with a truncated `details` preview and click-to-expand for the full thing (up to the 4 KiB captured for Claude-source events). Progressive disclosure, no layout shift elsewhere, keyboard-accessible.
2. **Ticket flow fix** — stop reading SQLite directly from the web container. Writes already go through the Rust daemon over TCP; reads now route through the same daemon, fixing the WAL-over-Docker VFS stale-read bug where bun:sqlite in the container can't see recent writes from the host daemon's connection. This is the root cause of "unassign then re-assign doesn't stick".
3. **Design polish pass** — contrast fixes (`--fg-subtle` + dark `--fg-muted` currently fail WCAG AA), visible focus rings on every interactive element, loading states on all async affordances, coherent modular typography scale, 8pt spacing audit. Runs the `/design` skill's 5-phase process end-to-end.
4. **`worklog upgrade` restarts running daemon** — after the atomic binary swap succeeds (not rolled back), probe the service. If it's installed and the old daemon is responding to `/health`, `launchctl kickstart -k` it (macOS) or `systemctl --user restart` (linux) so the new binary is the supervised one. Print a line so the user knows.

Version bump: **0.5.0 → 0.6.0** (feat:).

## Assumptions (baked into plan, correct at approval if wrong)

- Events submenu: **progressive disclosure inline on the card**, not a drawer or hover popover. Hover popovers fail touch + a11y; drawers are overkill for an inline list. Jakob's Law wins.
- Events fetch strategy: **load on first expand**, not eagerly. Cards stay cheap until a user actually drills in.
- Ticket flow fix scope: **migrate reads to daemon HTTP, retire `web/lib/db.ts`**. The partial asymmetry (reads direct, writes through daemon) is the actual bug — going all-in on the daemon fixes it permanently.
- Design aesthetic: **keep** the warm-editorial OKLCH palette. The user called out ticket-chip + general look as good. Polish, not redesign.
- Daemon restart on upgrade: **best-effort + announce**. If `launchctl kickstart` fails, we still return success (binary was swapped). A warning line is printed so the user knows to restart manually.
- Breaking change surface: NONE externally. Internally we're removing `web/lib/db.ts` — no external consumers.

## Behavior Inventory

| ID | Given | When | Then | Priority |
|----|-------|------|------|----------|
| B1 | Daemon has blocks + events + tickets in DB | web container calls `GET /days/:day` on the daemon | response contains `blocks[]` each with `event_count`, `sources[]`, and the total seconds for the day | P0 |
| B2 | Daemon has a block with 5 events | `GET /blocks/:id/events` | response is an ordered (by `started_at`) array of 5 event rows, each with `source`, `title`, `started_at`, `details` | P0 |
| B3 | Daemon has 200 cached tickets | `GET /tickets` | response is the full list, ordered by `updated DESC, key ASC`, plus a meta object `{ count, last_fetched }` | P0 |
| B4 | Block 17 has `jira_issue=NULL` in DB (just unassigned) | web UI renders day page | the ticket chip shows "Pick a ticket" (not the old value) because the daemon returned the fresh row — no WAL visibility gap | P0 |
| B5 | Block card is collapsed | user clicks the `▶ N events` chevron | list expands in-place, fetches events once (cache result client-side), shows them ordered by time | P0 |
| B6 | Events list is open | user clicks a truncated details snippet | it expands to show the full 4 KiB prompt / commit body / etc. | P0 |
| B7 | Events list is open | user presses Tab | focus moves through each event row; visible focus ring ≥3:1 contrast | P0 |
| B8 | Page loads with no events submenu opened | rendering | initial paint has no network calls for per-block events (saved for interaction) | P0 |
| B9 | Async Server Action is in flight (assign/set-description/delete) | the button or input is visible | the element shows a visible pending state (spinner or fade) — not just `opacity:0.5` | P0 |
| B10 | A user is editing a contenteditable description | they hit Enter and the Server Action is mid-flight | the input is disabled (no concurrent edits / races) | P1 |
| B11 | Page loaded in light mode | tab-through interactive elements | every element shows a focus ring with ≥3:1 contrast against its background | P0 |
| B12 | Page loaded in dark mode | same | same — focus ring passes 3:1 on the dark bg | P0 |
| B13 | Body text uses `--fg-subtle` | measure contrast in light mode | contrast ≥ 4.5:1 against `--bg` | P0 |
| B14 | Meta text uses `--fg-muted` | measure contrast in dark mode | contrast ≥ 4.5:1 against `--bg` | P0 |
| B15 | Combobox popover is open | keyboard user presses ↓ / ↑ arrow | the active option has a visibly different background that meets 3:1 against the popover surface | P0 |
| B16 | Daemon service is installed AND running AND serving v0.5.0 | user runs `worklog upgrade` and it swaps binary to v0.6.0 | post-swap, restart is triggered (launchctl kickstart / systemctl restart); daemon now reports v0.6.0 on `/health` | P0 |
| B17 | Daemon service is installed but NOT running | `worklog upgrade` succeeds | no restart attempted (nothing to restart); command prints a note | P1 |
| B18 | Daemon service is NOT installed at all | `worklog upgrade` succeeds | no restart attempted; no error; prints a note | P1 |
| B19 | `worklog upgrade` is run under tests (ENV_SCHEDULE_HOME set) | test executes | no real `launchctl` / `systemctl` call is made | P0 |
| B20 | Web UI loads on a day with no events on any block | rendering | block cards show `0 events` chip (disabled, not clickable), no errors | P1 |
| B21 | Daemon is unreachable (TCP closed) | web container renders | page shows a clear "daemon unreachable" empty state, NOT an uncaught 500 | P0 |
| B22 | `cargo test` + `bun test` | CI | all tests pass including new daemon endpoints + web reader + events component | P0 |

## Phases

### Phase 1: daemon read endpoints — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T1.1 | Add `repo::list_events_for_block(conn, block_id) -> Vec<Event>` stub returning `bail!("not implemented")` | `rust/crates/worklog-core/src/repo.rs` | — |
| T1.2 | Add axum route stubs: `GET /days/:day`, `GET /tickets`, `GET /tickets/meta`, `GET /blocks/:id/events` — all returning 501 Not Implemented for now | `rust/crates/worklog-core/src/daemon.rs` | — |
| T1.3 | Tests for B1 (day endpoint shape), B2 (block events), B3 (tickets list), B20 (empty events), B21 (bad block id → 404) | `rust/crates/worklog-core/src/daemon.rs` (in-module tests) + `rust/crates/worklog-core/src/repo.rs` | T1.1, T1.2 |

**Red Gate**: `cargo test --manifest-path rust/Cargo.toml` — MUST exit non-zero with the new tests failing against the 501 stubs.

**Commit**: `test(daemon,repo): add failing tests for /days, /tickets, /blocks/:id/events [RED]`

### Phase 1: daemon read endpoints — GREEN

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T1.4 | Implement `repo::list_events_for_block` — JOIN `block_events` + `events` WHERE `block_id = ?`, ORDER BY `started_at` | `repo.rs` | T1.1 |
| T1.5 | Implement handler for `GET /days/:day`: returns `{ day, blocks: [{...Block, event_count, sources: [{source, n}]}], total_seconds }`. One SQL query per piece, stitched in the handler — cheap for a personal tool. | `daemon.rs` | T1.4 |
| T1.6 | Implement `GET /tickets` — returns `{ tickets: JiraTicket[], meta: { count, last_fetched } }` | `daemon.rs` | — |
| T1.7 | Implement `GET /blocks/:id/events` — returns `Event[]` | `daemon.rs` | T1.4 |

**Green Gate**: `cargo test --manifest-path rust/Cargo.toml` passes. Phase 1 tests now green.

**Commit**: `feat(daemon): read endpoints — /days/:day, /tickets, /blocks/:id/events`

### Phase 1: daemon read endpoints — REFACTOR

- Extract the `/days/:day` handler's stitch logic into a private helper so `event_count`/`sources` computation is named.
- Ensure all new endpoints respect existing 400 (BadRequest) / 500 (Internal) error split.

**Commit**: `refactor(daemon): extract day-summary helper; tighten error taxonomy`

---

### Phase 2: web reader migration — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T2.1 | Tests for `lib/daemon.ts` read functions using a bun fetch-mock: `listBlocksForDay`, `listTickets`, `ticketCacheMeta`, `dayTotalSeconds`, `blockEvents(blockId)`. Assert each hits the right URL and returns the right shape | `web/lib/daemon.test.ts` | Phase 1 endpoints exist |

**Red Gate**: `bun test` must exit non-zero.

**Commit**: `test(web/daemon): add failing tests for read functions [RED]`

### Phase 2: web reader migration — GREEN

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T2.2 | Implement read helpers in `lib/daemon.ts` — plain `fetch()` with the existing `WORKLOG_DAEMON_URL` env, 5s timeout for reads (faster than writes) | `web/lib/daemon.ts` | T2.1 |
| T2.3 | Switch `app/[day]/page.tsx` to call `daemon.ts` helpers for all four reads | `web/app/[day]/page.tsx` | T2.2 |
| T2.4 | Retire `web/lib/db.ts` + `db.test.ts` — delete files. Keep `types.ts`. Drop `bun:sqlite` dep if unreferenced. | `web/lib/db.ts`, `web/lib/db.test.ts` | T2.3 |
| T2.5 | Page-level error boundary: if daemon is unreachable, render a clear "daemon unreachable — start `worklog daemon` on the host" empty state (B21) | `web/app/[day]/page.tsx` + `error.tsx` | T2.3 |

**Green Gate**: `bun test` passes. `bun run typecheck` clean. `bun run build` succeeds.

**Commit**: `feat(web): route all DB reads through the daemon — fixes stale-read over Docker WAL`

### Phase 2: web reader migration — REFACTOR

- Consolidate fetch-helpers into one shared `fetchDaemon()` wrapper so read + write paths share the same error-mapping.
- Remove dead `bun:sqlite` imports.

**Commit**: `refactor(web): one fetchDaemon helper; drop bun:sqlite`

---

### Phase 3: events submenu UI — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T3.1 | Create `web/components/EventList.tsx` stub: `export function EventList(props: { blockId: number }) { throw new Error("not implemented") }` | EventList.tsx (new) | — |
| T3.2 | Create `web/lib/format-event.ts` stub: `formatEventTime(iso: string): string`, `eventSourceIcon(source: string): ReactNode` — throw | format-event.ts (new) | — |
| T3.3 | Tests for B5 (click expand), B6 (show more on details), B7 (focus ring on event row), B20 (0 events disabled chip). Use bun:test + happy-dom via `@testing-library/react` if available, else DOM assertions | `EventList.test.tsx` (new) | T3.1 |

**Red Gate**: `bun test` MUST exit non-zero.

**Commit**: `test(events): add failing tests for per-block event submenu [RED]`

### Phase 3: events submenu UI — GREEN

#### Tasks
| ID | Task | Files | Depends On | Parallelizable |
|----|------|-------|------------|----------------|
| T3.4 | Implement `formatEventTime` + `eventSourceIcon` | `lib/format-event.ts` | T3.2 | Yes |
| T3.5 | Implement `EventList` — collapsed chevron ("▶ N events"), on expand fetch `daemon.blockEvents(blockId)` once, render ordered rows. Each row: `[icon] [title, 1-line]` with a `↓ details` button that expands the truncated details into full | `EventList.tsx` | T3.4, Phase 1 T1.7 | Yes |
| T3.6 | Wire `<EventList />` into `BlockCard.tsx` below the description, above `block-meta` | `BlockCard.tsx` | T3.5 | No |
| T3.7 | CSS: `.events-disclosure`, `.events-list`, `.event-row`, `.event-detail` — match the warm-editorial aesthetic. Focus ring on every row. | `globals.css` | T3.5 | Yes |

**Execution waves**:
- Wave 1: T3.4, T3.7 (no file conflicts)
- Wave 2: T3.5
- Wave 3: T3.6

**Green Gate**: `bun test` passes. Visual check: click a chevron, list expands.

**Commit**: `feat(events): per-block events submenu with progressive disclosure`

### Phase 3: events submenu UI — REFACTOR

- Extract the expand/collapse state hook so it can be reused for the "Show more details" toggle.
- Memoise the fetched events per block so re-expand is free.

**Commit**: `refactor(events): memoise fetch, share disclosure hook`

---

### Phase 4: design polish — RED

**Mandate**: Write failing tests ONLY (contrast snapshots + focus-ring existence).

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T4.1 | Add a contrast-audit test file. Parses `globals.css`, extracts OKLCH token pairs we care about (fg/bg, fg-muted/bg, fg-subtle/bg, ring/bg), computes approximate contrast via OKLCH lightness delta, asserts ≥4.5:1 for body pairs and ≥3:1 for UI pairs. Fails at the CURRENT token values per the explore agent's audit | `web/lib/contrast.test.ts` (new) | — |
| T4.2 | Add test that no interactive element in `BlockCard`, `TicketCombobox`, `EventList`, `ActionBar` is missing `:focus-visible` styling — simple grep-based test that each component's selectors appear in the `:focus-visible` blocks of globals.css | `web/lib/styleguide.test.ts` (new) | — |

**Red Gate**: `bun test` — the new contrast + focus-visible tests MUST fail (they flag the current token choices + the missing combobox-item focus style).

**Commit**: `test(design): add failing contrast + focus-visible guardrails [RED]`

### Phase 4: design polish — GREEN (runs `/design` skill's 5-phase process)

Apply the `/design` skill's discipline (think → architect → execute → polish → evaluate). Changes fall into 5 buckets:

#### Tasks
| ID | Task | Files | Depends On | Parallelizable |
|----|------|-------|------------|----------------|
| T4.3 | **Contrast** — bump `--fg-subtle` 0.68→0.60 (light), tighten dark `--fg-muted` 0.70→0.78, raise `--ring` lightness in both modes so 3:1 holds | `globals.css` | T4.1 | Yes |
| T4.4 | **Typography scale** — introduce a 1.25× modular step: 11 / 14 / 17 / 21 / 26 / 32 px, map to existing tokens (h1=32, h2=21, body=17→15 stays, meta=13→14, caption=11). Add `--lh-tight` (1.2) / `--lh-body` (1.55) / `--lh-meta` (1.4) tokens. | `globals.css` + component class usages | T4.1 | Yes |
| T4.5 | **Focus rings** — add explicit `:focus-visible` to `.combobox-item`, `.event-row`, `.day-nav-btn.today`, every previously-missing interactive. Match ring color to bg-raised. | `globals.css` | T4.1, T4.2, Phase 3 | Yes |
| T4.6 | **Loading states** — `.action-btn[aria-busy=true]` shows a small inline spinner (use indicatif-style CSS keyframes or a tiny SVG). `.ticket-chip[aria-busy=true]` same. `contenteditable` description gets `pointer-events:none + opacity:0.7` while the action is pending. | `globals.css` + `ActionBar.tsx`, `TicketCombobox.tsx`, `BlockCard.tsx` | — | Yes |
| T4.7 | **Spacing audit** — snap every gap/margin to 4/8/12/16/24/32 pixel grid. Fix the 28px-28px-14px rhythm bug (day-header → actions → blocks). | `globals.css` | — | Yes |
| T4.8 | **Responsive breakpoint** — at 320px the combobox popover currently overflows. Shrink `max-width: calc(100vw - 32px)` and shrink page padding floor from 24→16 on the narrow breakpoint. | `globals.css` | — | Yes |

**Execution waves**:
- Wave 1 (parallel): T4.3, T4.5, T4.6, T4.7 (no file conflicts — small CSS + component tweaks each)
- Wave 2: T4.4 (global typography may touch several components)
- Wave 3: T4.8 (responsive tweaks)

**Green Gate**: `bun test` passes (contrast + focus tests green). `bun run build` clean.

**Commit**: `feat(design): contrast + focus + loading + modular type + spacing polish`

### Phase 4: design polish — REFACTOR (runs `/design` skill's EVALUATE phase)

- Spawn a separate **evaluator subagent** (per the `/design` skill's mandate) against the polished output. Gets a composite score across Usability/Composition/Color+Type/Interaction/A11y/Microcopy.
- If composite ≥ 4.0: done.
- If 3.0-3.9: apply top-3 fix list, re-spawn evaluator (fresh context).
- If < 3.0: return to Phase 4 GREEN.

**Commit**: `refactor(design): evaluator-driven polish iteration`

---

### Phase 5: `worklog upgrade` restarts daemon — RED

**Mandate**: Write failing tests ONLY.

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T5.1 | Add `daemon_service::restart() -> Result<RestartOutcome>` stub returning `bail!("not implemented")`. Outcome enum: `Restarted`, `NotInstalled`, `NotRunning`, `Unsupported` | `daemon_service.rs` | — |
| T5.2 | Tests for B16 (installed+running → Restarted), B17 (installed+not running → NotRunning), B18 (not installed → NotInstalled), B19 (tests gated on ENV_SCHEDULE_HOME — no real launchctl call) | `daemon_service.rs` | T5.1 |
| T5.3 | Updater hook test: after swap succeeds with `rolled_back=false`, `restart()` is called once. Use a `RestartGate` trait injected at test time (mirrors the `ModelInvoker` test pattern). | `updater/mod.rs` | T5.1 |

**Red Gate**: `cargo test` MUST exit non-zero.

**Commit**: `test(daemon-service,updater): add failing tests for post-upgrade restart [RED]`

### Phase 5: `worklog upgrade` restarts daemon — GREEN

#### Tasks
| ID | Task | Files | Depends On |
|----|------|-------|------------|
| T5.4 | Implement `daemon_service::restart()`. macOS: `launchctl kickstart -k gui/$UID/is.p5.worklog.daemon` via `.output()` (silent stderr). Linux: `systemctl --user restart worklog-daemon.service`. Gated on `ENV_SCHEDULE_HOME` — tests never touch real supervisor. | `daemon_service.rs` | T5.2 |
| T5.5 | Hook into `updater/mod.rs::run_update` — after `swap_with_rollback` returns `Ok(outcome)` and `outcome.rolled_back == false`, call `daemon_service::restart()`; log the outcome. Swallow error (upgrade itself succeeded — restart is best-effort). | `updater/mod.rs` | T5.3, T5.4 |
| T5.6 | CLI output: surface the restart outcome in the `worklog upgrade` summary line ("✓ updated 0.5.0 → 0.6.0 · daemon restarted" or "… · daemon service wasn't running"). | `cli.rs` | T5.5 |

**Green Gate**: `cargo test` passes.

**Commit**: `feat(upgrade): restart the daemon service after successful swap`

### Phase 5: — REFACTOR

- Extract the restart-decision logic into a small pure fn so the CLI rendering and the updater hook share the same branches.

**Commit**: `refactor(upgrade): share restart-decision across updater + CLI`

---

### Phase 6: Inline Gates (MANDATORY)

```bash
cargo fmt --manifest-path rust/Cargo.toml --all
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test  --manifest-path rust/Cargo.toml

cd web && bun test && bun run typecheck && bun run lint && bun run build

bash scripts/release-smoke.sh
bash tests/install/smoke.sh
```

All must pass. Fix inline.

### Phase 7: Quality Pipeline (MANDATORY)

Launch 4 agents in parallel:

| Agent | Model | Artifact |
|-------|-------|----------|
| Security Scanner | sonnet | `.claude/quality/security.md` |
| Performance Analyzer | sonnet | `.claude/quality/performance.md` |
| Accessibility Auditor | sonnet | `.claude/quality/accessibility.md` (heavy focus this round — it's the design-polish PR) |
| Type-Safety Checker | sonnet | `.claude/quality/type-safety.md` |

Aggregate, auto-fix CRITICAL + high-confidence IMPORTANT (max 2 attempts), document the rest.

### Phase 8: Browser Verification (NON-SKIPPABLE)

Use Claude-in-Chrome:
1. `worklog web up` → navigate to http://localhost:3333
2. Inject `[VERIFY]` debug marker via console
3. Assign a ticket → unassign → assign different → verify DB state AND UI state match
4. Click `▶ N events` on a block → list expands with correct events and order
5. Click `Show more details` on an event → details expand to full 4 KiB
6. Tab through the page → every focusable element has a visible ring
7. Emulate dark mode → repeat contrast check
8. Emulate mobile 375px viewport → combobox doesn't overflow
9. Write results to `.claude/verification/browser-results.md`

### Phase 9: User Verification (HARD GATE)

Present results + screenshots. User must confirm:
1. **Ticket flow works** — unassign, reassign, unassign, reassign all reflect correctly in the UI with no refresh needed
2. **Events submenu is useful** — the information shown is what they'd want while reviewing
3. **Design feels better** — not worse. Contrast, focus rings, spacing all feel coherent.
4. **`worklog upgrade` restart works** — staged test: with a stable v0.5.0 running + local v0.6.0 build, fake an upgrade, confirm daemon cycles and reports the new version on `/health`.

**STOP. Wait for explicit "approved" / "ship it".**

### Phase 10: QA Pass

Run `/qa` skill. Expect PASS or PASS-WITH-NOTES. Fix ship-blockers, rerun gates, rerun QA.

### Phase 11: PR Creation + Release

- Bump workspace version `0.5.0 → 0.6.0` (the release-please will adjust the final number).
- Push `feat/v0.5-events-submenu-polish`
- Draft PR: `feat: events submenu + ticket-flow fix + design polish + upgrade daemon-restart (v0.6.0)`
- Merge after user approval → release-please → tag push → signed release.
- Local upgrade via `worklog upgrade` to validate the NEW restart feature end-to-end on the real host.

## Browser Verification Steps

See Phase 8 — nine steps including the cross-day assign/unassign drill, events submenu expand, focus ring audit, dark mode, narrow viewport.

## User Verification Steps

1. Assign → unassign → assign different → does the chip reflect reality without refresh?
2. Click `▶ N events` on any block → list expands with real event data?
3. Click "Show more" on an event detail → full text appears (no truncation marker)?
4. Tab through the whole day page → is every focused element visibly ringed?
5. Switch to dark mode → contrast still readable? (No eye strain on timestamps.)
6. `worklog upgrade` → does it announce restarting the daemon and does `/health` show the new version right after?

## Done When

- All P0 behaviors (B1-B9, B11-B16, B19, B21, B22) tested and passing.
- All quality gates PASS (design skill's evaluator composite ≥ 4.0 included).
- Browser verification PASS (9/9 assertions).
- User explicit approval.
- QA PASS (or PASS WITH NOTES, no ship-blockers).
- `worklog upgrade` on the user's host correctly restarts the daemon to v0.6.0.

## Rollback Plan

- Starting commit: `0a2a513` (post-v0.4 / v0.5.0 signed release).
- Revert sequence: `git revert --no-commit 0a2a513..HEAD` on the feature branch + force-push.
- Migrations: none (no schema change).
- Binary compatibility: forward-compatible. v0.6.0 reads the same DB schema as v0.5.0; downgrade is safe.
- The ONLY public surface change is `/days/:day` + `/tickets` + `/blocks/:id/events` on the daemon. Existing web containers on v0.5.0 don't call these, so adding them is purely additive.
