# Brief — T008
Base: 7d2dba0
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-block-clues/.specs/007-session-lanes-and-fresh-descriptions
Approved: 2026-09-27 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml`

## Phase 1 — Session lanes
Goal: one repo serving two customers yields separate blocks per customer session.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session` — green, and all existing infer tests still green.

## Your task
- [ ] T008 customer lanes split only the minutes their folder already owns (B9) — files: rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer_lane_tags.rs, rust/crates/worklog-core/src/infer_lane_tags_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_lane_tags` — after: T007


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
no sleeps, no snapshot as the only assertion. Never skip, weaken, or delete a test.

Write a high-quality, general-purpose solution. Do not hard-code values or special-case
the test inputs. If the task is unreasonable or a test is wrong, say so in the report
instead of working around it.

Before reporting done, run `scripts/slop-check --base <base>` and either fix each
finding or justify it in one line in the report.
```

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T008.

## Contract for T008 (orchestrator, binding)
WHY        `infer_lanes::build_project_blocks` builds `keyed` with `lane_key` (folder, or `folder#Customer` when an event has `lane_tag`). So a folder split into customer lanes competes against OTHER folders as 2–3 weaker lanes: on real data another folder then wins minutes it never won before and the day's work total drops. Minute ownership between folders must be exactly what it was before tagging; tags may only divide a folder's own minutes.
DESIGN     Two-level ownership inside `build_project_blocks`:
           1. `keyed_folder` = today's keyed list but keyed by `lane_folder`. `runs = merge_by_evidence(fold_short_runs(owner_runs(&keyed_folder)), &keyed_folder)` — identical to pre-tagging behaviour.
           2. `runs = crate::infer_lane_tags::split_runs_by_tag(runs, &events)` (new module). For each run `(folder, s, e)`: collect the keyed entries (built with `lane_key`, same window rules) of events whose `lane_folder == folder`. If those entries carry < 2 distinct lane keys, emit ONE run whose owner is that single lane key (or `folder` if there are none) spanning exactly `[s, e]`. Otherwise run the same passes on that subset — `merge_by_evidence(fold_short_runs(owner_runs(&subset)), &subset)` — clamp each sub-run to `[s, e]`, drop empty ones, then make them tile `[s, e]` exactly (first starts at `s`, last ends at `e`, each gap goes to the preceding sub-run; merge touching sub-runs with the same owner). If that leaves nothing, emit the single run as above.
           3. Everything after (bucketing by `lane_key(e) == owner`, `by_key` lookup by lane key, span_block) is unchanged — every run owner is now a lane key.
           The early "< 2 distinct keys → `build(events)`" check must count distinct **lane keys** (so a single-folder day that is split by tags still splits; untagged days behave exactly as today).
           To share the keyed builder, factor today's inline `keyed` construction into `pub(crate) fn keyed_by(events: &[InferEvent], key: fn(&InferEvent) -> Option<String>) -> Vec<Keyed>` in infer_lanes.rs, and make `owner_runs` / `fold_short_runs` `pub(crate)`. `Run` = `(String, i64, i64)`.
FILES      infer_lanes.rs (MUST stay ≤ 400 lines — condense comments you touch, move nothing else), new `infer_lane_tags.rs` + `infer_lane_tags_test.rs` (wired `#[cfg(test)] #[path = "infer_lane_tags_test.rs"] mod tests;`), `mod infer_lane_tags;` in lib.rs (alphabetical, private).
NEVER      (1) Never change behaviour for days with no `lane_tag` — every existing test stays green untouched. (2) Never let a tag change which FOLDER owns a minute. (3) Never edit files outside FILES. (4) No inline lint suppressions.
TESTS      In infer_lane_tags_test.rs, build events with `InferEvent { .., lane_tag: Some(..) }` (see infer_lanes_test.rs helpers for shape) and run `crate::infer_lanes::build_blocks_by_project(events, crate::infer::build_blocks)` (check the real `build` fn name used in infer_lanes_test.rs):
           - `split_keeps_folder_minutes`: folder A (`/Users/dev/Desktop/Work/a`) has interleaved prompt streams tagged "Cust1" and "Cust2" (~20-min stretches each over ~2 h) while folder B (`/Users/dev/Desktop/Work/b`) has its own weaker interleaved activity. Build once with tags and once with every `lane_tag` set to None. Assert: the set of minutes covered by blocks whose events are in folder B is identical in both; the total minutes of folder-A blocks is identical in both; and the tagged build has ≥ 2 folder-A blocks, none mixing Cust1 and Cust2 events.
           - `untagged_session_inside_split_folder_keeps_its_own_stretch`: in folder A, Cust1 stretch, then a ≥20-min stretch of an untagged session, then Cust2 stretch → three blocks, the middle one holding only untagged events (FR-03), total minutes equal to the untagged-build total.
           - `single_customer_run_is_one_block`: a folder run holding only Cust1-tagged events (plus none untagged) → exactly the same block bounds as the all-untagged build.
           Confirm the first test FAILS against the current code before implementing (folder B gains or loses minutes, or totals differ). The existing `tagged_lanes_in_one_folder_compete_and_split_into_separate_blocks` and all infer/session tests must stay green.
COMMIT     Message describes the change (e.g. `fix(infer): customer lanes only divide their folder's own minutes`). No task ids. Nothing under .specs/.
