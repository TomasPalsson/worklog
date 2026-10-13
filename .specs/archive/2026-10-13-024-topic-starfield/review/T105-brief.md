# Brief — T105
Base: 7dfbbaf
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/embedding-stats/.specs/024-topic-starfield
Approved: 2026-10-12 by user (pre-approved: /goal directive)
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Phase 2 — Star field
Goal: the galaxy card toggles to a flyable 3-D star field.
Independent test: `cd web && bun test components/stats && bun run typecheck`

## Your task
- [ ] T105 [P] Star field scene, camera and canvas view (B4) — files: web/components/stats/starfield.ts, web/components/stats/starfield.test.ts, web/components/stats/StarField.tsx, web/components/stats/StarField.test.tsx, web/app/stats/starfield.css — verify: `cd web && bun test components/stats/starfield.test.ts components/stats/StarField.test.tsx && bun run typecheck`


## Tests
verify: names one test; the spec states more. List every boundary and negative case it
states for this task (exactly-at and just-past each limit, empty, missing, sibling/prefix,
error path) and write ONE assertion per item in RED, even when verify: runs a single test.
A guard delegated to a shared helper still needs tests at the call site that fail for the
helper's plausible wrong versions. Beside each assertion name the wrong implementation it
catches (swallow the error, startswith with no separator, > for >=, a running total from
zero); an assertion that would still pass against that version is not a RED test.

## Prior art
Prior art: none found


