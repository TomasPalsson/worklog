# Brief — T009
Base: 0a5c223
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/mods-worklog/.specs/015-claude-code-mods
Approved: 2026-10-04 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && claude plugin test mods/worklog`

## Phase 2 — the mod
Goal: inside a Claude Code terminal session the Owner sees hours, reminders and the ticket, and can review today with `/worklog`.
Independent test: `claude plugin validate mods/worklog && claude plugin test mods/worklog` — green with Rust untouched.

## Your task
- [ ] T009 Review pane layout: bordered, header with day and totals, aligned rows, plain hotkey buttons (FR-13) — files: mods/worklog/hooks/command.tsx, mods/worklog/hooks/command.test.tsx — verify: `claude plugin validate mods/worklog && claude plugin test mods/worklog` — after: T008


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

## Contract for T009 — review pane layout
MODULE     mods/worklog/hooks/command.tsx · only `reviewPane` and `blockLabel` change. Keep every element `key`, `hotkey`, `onPress`/`onSelect`/`onSubmit`/`onCancel` behaviour, the Select (arrow-key navigation), and the request sent per action exactly as now.
LAYOUT     Engine props: ~/.local/share/worklog/claude-mod/.claude-plugin/types/claude-code/index.d.ts (`BoxProps` ~l.841, `ButtonProps` ~l.1000 — `plain` draws `p: label`, `TextProps` ~l.12011).
           Outer `<Box flexDirection="column" borderStyle="round" borderDimColor paddingX={1}>`.
           Header `<Box justifyContent="space-between">`: left `<Text bold>{day}</Text>` (the blocks' `day`, else omit), right `<Text dimColor>work {formatHours(workSeconds(blocks))} · personal {formatHours(sum of duration_seconds of is_personal blocks)}</Text>`. Reuse lib `formatHours`/`workSeconds`.
           Rows: the Select, `marginY={1}` around it. `blockLabel(block)` = `${HH:MM start}–${HH:MM end}  ${formatHours(duration).padStart(5)}  ${(jira_issue ?? '—').padEnd(12)}  ${is_personal ? 'personal  ' : ''}${description truncated to 40 chars with '…'}` trimmed at the end. start/end from `started_at`/`ended_at` `.slice(11, 16)` as today.
           Actions `<Box gap={3}>`: the four Buttons get `plain`; the personal Button's label reads `work` when the block is personal, else `personal` (key stays `personal`).
           Editing row `marginTop={1}`, `gap={2}`; cancel Button `plain dimColor`.
           Error: `<Text color="red">` directly under the header. Empty day: header-less bordered box with `<Text dimColor>Nothing to review today.</Text>`.
TESTS      Update only assertions the layout breaks; add one test that the Select labels are aligned (e.g. label of a ticketless personal 35-min block equals `11:44–12:19   0h35  —             personal` per the formula) and one that the personal Button label reads `work` for a personal block. Existing behaviour tests (hotkeys, POST bodies) must stay and pass unchanged.
THE FIVE   as above.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T009.
