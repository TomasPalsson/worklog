# Brief — T107
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
- [ ] T107 Star field interactions fixed, camera kept across refresh, engine split out (B4) — files: web/components/stats/StarField.tsx, web/components/stats/StarField.test.tsx, web/components/stats/starfieldEngine.ts, web/components/stats/starfieldEngine.test.ts, web/components/stats/starfieldDraw.ts — verify: `cd web && bun test components/stats/StarField.test.tsx components/stats/starfieldEngine.test.ts && bun run typecheck` — after: T106


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

## Contract for T107 — Star field interactions fixed, camera kept across refresh, engine split out
USAGE      `<StarField report={report} />` unchanged.
CALLS      Verified in Chrome (Claude in Chrome, 1440×739, DPR 2) on the live-copy sandbox: (1) hovering a star never shows `.sx-star-card` — reproduced deterministically by dispatching `pointermove` at the screen position of four visible stars (no card); (2) clicking a legend chip (`engine.current?.flyTopic(id)`, StarField.tsx:473 → :361) never moves the camera (screenshots 1 s and 4 s later identical); (3) when RefreshSoon refreshes the report (AI names arrive), `useMemo(() => buildScene(report), [report])` (:393) gets a new report object, the engine effect (:403-417, deps [scene, k]) disposes and recreates the engine, and the camera jumps home; (4) StarField.tsx is 501 lines (size guard max 400). Also observed: `getImageData` on the canvas reads all zeros while the screen shows stars — check whether the canvas you draw to is the one on screen (e.g. a second engine/canvas, or drawing after dispose). Find the ROOT CAUSE of (1) and (2) before changing code — candidate causes to rule in/out with a test: `v.fly` never cleared (t0 set only inside a loop that is not running → onMove's `v.keys.size > 0 || v.fly ? null` guard at :288 blocks hover forever); keys left in `v.keys` after keyup/blur (no keyup when focus leaves → stuck keys block hover and keep moving); `wake()` not starting the RAF loop when idle/hidden; hits computed in device px vs CSS px.
  Structure (split, no behaviour loss): move the non-React engine (`createEngine`, View/Engine types, input handlers, loop) to `web/components/stats/starfieldEngine.ts` and the per-frame drawing (`draw`, palette/sprite helpers) to `web/components/stats/starfieldDraw.ts`; StarField.tsx keeps the React shell (refs, effect, legend, cards, hint) and must end ≤ 400 lines; each new file ≤ 400 lines. `createEngine(wrap, canvas, ctx, scene, k, cb, opts?: { now?: () => number; raf?: (f) => number; caf?: (id) => void; camera?: Camera })` takes injectable clock/raf for tests.
  Fixes required: hover and click work whenever the camera is not mid-fly (a finished or cancelled fly clears `v.fly`); flyTopic animates to the target and ends (fly duration 900 ms, ease-out); keys are cleared on `blur` and on `visibilitychange`; the camera survives a report refresh: keep the last camera in a ref in StarField and pass it as `opts.camera` to the new engine when the topic count is unchanged (new topics → start from home); a refresh must not reset a pinned panel's star if the block still exists (re-find by block_id).
THE FIVE   (1) No stuck state: after any sequence of input, hover works again once idle. (2) The loop still stops when idle. (3) Key handlers stay on the focused wrapper only. (4) Disposing removes every listener and cancels RAF. (5) Files ≤ 400 lines.
TESTS      starfieldEngine.test.ts with a fake 2-D context (record calls; measureText returns {width: text.length*8}) and fake raf/clock: after one draw, a pointermove at a star's projected CSS-px position calls onHover with that star; after `flyTopic(id)` and frames covering 900 ms, the camera equals `flyTarget` (±1e-6) and `v.fly` is null and hover works again; keydown "w" then blur → keys empty; dispose cancels raf and removes listeners (spy add/removeEventListener counts match); `opts.camera` is used as the starting camera. StarField.test.tsx: re-rendering with a new report object of the same topic count keeps the engine camera (spy on createEngine's opts.camera).

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T107.
