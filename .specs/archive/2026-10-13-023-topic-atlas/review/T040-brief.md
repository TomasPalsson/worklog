# Brief — T040
Base: 00bbb3f
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/embedding-stats/.specs/023-topic-atlas
Approved: 2026-10-11 by user (pre-approved: /goal directive, /flow:next --finish)
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Phase 1 — Honest, named themes on a map
Goal: /stats/topics returns centered-vector topics with keywords, cached AI names, map points, per-day metrics, river and highlights; the web shows them as the Topic atlas.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats && (cd web && bun test components/stats)` — Rust math, naming cache, route and every atlas component together.

## Your task
- [ ] T040 Final a11y pass: phone text and targets, ticket keys, light-mode marks, percentages (B4) — files: web/app/stats/compass.css, web/app/stats/mirror.css, web/components/stats/TopicRiver.tsx, web/app/stats/river.css, web/components/stats/TopicHeartbeats.tsx, web/app/stats/heartbeats.css, web/app/stats/galaxy.css, web/components/stats/TopicMoments.tsx, web/components/stats/TopicMoments.test.tsx, web/components/stats/TopicHeartbeats.test.tsx, web/components/stats/TopicRiver.test.tsx — verify: `cd web && bun test components/stats && bun run typecheck` — after: T036, T037, T038


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
/Users/tomas/Desktop/Projects/flow/plugins/flow/skills/no-slop/scripts/slop-check --base 00bbb3f

## Contract for T040 — Final a11y pass: phone text and targets, ticket keys, light-mode marks, percentages
USAGE      Topic atlas on /stats, all views.
CALLS      Design round 4 (composite 3.38; usability/a11y 3.0). Apply exactly these: (1) compass.css: `@media (max-width: 599px) { .sx-cmp-end, .sx-cmp-axis, .sx-cmp-leader-label (whatever the leader label class is in DayCompass.tsx) { font-size: 22px; } }` so viewBox text renders ~12px at 343px. (2) mirror.css: below 599px the grid sits in an `overflow-x: auto` wrapper and each column is at least 24px wide (min-width on the grid = 24px × columns); if the wrapper element does not exist, add it via CSS only on the existing card container. (3) River: `GUTTER` 48 → 60 (scale label no longer clipped); at ≤ 599px add a right-edge fade mask on the scroll container (`mask-image: linear-gradient(to right, black 85%, transparent)`) and a small "scroll →" caption. (4) Heartbeats: each ticket key in its own `<span class="hb-ticket">` with `white-space: nowrap`, the ticket line prefixed "Tickets:"; extend the local GENERIC chip list with: pptx, prs, pr, file, files, boost, thinking, binding, multi, base, revise, pdf. (5) Light-mode mark contrast: in galaxy.css (it is imported by the atlas section) add `:root:not([data-theme="dark"]) .sx-gal, :root:not([data-theme="dark"]) .sx-river, :root:not([data-theme="dark"]) .hb-row, :root:not([data-theme="dark"]) .sx-cmp { --project-l: 0.60; }` guarded so it only applies in light mode (respect both dark selectors: under `@media (prefers-color-scheme: dark) :root:not([data-theme="light"])` and `:root[data-theme="dark"]` it must NOT apply). (6) Moments: show scores as percentages — first times "85% new", oddballs "closest match 23% · your usual 57%"; tips keep the decimals.
TESTS      update/extend the existing stats tests for (3) gutter/caption, (4) nowrap ticket spans, prefix and new generic words filtered, (6) percentage strings; a CSS assertion for (1), (2) and (5) rules; full `bun test components/stats` green and typecheck clean.
THE FIVE   (same five as T015)

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T040.
