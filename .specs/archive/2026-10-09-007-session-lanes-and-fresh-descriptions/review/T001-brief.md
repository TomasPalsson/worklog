# Brief — T001
Base: cb55066
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
- [ ] T001 lane_tag field and tagged lane key (B1) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer_lanes_test.rs, rust/crates/worklog-core/src/infer_evidence_test.rs, rust/crates/worklog-core/src/infer_allocations.rs, rust/crates/worklog-core/src/infer_allocations_test.rs, rust/crates/worklog-core/src/overlaps.rs, rust/crates/worklog-core/src/overlaps_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer`


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

## Contract for T001 — lane_tag field and tagged lane key
CONTRACT   rust/crates/worklog-core/src/infer.rs — add `pub lane_tag: Option<String>` to `InferEvent` with a doc line.
NAMES      lane tag = `lane_tag`; lane key = `lane_key`; lane folder = `lane_folder`.
CALLS      `pub(crate) fn lane_folder(e: &InferEvent) -> Option<String>` (today's lane_key body); `lane_key(e)` = `lane_folder(e)` + `"#" + tag` when tagged.
           `infer_allocations::events_by_key` and `overlaps` call `lane_folder`.
THE FIVE   (1) NEVER invent a field or name not in this block. (2) Every `InferEvent { .. }` literal gets `lane_tag: None`. (3) NEVER change lane ownership behaviour for untagged events. (4) NEVER edit files outside `files:`. (5) Keep `infer_lanes.rs` ≤ 400 lines — trim a comment if needed.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T001.

## Orchestrator notes (binding)
- `infer_lanes::build_project_blocks` looks up `by_key.get(owner)` where `owner` is a run's lane key. Once `events_by_key` is keyed by `lane_folder`, a tagged owner (`folder#Cust`) would miss. Fix: in `build_project_blocks`, look up the owner's events by **lane key** (build that one local map with `lane_key`, or filter by `lane_key(e) == owner`). `events_by_key` itself switches to `lane_folder` (allocation windows are folder-keyed).
- `infer_evidence.rs::single_project` keeps using `lane_key` (a folder's two customer lanes must not merge back). Do not edit `infer_evidence.rs`.
- Required test B1 in `infer_lanes_test.rs`: `tagged_event_gets_its_own_lane_key` — an event with project_path under `~/Desktop/Work/foo` and `lane_tag: Some("Acme")` → `lane_key` = `Some("foo#Acme")`, `lane_folder` = `Some("foo")`; the same event untagged → both `Some("foo")`. Add one more test: two interleaved event streams in one folder with different tags produce separate blocks via `build_blocks_by_project` (tagged lanes really compete), and each block's events keep the untouched `project_path`.
- Commit message describes the change (e.g. `feat(infer): lane_tag splits a folder's lane by session customer`). No task ids in it.
