# Brief — T002
Base: 095120c
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
- [ ] T002 session customer tagging (B2, B3) — files: rust/crates/worklog-core/src/session_customers.rs, rust/crates/worklog-core/src/session_customers_test.rs, rust/crates/worklog-core/src/lib.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_customers` — after: T001


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

## Contract for T002 — session customer tagging
CONTRACT   rust/crates/worklog-core/src/session_customers.rs (new) + `mod session_customers;` in lib.rs.
CALLS      `pub(crate) fn tag_sessions(events: &mut [InferEvent], registry: &crate::billing_registry::Registry)`
           Group events with `session_id` by (`lane_folder(e)`, session). Text = that session's titles + jira_issue joined by "\n". Customer = `registry.customer_in_text(&text)`.
           Per folder: if resolved sessions name ≥ 2 distinct customers, set `lane_tag = Some(customer)` on every event of each resolved session. Otherwise tag nothing. Unresolved sessions and session-less events are never tagged.
THE FIVE   (1) NEVER guess a customer — only `customer_in_text`. (2) NEVER tag when a folder has < 2 distinct customers. (3) NEVER touch `project_path`. (4) NEVER edit files outside `files:`. (5) Tests build a `Registry` directly, no DB.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T002.

## Orchestrator notes (binding)
- `lane_tag` and `infer_lanes::lane_folder` already exist (previous task). `Registry::customer_in_text` is in `billing_registry.rs` (0 or 2+ hits → `None`). Build test registries as `Registry { customers: vec![...], folders: vec![] }` with real `Customer` values.
- Register as a private module: `mod session_customers;` in lib.rs, alphabetical position. Tests live in `session_customers_test.rs`, wired with `#[cfg(test)] #[path = "session_customers_test.rs"] mod tests;` like `infer_evidence.rs` does.
- Required tests (exact names): `two_customers_in_one_repo_tag_both` (folder `~/Desktop/Work/vitinn-infra`, session A titles name customer 1, session B titles name customer 2 → every A event tagged customer 1, every B event tagged customer 2); `one_customer_tags_nothing` (two sessions, both resolve to the same customer, or one resolves and one doesn't → all `lane_tag` stay `None`). Also: `unresolved_session_stays_untagged` (three sessions in one folder: A→cust1, B→cust2, C names none or both → C untagged, A and B tagged), and a test that a session-less event in a split folder stays untagged and that `project_path` is unchanged on every event. Plus: two different folders each with one customer → nothing tagged (per-folder rule, not per-day).
- Text for a session = its events' `title`s and `jira_issue`s joined with "\n" (skip `None`s).
- Commit message describes the change (e.g. `feat(infer): tag a repo's sessions by customer when they name different ones`). No task ids.