## Before you write
```
Avoid over-engineering. Only make changes that are directly requested or clearly
necessary. Keep solutions simple and focused:

- Scope: Don't add features, refactor code, or make "improvements" beyond what was
  asked. A bug fix doesn't need surrounding code cleaned up. A simple feature doesn't
  need extra configurability.
- Documentation: Don't add docstrings, comments, or type annotations to code you
  didn't change. Only add comments where the logic isn't self-evident.
- Defensive coding: Don't add error handling, fallbacks, or validation for scenarios
  that can't happen. Trust internal code and framework guarantees. Only validate at
  system boundaries (user input, external APIs).
- Abstractions: Don't create helpers, utilities, or abstractions for one-time
  operations. Don't design for hypothetical future requirements. The right amount of
  complexity is the minimum needed for the current task.

Before writing any new function, class, or helper: search the whole repository for an
existing implementation of the same behaviour. A name search is not enough — the helper
you need is usually named differently. Do all three: (1) list the helper packages
(`**/{utils,support,helpers,lib,common,shared,core}/**`) and open every small module
there; (2) grep the exact idiom you are about to write — the regex (`[^a-z0-9]+`), the
join, the format string, the arithmetic — an existing helper contains it; (3) grep the
verb and two synonyms (slug: kebab, dash, hyphen; format: render, label; validate:
check, verify; parse: load, decode; retry: backoff) plus the library you would import.
When an LSP tool is available, query workspace symbols for each term too. Reuse what
exists. State in one line what you searched and what you found before your first edit,
in the form: `searched: <terms and dirs>; found: <path:line | nothing>`.

Two rules that are not the same rule:
- An existing helper is reused, always. Re-implementing one is a defect.
- A new helper is extracted at the third similar block, or in a measured hot path,
  never at the second. Three similar lines beat a premature abstraction.

Third-party APIs: training data is older than the installed package. For each
third-party symbol your diff calls that `git grep` finds nowhere else in the repo, read
the installed version and the symbol's source or types before you write the call, then
put one line in your report: `api: <pkg>@<version from the lockfile or the installed
package.json/METADATA> <path:line in the installed types or source | doc URL>`. When no
installed source or doc can be read, write `api: <pkg>@<version> UNVERIFIED` and say so.
A symbol already used elsewhere in the repo needs none: copy that call's shape.

Comments: default to none. Add one short line only when the WHY is non-obvious — a
hidden constraint, an invariant, a workaround for a specific bug, behaviour that would
surprise a reader. Never explain WHAT the code does, and never reference the current
task, fix, ticket, or callers; those belong in the report and rot in the code.
No docstring on a function shorter than five lines unless the file already documents
every sibling.

Match the surrounding file exactly: quote style, naming case, indent, import style,
error types. Read the three functions above and below your insertion point first.

Tests: the new test must fail against the unchanged code before you make it pass, and
you report that exit code. Expected values come from the spec or hand computation,
never from running the implementation. No `assert True`, no `toBeDefined()` alone, no
asserting a mock was called with the input you just passed, no `raises(Exception)`,
no sleeps (wait on a condition with a timeout that fails with a message), no snapshot
as the only assertion. Never skip, weaken, or delete a test. A mock returns the
full shape of the real response, not only the fields this test reads. A method only
tests call lives in test utilities, not the production class.
Test through the public interface; mock only at system boundaries (network, clock,
randomness, third-party APIs), never your own modules; verify through the interface,
never through a side channel such as querying the database the code just wrote.
Scratch checks verify; they do not become committed test files unless the task or repo convention asks.
A new external call gets its own named function (getUser, createOrder) beside the
existing ones, and a test mocks that function. A mock never branches on URL or
endpoint. Never refactor an existing generic client inside this task; report it instead.

Write a high-quality, general-purpose solution. Do not hard-code values or special-case
the test inputs. If the task is unreasonable or a test is wrong, say so in the report
instead of working around it.

Before reporting done, run `scripts/slop-check --base <base>` and either fix each
finding or justify it in one line in the report.
```

The slop-check command above lives at an absolute path; run it as:
/Users/tomas/Desktop/Projects/flow/plugins/flow/skills/no-slop/scripts/slop-check --base 7dfbbaf

## Contract for T105 — Star field (pure scene/camera module + canvas view)
USAGE      `const StarField = dynamic(() => import("./StarField"), { ssr: false }); <StarField report={report} />`
FILES      new: `web/components/stats/starfield.ts` (pure, no DOM), `web/components/stats/starfield.test.ts`, `web/components/stats/StarField.tsx` (`"use client"`, default export), `web/components/stats/StarField.test.tsx`, `web/app/stats/starfield.css` (import it from StarField.tsx the way other modules import their css — check how galaxy.css is loaded and mirror it).
CALLS      Pure module API (all numbers in world units, R = 100):
  - `buildScene(report: TopicsReport): Scene` — `stars[]` from `report.points`: `pos = [(sx−0.5)·2R, (sy−0.5)·2R, (sz−0.5)·2R]`, `r = 0.8 + 2.6·√(seconds / maxSeconds)`, `topic`, `firstTime` (block_id ∈ report.first_times), `oddball`, `label`, `day`, `seconds`, `ticket`, `novelty`; `topics[]`: `{id, name, centroid, spread (RMS distance of its stars to centroid), seconds}`; `dust[]`: 700 deterministic background points on a sphere of radius 6R from a seeded LCG (seed 1), brightness 0.2–0.6.
  - `initialCamera(scene): Camera` — `{pos:[0, 0.15R, −2.6R], yaw: 0, pitch: −0.05, fov: 60°}` looking at the origin (+z forward at yaw 0).
  - `forward(cam)`, `right(cam)` unit vectors; `stepCamera(cam, keys: ReadonlySet<string>, dt: number): Camera` — W/S along forward, A/D along right, E/Q world up/down, speed 0.6R/s, ×3 when `"Shift"` held; arrow keys rotate yaw/pitch at 1.4 rad/s; pitch clamped to ±1.45.
  - `look(cam, dxPx, dyPx): Camera` — 0.004 rad per pixel; `dolly(cam, wheelDeltaY): Camera` — moves along forward by −deltaY·0.05·R/100.
  - `orbit(cam, dt): Camera` — idle auto-orbit around the origin at 0.06 rad/s keeping distance and height, facing the origin.
  - `project(cam, p, w, h): {x, y, depth, scale} | null` — perspective with fov; null when behind the camera (depth ≤ 0.5).
  - `pick(projected: {i, x, y, rPx}[], mx, my, maxPx = 14): number | null` — nearest centre within max(maxPx, rPx); ties → nearer depth.
  - `flyTarget(cam, topic): Camera` — camera placed at `centroid − forward·max(2.2·spread, 0.35R)` looking at the centroid; `ease(a, b, t)` interpolates position and angles (cubic ease-out).
  View (StarField.tsx):
  - A focusable wrapper `<div className="sx-star" tabIndex={0} aria-describedby=hintId>` holding `<canvas role="img" aria-label="3-D star field: {n} blocks in {k} topics">`, a topic legend row (chips with colour + name; click → fly), the hint `"Click the field, then W A S D to fly · drag to look · Shift faster · R reset · Esc leave"`, a hover card (block label, topic name, day, hours via formatDuration, ticket if any) and a pinned info panel on click (adds "Newness N%").
  - Rendering per frame on a DPR-scaled canvas sized to the wrapper (ResizeObserver): fill background from the CSS token (getComputedStyle on the wrapper: `--bg` or the card token used in galaxy.css); dust (tiny, alpha by brightness, rotate with the camera only — infinitely far); topic nebulae (large radial gradient in topic colour at the projected centroid, radius `spread·1.8` projected, alpha 0.10 dark / 0.07 light); stars back-to-front: glow sprite (pre-rendered offscreen canvas per topic colour, radial gradient) at 4× r plus a solid core; alpha fades with depth (fog from 1.5R to 7R); first-time stars twinkle (sin(t·3 + i)) unless reduced motion; oddballs get a thin ring; topic labels (13px mono uppercase, topic colour mixed toward --fg, alpha by depth, hidden when behind camera) with hit boxes. In dark mode use `globalCompositeOperation = "lighter"` for glows; in light mode `"source-over"` with stronger cores. Resolve `oklch(...)`/`var(...)` colours to canvas-usable strings by setting them on a hidden element and reading `getComputedStyle(el).color` once per topic.
  - Loop: requestAnimationFrame only while (visible via IntersectionObserver) && (document visible) && (auto-orbit active OR keys held OR a fly animation running OR a drag in progress); otherwise draw on demand (resize, hover change, theme change).
  - Input: keydown/keyup on the wrapper only (preventDefault for handled keys so the page does not scroll); pointerdown + pointermove on the canvas for drag-look (setPointerCapture); wheel on the canvas with preventDefault (passive: false) only while focused; click → pick star (pin panel) or label hit (fly). Escape → close panel and `blur()`. Any input stops auto-orbit for good.
  - prefers-reduced-motion: no auto-orbit, no twinkle, fly is instant.
  - `canvas.getContext("2d")` null (tests, old browsers) → render the text "Star field needs canvas support." and no loop.
THE FIVE   (1) Key handlers only on the focused wrapper — the page never loses arrow/space scrolling elsewhere. (2) Loop stops when idle/off-screen (no 60 fps forever). (3) DPR scaling (crisp on retina) and resize. (4) Dark and light both readable. (5) No new dependencies; module is client-only and loaded lazily.
TESTS      starfield.test.ts: buildScene maps sx/sy/sz=0.5 to origin and 1 to +R; radii bounds; firstTime/oddball flags; topic centroid and spread exact on a 3-point fixture; dust deterministic (same first 3 points twice) and on the 6R sphere; stepCamera W moves +forward by speed·dt, Shift ×3, A/D perpendicular, pitch clamp; project of a point straight ahead lands at canvas centre, behind → null; pick nearest within radius and null outside; flyTarget looks at the centroid (forward·(centroid−pos) > 0.99·|centroid−pos|). StarField.test.tsx (happy-dom, no canvas context): renders the fallback text, the legend chips with topic names, the hint, aria-label with counts, wrapper tabIndex 0; keydown "w" on the wrapper calls preventDefault; keydown on document.body does not.

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T105.
