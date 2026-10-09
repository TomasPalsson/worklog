# Brief — T003
Base: 791b666
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
- [ ] T003 wire tagging into the day rebuild (B4) — files: rust/crates/worklog-core/src/infer_allocations.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core two_customer_sessions_split` — after: T002


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

## Contract for T003 — wire tagging into the day rebuild
CALLS      In `infer_allocations::build_day_blocks`: `let mut events = load_day_events(..)?; tag_sessions(&mut events, &Registry::load(conn)?);` before building.
THE FIVE   (1) NEVER add a second rebuild path. (2) NEVER edit files outside `files:`.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T003.

## Orchestrator notes (binding)
- `session_customers::tag_sessions(events: &mut [InferEvent], registry: &Registry)` exists. It currently carries `#[allow(dead_code)] // unused until ...` because nothing called it. Once you call it from `build_day_blocks`, **delete that allow line** in `rust/crates/worklog-core/src/session_customers.rs` — that one-line deletion is the only edit you may make to that file (orchestrator ruling; inline lint suppressions are banned in this repo).
- Required DB test in `infer_allocations_db_test.rs`: `two_customer_sessions_split_into_separate_blocks`. Setup: in-memory DB (`crate::db::open_memory()`), insert two customers via `crate::billing_registry::upsert_customer` (e.g. names "Sjúkra" and "APRÓ", or aliases), then upsert interleaved events in ONE folder (`/Users/dev/Desktop/Work/vitinn-infra`) with two `session_id`s: session A's `claude_turn` titles name customer 1, session B's name customer 2, interleaved in alternating ~20-min stretches over ~2 h so each stretch clearly owns its minutes. Assert: `build_day_blocks` returns ≥2 non-calendar blocks; no block holds events from both sessions; every block's events have `project_path` under vitinn-infra; after `persist_blocks`, `crate::personal::dominant_project_path_for_block` for every saved block is the vitinn-infra path (FR-05). Confirm it fails before wiring (all in one block).
- Also add a control assertion or test: same setup where both sessions name the SAME customer → exactly the blocks you'd get with no customers in the registry (FR-04).
- Commit message describes the change (e.g. `feat(infer): day rebuild splits a repo's sessions by customer`). No task ids. Do not commit .specs/.
