# Brief — T108
Base: 1f89fd2
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/embedding-stats/.specs/024-topic-starfield
Approved: 2026-10-12 by user (pre-approved: /goal directive)
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Phase 3 — Star field that works and wows
Goal: hover, click, fly-to and held keys work in a real browser; the field fills the view with constellations and readable names; map labels sit above islands.
Independent test: `cd web && bun test components/stats && bun run typecheck`

## Your task
- [ ] T108 Star field look: fit, constellations, label declutter, glow (B4) — files: web/components/stats/starfieldScene.ts, web/components/stats/starfieldScene.test.ts, web/components/stats/starfieldDraw.ts, web/components/stats/starfieldLabels.ts, web/components/stats/starfieldLabels.test.ts, web/app/stats/starfield.css — verify: `cd web && bun test components/stats/starfieldScene.test.ts components/stats/starfieldLabels.test.ts && bun run typecheck` — after: T107


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
/Users/tomas/Desktop/Projects/flow/plugins/flow/skills/no-slop/scripts/slop-check --base 1f89fd2

## Contract for T108 — Star field look: fit, constellations, label declutter, glow
USAGE      unchanged component; pure helpers in starfieldScene.ts / starfieldLabels.ts.
CALLS      Chrome screenshots (1440 px) show: all stars bunched in the middle third with lots of empty sky; topic names stacked on top of each other (e.g. "LIBRECHAT DEVELOPMENT" over "VÍS SJÚKRA INTEGRATION"); glows faint. Goal: an instantly beautiful, readable star map that is fun to fly. Changes:
  1. Fit (starfieldScene.ts): `initialCamera(scene, aspect = 16/9)` places the camera so the stars' bounding sphere (centre = mean of star positions, radius = max distance) fills 75% of the smaller of the vertical/horizontal FOV: distance = radius / tan(min(fovY, fovX)/2 · 0.75)… (state your exact formula in a comment and test it); looks at the bounding centre; slight downward pitch kept. Orbit and R-reset use this camera.
  2. Readable islands (buildScene): after positions are placed, for each topic with ≥ 2 stars scale offsets from its centroid so RMS spread = max(current spread, 0.22 × distance from this centroid to the nearest other topic centroid), capped so it never exceeds 0.45 × that distance. Pure, deterministic; topics with 1 star unchanged.
  3. Constellations: `constellation(points: Vec3[]): [number, number][]` — Euclidean minimum spanning tree (Prim, O(m²) per topic, m ≤ 2000 total) over a topic's stars; draw each edge as a 1 CSS-px line in the topic colour, alpha 0.28 dark / 0.18 light, multiplied by the fog of the nearer endpoint; edges of the hovered/pinned star's topic drawn at alpha 0.6.
  4. Labels (`starfieldLabels.ts`): `placeLabels(cands: {id, x, y, w, h, priority}[], pad = 6): {id, x, y}[]` — greedy in priority order (topic seconds desc), skip any candidate whose padded box overlaps an already placed box or leaves the canvas; label anchor = projected centroid shifted up by (projected spread radius + 12 px); a skipped topic's label still appears while one of its stars is hovered or pinned, or its chip is hovered. Label font 13px (page floor), weight 600, letter-spacing 0.06em, a dark halo (stroke 4 px of the background) — keep text readable.
  5. Glow and depth: star core radius = max(1.6, r·scale) px capped at 16 px; glow sprite alpha 0.8 dark / 0.35 light at 5× core radius; nebula alpha 0.16 dark / 0.09 light with radius = max(projected spread·2.2, 28 px); dust gets a slow twinkle (unless reduced motion); fog from 0.8× to 3× the fit distance (relative to the fit, not fixed R).
  6. Hover feel: hovering a star brightens its topic (constellation 0.6, nebula ×1.6) and dims other topics' stars to 45% alpha; the hover card shows the topic name in the topic colour.
THE FIVE   (1) Deterministic (no Math.random). (2) No allocation-heavy work per frame beyond projections (MST computed once per scene). (3) Dark and light both readable. (4) 13px minimum text. (5) Reduced motion: no twinkle.
TESTS      starfieldScene.test.ts: initialCamera fits a known sphere (projected extreme points land within the canvas and the farthest within 70–80% of the half-height); island inflation exact on a 2-topic fixture (spread reaches 0.22×d; cap 0.45×d; 1-star topic unchanged); constellation returns m−1 edges forming a tree with minimal total length on a 4-point fixture (exact edge set). starfieldLabels.test.ts: overlapping candidates → only the higher priority placed; non-overlapping → all placed; off-canvas → skipped; padding respected.

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T108.
