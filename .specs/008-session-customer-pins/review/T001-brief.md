# Brief — T001
Base: 9704664
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/session-customer-pins/.specs/008-session-customer-pins
Approved: 2026-09-27 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml`

## Phase 1 — Pins exist
Goal: Claude can pin a customer to its session from the command line, and a new session on the same branch inherits it.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_pins` and `cargo test --manifest-path rust/Cargo.toml -p worklog-cli pin` — green.

## Your task
- [ ] T001 [P] pin store and customer lookup (B1, B2) — files: rust/crates/worklog-core/sql/schema.sql, rust/crates/worklog-core/src/session_pins.rs, rust/crates/worklog-core/src/session_pins_test.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/src/billing_registry.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core session_pins`


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

## Contract for T001 — pin store and customer lookup
NAMES      pin table, pin source, pin row, customer lookup, default branch (§1, verbatim).
CALLS      `pub fn pin(conn: &Connection, registry: &Registry, session_id: &str, cwd: &Path, name: &str, at: DateTime<Utc>, branch: Option<&str>) -> Result<SessionPin, PinError>` where `pub enum PinError { UnknownCustomer { known: Vec<String> }, Other(anyhow::Error) }`; `pub fn pin_for_branch(conn, folder: &str, branch: &str) -> Result<Option<SessionPin>>` (latest by from_at, never for a default branch); `pub fn pins_for_sessions(conn, session_ids: &[String]) -> Result<Vec<SessionPin>>`.
THE FIVE   (1) NEVER invent a field or name not in §1. (2) NEVER store an unresolved name. (3) NEVER bump SCHEMA_VERSION — idempotent CREATE only. (4) NEVER edit files outside files:. (5) NEVER abbreviate inside an identifier.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T001.
