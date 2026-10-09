# Brief — T008
Base: ccb6bb0
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-event-routing/.specs/003-browser-slack-event-routing
Approved: 2026-09-23 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test) && bun test extension/firefox`

## Phase 3 — Firefox add-on
Goal: an installable add-on that reports focused, active, work-hours tab time and can be paused.
Independent test: `bun test extension/firefox` — green with no daemon running.

## Your task
- [ ] T008 [P] Firefox add-on: heartbeat logic, container/incognito skip, idle, pause popup (B11) — files: extension/firefox/manifest.json, extension/firefox/background.js, extension/firefox/heartbeat.js, extension/firefox/heartbeat.test.js, extension/firefox/popup.html, extension/firefox/popup.js, extension/firefox/README.md — verify: `bun test extension/firefox`


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

## Contract for T008 — Firefox add-on
CONTRACT   body = `Heartbeat` in rust/crates/worklog-core/src/routing_contract.rs, JSON field-for-field: `{"ts": ISO-8601 UTC, "url": string, "title": string, "container": string|null, "incognito": bool}`
CALLS      `fetch("http://127.0.0.1:9323/browser/heartbeat", {method:"POST", headers:{"Content-Type":"application/json"}, body})` from the background script (the browser adds the `moz-extension://` Origin; never set it by hand) · Manifest V2, `browser_specific_settings.gecko.id = "worklog@tomasari.is"`, permissions: tabs, idle, alarms, storage, contextualIdentities, cookies, `http://127.0.0.1:9323/*` · `browser.alarms` every 1 min · `browser.idle.setDetectionInterval(120)` · container name via `browser.contextualIdentities.get(tab.cookieStoreId)` (default container → null) · pause flag in `browser.storage.local` key `paused`, toggled from popup.html
MODULE     heartbeat.js exports pure fns only (no `browser.*`): `shouldSend({paused, incognito, containerName, idleState, windowFocused}) -> boolean` (false when paused, incognito, containerName === "Personal", idleState !== "active", or no focused window) · `buildHeartbeat(tab, containerName, now: Date) -> Heartbeat` · background.js wires `browser.*` to them
RULES      Work hours are enforced by the daemon (single editable setting); the add-on does not duplicate them. A failed POST is dropped silently (daemon down = no data, never a retry queue). README.md: load via about:debugging, build with `web-ext build`, sign with `web-ext sign --channel unlisted`.
THE FIVE   (1)–(5) as in T001.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T008.
