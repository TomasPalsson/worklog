# Brief — T009
Base: 55d848c
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/logged-overview/.specs/014-no-tempo-daily-log-overview
Approved: 2026-10-04 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && cd web && bun test`

## Phase 2 — The Logged section
Goal: the Owner opens /logged and sees month, week and day views of real Tempo time, auto-refreshed, with short days flagged and dismissable.
Independent test: `cd web && bun test components/Logged app/actions-logged lib/format` — green with the daemon mocked.

## Your task
- [ ] T009 Logged pages and components per ui.md: /logged redirect, /logged/month/[month], /logged/week/[monday], /logged/day/[day], LoggedFetch, DismissDay (B9, B10) — files: web/app/logged/page.tsx, web/app/logged/month/[month]/page.tsx, web/app/logged/week/[monday]/page.tsx, web/app/logged/day/[day]/page.tsx, web/components/LoggedMonth.tsx, web/components/LoggedWeek.tsx, web/components/LoggedDay.tsx, web/components/LoggedFetch.tsx, web/components/DismissDay.tsx, web/components/LoggedHeader.tsx, web/components/LoggedEntries.tsx, web/components/Logged.test.tsx, web/app/globals.css — verify: `cd web && bun test components/Logged.test.tsx && bun run typecheck` — after: T004, T005, T006, CHK008


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

## Contract for T009 — Logged views
CONTRACT   web/lib/logged_contract.ts — import from it.
NAMES      as T003
MODULE     web/app/logged/** pages; web/components/Logged*.tsx, DismissDay.tsx · may import: actions-logged, format.ts, logged_contract
CALLS      server pages call getLogged; LoggedFetch (client) calls refreshLogged(from,to) on mount + refresh button, then router.refresh(); DismissDay calls dismissLoggedDay
DUPLICATE  none
THE FIVE   (1)–(5) as above.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T009.
