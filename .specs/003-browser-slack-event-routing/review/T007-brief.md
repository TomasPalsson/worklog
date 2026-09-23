# Brief — T007
Base: b6d3771
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-event-routing/.specs/003-browser-slack-event-routing
Approved: 2026-09-23 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test) && bun test extension/firefox`

## Phase 2 — Routing
Goal: every browser/Slack event gets a project from a rule, a confident guess, or waits in unsorted — and labelled ones join the right block.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core routing laya` — green with the model helper absent.

## Your task
- [ ] T007 Route before inference in `collect all` and `POST /infer` — files: rust/crates/worklog-cli/src/cli.rs, rust/crates/worklog-core/src/daemon.rs — verify: `cargo test --manifest-path rust/Cargo.toml` — after: T003, T004, T006


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

## Contract for T007 — Route before inference (orchestrator-written)
Existing API to reuse (do NOT re-implement): `worklog_core::routing::{load_pending, decide, commit_labels, route_day, RouteStats}`,
`worklog_core::laya::LayaClassifier::new()` (a connection failure = "no guess", never an error),
`daemon.rs::configured_route_threshold()` (currently private — make it `pub` so cli.rs can reuse it; do not copy it).

1. `rust/crates/worklog-core/src/daemon.rs` — `run_infer` (POST /infer):
   before `infer::load_day_events`, route the day in THREE steps so the sqlite mutex is never held across the model call
   (design decision 4, PR #41): `with_conn(load_pending)` → `tokio::task::spawn_blocking(decide(&pending, &LayaClassifier::new(), configured_route_threshold()))` with NO lock →
   `with_conn(commit_labels + the existing load_day_events/build_blocks/persist_blocks)`.
   Keep `InferResponse`'s existing fields unchanged (web reads them).
2. `rust/crates/worklog-cli/src/cli.rs` — `cmd_collect`: when `target` is `CollectTarget::All`, after all collectors ran and before the output,
   call `routing::route_day(&conn, d, &LayaClassifier::new(), configured_route_threshold())` for every day `d` from `since` to `today` inclusive.
   A routing error for a day is reported with `style::info` (non-json) and does not fail the collect. JSON output shape stays unchanged.
   Do NOT touch `cmd_infer` or `cmd_day`.

Edge cases: laya helper absent → every unmatched event stays unsorted (label_origin NULL), still Ok. A day with zero pending events makes no classifier call.

Tests (inline `#[cfg(test)]`, hermetic — never depend on a real laya process; only use events a hard rule matches, so `decide` gets no pending model candidates):
- daemon.rs: `infer_routes_rule_hits_before_building_blocks` — seed a firefox event (label_origin NULL) and a matching Domain rule for folder X, POST /infer,
  assert the event's `label_origin` is `rule` and a block for that day has project_path `<work_prefix>/X`. Must fail before the change.
- cli.rs: only if cmd_collect is already unit-tested with a temp WORKLOG home; otherwise skip the CLI test and say so.
Acceptance: `cargo test --manifest-path rust/Cargo.toml` green; `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` clean.
Commit message: `feat(routing): T007 route before inference in collect all and POST /infer`.
