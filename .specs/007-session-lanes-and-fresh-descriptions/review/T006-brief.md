# Brief — T006
Base: d9a9234
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
- [ ] T006 Friday real-data check covers splits and block count — files: scripts/verify_inference_report.py — verify: `bash scripts/verify-inference.sh` — after: T004, T005, T007, T008


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
orchestrator re-runs verify: and runs flow tick T006.

## Contract for T006 (orchestrator, binding)
CONTEXT    `scripts/verify-inference.sh` rebuilds 2026-09-21..27 on a COPY of the live db and calls `scripts/verify_inference_report.py <db> <days…>`, which prints per-day stats and asserts. All current assertions pass (Friday 8.47 h, 18 blocks, 2 under 10 min). The day rebuild now tags a repo's Claude sessions with their customer (resolved from the session's prompt text in `events.raw_json` → `{"kind":"claude_prompt","text":…}` on `claude_turn` rows, matched against `billing_customers.name`/`aliases` (newline-separated) with a case-insensitive whole-word match — exactly one customer hit → that customer, 0 or 2+ → unknown) and splits the repo's blocks per customer only when ≥ 2 customers appear in that repo that day.
ADD        Only in `scripts/verify_inference_report.py` (stdlib only):
           1. Assert 2026-09-25 has ≤ 18 blocks (spec §5).
           2. Mirror the session→customer resolution in a small function (per (repo folder, session_id): join its `claude_turn` prompt texts; whole-word, case-insensitive match of each customer's name and aliases; one hit → customer). Repo folder = the path segment after `/Desktop/Work/` (worktrees collapse to it).
           3. Print, for 2026-09-25, each vitinn-infra block's start–end and the resolved customers of the sessions linked to it (via `block_events` → `events.session_id`).
           4. Assert: no 2026-09-25 block links sessions resolving to two different customers ("no mixed-customer block").
           5. Assert: vitinn-infra on 2026-09-25 has at least one block with a Sjúkra-resolved session and at least one with an APRÓ-resolved session, in different blocks ("the split fired").
           Keep the existing assertions and output unchanged. Update the module docstring to mention the new checks.
NEVER      Never open the live db (only argv[1]). Never loosen an existing threshold. Never edit other files.
VERIFY     `bash scripts/verify-inference.sh` exits 0 with the new assertions printed as [OK]. Also prove the mixed-customer check can fail: run the report once against a scratch copy where you artificially relink one Sjúkra-session event into an APRÓ block (a throwaway sqlite UPDATE on a copy under $TMPDIR — never the live db, never committed) and show it prints [FAIL].
COMMIT     Message e.g. `test(verify): Friday check covers customer splits and block count`. No task ids. Nothing under .specs/.
