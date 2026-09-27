# Brief — T005
Base: cd54c05
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-block-clues/.specs/007-session-lanes-and-fresh-descriptions
Approved: 2026-09-27 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml`

## Phase 2 — Fresh descriptions
Goal: a block that changed a lot is described again, and long blocks read as their tasks.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core description` — green, `reinference_preserves_tempo_id_and_description` still green.

## Your task
- [ ] T005 [P] long blocks described as tasks (B7) — files: rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core long_block_asks_for_tasks`


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

## Contract for T005 — long blocks described as tasks
CALLS      `pub const LONG_BLOCK_MINUTES: i64 = 90;` in estimate.rs. The block's user message carries `"describe_as_tasks": true` when `block_duration_minutes >= LONG_BLOCK_MINUTES`; SYSTEM_PROMPT gains one rule: when `describe_as_tasks` is true, write up to 3 imperative tasks joined by "; ", ≤ 140 chars for the whole joined string.
THE FIVE   (1) NEVER change behaviour for blocks < 90 min. (2) NEVER edit files outside `files:`.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T005.

## Orchestrator notes (binding)
- The user message is built in `estimate.rs::build_user_message` (it already computes `duration_min`). Add `"describe_as_tasks": duration_min >= LONG_BLOCK_MINUTES` there — present as `true`/`false` or only when true, your call, but blocks < 90 min must get no new instruction.
- `response_schema()`'s `description` field text says "max 120 chars"; the long-block rule allows 140 for the joined string. Update that schema description text so it does not contradict the new SYSTEM_PROMPT rule (e.g. "max 120 chars; up to 3 tasks joined by \"; \", max 140 chars, when describe_as_tasks is true").
- Required test in estimate.rs's inline tests: `long_block_asks_for_tasks` — a 90-min block's user message has `describe_as_tasks` true, an 89-min block's does not (parse the JSON, don't substring-match), and `SYSTEM_PROMPT` contains the new rule (mentions `describe_as_tasks`, `"; "`, `3`, `140`).
- Another session may be editing estimate.rs's invoke loop on another branch; keep your diff small and away from the loop.
- Commit message describes the change (e.g. `feat(estimate): describe blocks of 90+ min as up to 3 tasks`). No task ids.
