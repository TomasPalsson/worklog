# Brief — T007
Base: 291818c
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
- [ ] T007 session text includes the owner's prompt text (B8) — files: rust/crates/worklog-core/src/infer.rs, rust/crates/worklog-core/src/infer_allocations_db_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core prompt_text_names_the_session_customer` — after: T003


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
orchestrator re-runs verify: and runs flow tick T007.

## Contract for T007 (orchestrator, binding)
WHY        On real data every `claude_turn` row is titled "prompt"; the owner's prompt text is in `events.raw_json` as `clues_contract::RawRecord::ClaudePrompt { session_id, text }` (serde, `tag = "kind"`, e.g. `{"kind":"claude_prompt","session_id":"…","text":"…"}`). `session_customers::tag_sessions` reads each event's `title` + `jira_issue`, so today it never resolves a real session.
CHANGE     In `rust/crates/worklog-core/src/infer.rs`, `load_day_events` also selects `raw_json`; the row mapper `infer_event_row` sets `title = Some(text)` when `source == "claude_turn"` AND `raw_json` deserialises (`serde_json::from_str::<crate::clues_contract::RawRecord>`) to `RawRecord::ClaudePrompt { text, .. }`. Otherwise `title` is the DB title exactly as today (malformed/NULL raw_json → DB title, never an error). Update the `title` field's doc line on `InferEvent` to say it carries a prompt's text for `claude_turn`.
NEVER      (1) Never put tool/helper output (`ClaudeTool` etc.) into title — those mention every customer. (2) Never change which rows are loaded. (3) Never add a field to `InferEvent`. (4) Never edit files outside files:. (5) Keep the loader function body within its current size budget — the mapper is where the logic goes.
TEST       In `infer_allocations_db_test.rs`: `prompt_text_names_the_session_customer` — like `two_customer_sessions_split_into_separate_blocks` but the `claude_turn` events are titled literally "prompt" and carry the customer name only inside `raw_json` (`serde_json::to_string(&RawRecord::ClaudePrompt{..})`, set on the `Event` before `upsert_event`). Assert ≥2 blocks and no block mixes the two sessions. Must FAIL before the change (one block). Add a unit-level assertion (same file or infer.rs tests) that a `claude_turn` with malformed raw_json keeps title "prompt", and a `shell` row with a raw_json keeps its own DB title.
COMMIT     Message describes the change (e.g. `feat(infer): read a prompt's text for session customer tagging`). No task ids. Nothing under .specs/.
