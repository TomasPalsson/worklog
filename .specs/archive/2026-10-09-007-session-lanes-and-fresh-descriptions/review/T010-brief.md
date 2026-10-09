# Brief — T010
Base: d6bfb67
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-block-clues/.specs/007-session-lanes-and-fresh-descriptions
Approved: 2026-09-27 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml`

## Phase 3 — Review fixes
Goal: a split folder keeps every minute and every event link it had, and a description written for a short block is refreshed once the block grows past the thresholds, however gradually.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green, and `bash scripts/verify-inference.sh` exits 0.

## Your task
- [ ] T010 descriptions remember the length they were written for (B11) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/db.rs, rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_carry.rs, rust/crates/worklog-core/src/estimate.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core described` — after: T009


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
orchestrator re-runs verify: and runs flow tick T010.

## Contract for T010 (orchestrator, binding)
DEFECT     `infer.rs::persist_blocks` gates the description carry with `infer_carry::keeps_description(prior, new, estimated_by)`, where `prior` is the claimed prior ROW's span. The scheduler rebuilds + estimates every 15 min (`worklog day`), so a block grows in steps below both thresholds (30→45 is exactly 1.5×, then +15 each run) and a description written for 30 min survives to 3 h. Also: the estimator merges adjacent same-ticket blocks into one wide row (`estimate::merge_block_into`); on the next rebuild the first piece is compared with the wide row (30 vs 120) and loses its description every cycle.
FIX        Remember the length a description was written for, and compare against THAT.
           1. New nullable column `blocks.described_seconds INTEGER` — the block's wall-clock span (ended_at − started_at, in seconds, NOT the rounded `duration_seconds`) at the moment the estimator wrote its description. Add it to `rust/crates/worklog-core/sql/schema.sql` (with a one-line comment like its neighbours) and an idempotent `ALTER TABLE blocks ADD COLUMN described_seconds INTEGER` migration in `db.rs` following the `exported_at` pattern (check column exists first), wired where the other block migrations run.
           2. Estimator: both writes that set `description` + `estimated_by = 'claude_p'` in estimate.rs (around lines 550 and 775) also set `described_seconds` = the block row's wall-clock span in seconds. Nothing else in estimate.rs changes (the invoke loop may be edited by another branch — stay out of it).
           3. `persist_blocks`: carry `described_seconds` exactly like `description` (select it into `CarryRow`, write it on insert, and set it to NULL whenever the description is dropped). When calling `keeps_description`, use `prior = (0, described_seconds / 60)` when the carried row has `described_seconds`, else the prior row's span as today. `new` stays the new block's span. `keeps_description`'s signature and rule stay unchanged.
           4. A NULL `described_seconds` (old rows, manual text) behaves exactly as today.
NEVER      (1) Never drop a `manual` description. (2) Never clear `tempo_worklog_id`, `exported_at` or `jira_issue` carry. (3) Never change which prior row is claimed. (4) Never edit files outside files:. (5) Existing tests stay green untouched, including `reinference_preserves_tempo_id_and_description` and `grown_block_is_described_again`.
TESTS      (fail first against current code):
           - infer.rs tests `gradual_growth_is_described_again`: persist a 30-min block; set `description='short', estimated_by='claude_p', described_seconds=1800`; re-persist the same start growing +15 min at a time (45, 60, 75 …). Assert the description is still there at 45 min (diff 15, 45·2=90 ≤ 30·3) and gone (description and described_seconds NULL) by 60 min (diff 30 vs the described 30).
           - infer.rs tests `merged_piece_keeps_its_description` (name must contain "described" OR put it in a module path that matches the verify filter `described`; simplest: name it `merged_piece_keeps_described_text`): two same-ticket pieces 10:00–10:30 and 11:30–12:00; the first carries description + described_seconds=1800 while its stored row was widened to 10:00–12:00 (simulate `merge_block_into` by UPDATE of ended_at/duration); rebuild → the 10:00–10:30 piece keeps its description.
           - estimate.rs test: after `commit_block_estimate` (or the batch path) on a 45-min block, `described_seconds` = 2700.
           - db.rs: migrating an old schema without the column adds it (follow an existing migration test if one exists).
           All three new test names should contain `described` so the verify filter runs them.
COMMIT     Message e.g. `fix(infer): compare a description against the length it was written for`. No task ids. Nothing under .specs/.
