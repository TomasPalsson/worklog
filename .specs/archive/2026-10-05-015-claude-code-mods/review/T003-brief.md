# Brief — T003
Base: 0fa51e2
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
- [ ] T003 Mod skeleton, shared lib and registrar stubs (FR-03b, FR-07, FR-10b, FR-04 timeout) — files: mods/worklog/.claude-plugin/plugin.json, mods/worklog/hooks/hooks.json, mods/worklog/hooks/register.tsx, mods/worklog/hooks/lib.ts, mods/worklog/hooks/lib.test.ts, mods/worklog/hooks/status.ts, mods/worklog/hooks/ticket.ts, mods/worklog/hooks/command.tsx — verify: `claude plugin validate mods/worklog && claude plugin test mods/worklog`


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

## Contract for T003 — mod skeleton and lib
CONTRACT   mods/worklog/hooks/contract.ts — import from it.
NAMES      §1 table, verbatim.
MODULE     mods/worklog/hooks/lib.ts · exports exactly:
CALLS      `daemonGet<T>($: EngineInterface, path: string): Promise<DaemonResult<T>>`
           `daemonPost<T>($: EngineInterface, path: string, body: unknown): Promise<DaemonResult<T>>`
           `daemonToday($: EngineInterface): Promise<DaemonResult<LocalDay>>`
           `workContext($: EngineInterface, cwd: string): Promise<WorkContext>`
           `ticketFromBranch(branch: string | undefined): string | undefined`
           `lastWorkday(today: LocalDay): LocalDay` · `mondayOf(day: LocalDay): LocalDay`
           `workSeconds(blocks: readonly Block[]): number` · `formatHours(seconds: number): string` (`2h30`, `0h05`)
           `reviewRequest(blockId: number, action: ReviewAction): { path: string; body: unknown }`
           register.tsx: `export const register: Register = on => { registerStatus(on); registerTicket(on); registerCommand(on) }`
           stubs: status.ts / ticket.ts / command.tsx each `export function registerX(on: On): void {}`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T003.

## References (read before writing)
- Mod API docs: /private/tmp/claude-501/bundled-skills/2.1.289/12aa4596cdc27ed5a06a5dab73257082/plugin-authoring/ — SKILL-level notes in reference.md, working examples in examples/, full types in types/claude-code.d.ts (grep by name). The engine interface type is `EngineInterface`, the `on` type is `On`, the module type is `Register`, all from `'claude-code'`.
- Manifest: `.claude-plugin/plugin.json` = `{ "name": "worklog", "version": "0.1.0", "description": "<one line>" }`; `hooks/hooks.json` = `{ "modules": ["./register.tsx"] }`.
- Tests: `import { test, expect, mock } from 'claude-code/testing'`; body is `($, on) => …`; stub `$` calls by event name beneath the plugin, e.g. `on('http.fetch', …)` → `{ value: { status, ok, headers, text } }`, `on('process.run', …)` → `{ value: { exitCode, stdout, stderr } }`; `mock.clock(on)`, `mock.store(on)`, `mock.env(on, vars)`. Grep `claude-code.d.ts` for `mock` and `http.fetch` to confirm shapes.
- `$.http.fetch(url, init)` has no timeout option: race it against `$.clock.after(2000, …)`.
- `$.process.run(argv, { cwd })` has no shell; it rejects on spawn failure — catch it.
- Home dir for the "~/Desktop/Work/" rule: `$.env.get('HOME')`.
