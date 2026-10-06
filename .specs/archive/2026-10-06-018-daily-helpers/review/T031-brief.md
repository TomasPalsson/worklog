# Brief — T031
Base: 1ad0d29
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/spec-018-daily-helpers/.specs/018-daily-helpers
Approved: 2026-10-06 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Phase 5 — After spec 017: already-in-Tempo and the 17:00 recap
Goal: a line equal to a hand entry is not sent twice, and the 17:00 run ends with a recap.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib tempo_match && cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib recap && (cd web && bun test components/RecapBanner)`.

## Your task
- [ ] T031 Recap built inside the 17:00 run (FR-43, FR-45, B7) — files: rust/crates/worklog-core/src/recap.rs, rust/crates/worklog-core/src/recap_test.rs, rust/crates/worklog-core/src/auto_send.rs, rust/crates/worklog-core/src/lib.rs, rust/crates/worklog-core/sql/schema.sql — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core --lib recap` — after: T029


## Tests
verify: names one test; the spec states more. List every boundary and negative case it
states for this task (exactly-at and just-past each limit, empty, missing, sibling/prefix,
error path) and write ONE assertion per item in RED, even when verify: runs a single test.
A guard delegated to a shared helper still needs tests at the call site that fail for the
helper's plausible wrong versions. Beside each assertion name the wrong implementation it
catches (swallow the error, startswith with no separator, > for >=, a running total from
zero); an assertion that would still pass against that version is not a RED test.


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

The slop-check command above lives at an absolute path; run it as:
/Users/tomas/Desktop/Projects/flow/plugins/flow/skills/no-slop/scripts/slop-check --base 1ad0d29

## Contract for T031 — Recap built inside the 17:00 run

- `auto_send::run_if_due(conn, local, enabled, send)` (src/auto_send.rs:122) returns `()` and stores outcomes on `tempo_line_texts` (auto_sent_at, send_error). After its per-line loop (before the final `Ok(())`, ~L175) call `recap::build_and_store(conn, today, work_hours)` guarded once per day by a meta key (`purge::meta_get/meta_set`, e.g. `recap_built_for`=day), so later 15-min ticks do not rebuild. No new timer (FR-45).
- `recap::build(conn, day, window) -> Recap`: sent = today's lines with auto_sent_at set; held_back = `ready_lines`-rejected or planned-but-failed lines with a plain reason (send_error text, "already in Tempo", "text needs a look", "ticket not confirmed", "no text"); coverage = block seconds (non-ignored) inside the work-hours window ÷ window seconds, rounded to 0..=100; gaps = `timeline::day_gaps` with min `RECAP_MIN_GAP_SECONDS`, clipped to the window, minus recorded breaks, longest first, top `RECAP_TOP_GAPS`.
- Work-hours window: `browser_ingest::WorkHours::parse` has no public start/end accessor — add a small public accessor there or parse `HH:MM-HH:MM` from `routing_contract::WORK_HOURS_KEY` (default `DEFAULT_WORK_HOURS`). Day offset `tz::day_offset()`.
- Store: `recaps(day TEXT PRIMARY KEY, json TEXT NOT NULL, built_at TEXT NOT NULL)` and `recap_breaks(day TEXT, started_at TEXT, ended_at TEXT, PRIMARY KEY(day, started_at))` in schema.sql. `recap::latest(conn) -> Option<Recap>`, `recap::apply_gap(conn, day, gap_started_at, action: GapAction)`: Personal → insert a block over the gap with is_personal=1, estimated_by='manual'; Break → insert into recap_breaks; PickTicket → `ticket_log::log_time` (manual, that key) over the gap. Each resolves the gap (rebuild + store the recap); only PickTicket adds billable time (FR-44).

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T031.
