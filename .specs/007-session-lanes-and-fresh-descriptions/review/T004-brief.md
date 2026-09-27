# Brief — T004
Base: 3fee6ae
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
- [ ] T004 drop stale descriptions on big change (B5, B6) — files: rust/crates/worklog-core/src/infer_carry.rs, rust/crates/worklog-core/src/infer.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_carry` — after: T003


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

## Contract for T004 — drop stale descriptions on big change
CALLS      `pub(crate) fn keeps_description(prior: (i64, i64), new: (i64, i64), estimated_by: Option<&str>) -> bool` in infer_carry.rs.
           true when `estimated_by == Some("manual")`; else false when `abs(new_len - prior_len) >= 30` or `max_len * 2 > min_len * 3` (growth and shrink alike); else true.
           In infer.rs persist: when false, write `description = None` and `estimated_by = None` for that block (ticket, tempo id, exported_at carry as today).
THE FIVE   (1) NEVER drop a `manual` description. (2) NEVER change which prior row is claimed. (3) NEVER edit files outside `files:`.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T004.

## Orchestrator notes (binding)
- The carry happens in `infer.rs::persist_blocks` (the loop that picks `carry: Option<&CarryRow>` then copies `description`/`estimated_by`). `CarryRow` has `started_at`/`ended_at` ISO strings; the new block has `b.started_at`/`b.ended_at` (`DateTime<Utc>`). Convert both to minutes since epoch (`ts.timestamp().div_euclid(60)`; parse the ISO strings with `chrono::DateTime::parse_from_rfc3339`) and call `crate::infer_carry::keeps_description(prior, new, carry.estimated_by.as_deref())`. When false: `description = None`, `estimated_by = None`. `jira_issue`, `tempo_worklog_id`, `exported_at` and which row is claimed stay exactly as today.
- `estimated_by` values in use: `'manual'` (Owner), `'claude_p'` (estimator), `'gap'` (estimator failure). Only `'manual'` is always kept.
- Required unit tests in `infer_carry.rs`'s `mod tests` (exact names first): `grown_block_drops_description` (30→182 min → false), `manual_description_always_kept` (30→182 with `Some("manual")` → true), plus: grow by exactly 30 (60→90 → false, abs diff ≥ 30), grow >1.5× but <30 (10→16 → false: 16*2=32 > 10*3=30), shrink >1.5× (60→25 → false), just-under (60→89 → true: diff 29, 89*2=178 ≤ 180), and unchanged (true). Expected values come from the rule, computed by hand.
- Required persist-level test in `infer.rs` `mod tests`: `grown_block_is_described_again` — persist a 30-min block, set its `description='old'`, `estimated_by='claude_p'`, `tempo_worklog_id='555'`; re-persist the same start as a 182-min block → stored description NULL, estimated_by NULL, tempo_worklog_id still '555'. And the existing `reinference_preserves_tempo_id_and_description` must stay green unchanged.
- Commit message describes the change (e.g. `feat(infer): re-describe a block whose length changed a lot`). No task ids. Nothing under .specs/.
