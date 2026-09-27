# Brief — T009
Base: e6e1a11
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/prep-block-clues/.specs/007-session-lanes-and-fresh-descriptions
Approved: 2026-09-27 by user
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml`

## Phase 3 — Review fixes
Goal: a split folder keeps every minute and every event link it had, and a description written for a short block is refreshed once the block grows past the thresholds, however gradually.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core` — green, and `bash scripts/verify-inference.sh` exits 0.

## Your task
- [ ] T009 split folders keep their evidence and event links (B10) — files: rust/crates/worklog-core/src/infer_lanes.rs, rust/crates/worklog-core/src/infer_lane_tags.rs, rust/crates/worklog-core/src/infer_lane_tags_test.rs, rust/crates/worklog-core/src/infer_evidence.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core infer_lane_tags` — after: CHK001


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
orchestrator re-runs verify: and runs flow tick T009.

## Contract for T009 (orchestrator, binding)
CONTEXT    `infer_lanes::build_project_blocks` now owns minutes by FOLDER first (`keyed_by(.., lane_folder)` → owner_runs/fold/merge_by_evidence), then `infer_lane_tags::split_runs_by_tag` divides each folder run into sub-runs owned by lane keys (`folder` or `folder#Cust`). Bucketing then accepts an event into a run only when `lane_key(e) == owner`, and each run's bucket must pass `infer_evidence::has_evidence_floor`.
DEFECTS    (E) An untagged event of a split folder (session-less shell/commit/reflog, or a prompt from an unresolved session) whose minute lies in a `folder#Cust` sub-run matches no run and — because a run covers the minute — is discarded: linked into no block. Before tagging it was linked. Estimator clues, invoice text and block details read those links.
           (F) `fold_weak_sub_runs` can leave a single sub-run whose own key has < MIN_EVIDENCE_MINUTES; its bucket then fails the floor and the whole folder run's block vanishes, although the folder's events together passed. Probe: split folder, Cust1 prompt 09:00, Cust2 prompt 09:03 → untagged: one 5-min block; tagged: none.
           (D) `infer_lane_tags::merge_touching` duplicates `infer_evidence::merge_touching_same_owner` (make that one `pub(crate)` and reuse it; delete the copy). Reuse `infer_evidence`'s `Run` alias the same way if practical.
FIX        Judge linking and evidence at the FOLDER-run level, exactly as an untagged day does, and only then divide:
           1. Bucket every non-calendar event into the folder-level runs by `lane_folder(e) == folder` (the pre-tagging rule; minutes owned by another folder still build nothing; leftovers unchanged).
           2. Apply `has_evidence_floor` to the FOLDER run's bucket (pre-tagging rule). A folder run that passes is never dropped afterwards.
           3. For a passing folder run with ≥ 2 sub-runs, hand each event to the sub-run covering its minute when the event is untagged (`lane_tag == None`) or its lane key equals that sub-run's owner. A tagged event whose minute lies in a DIFFERENT customer's sub-run joins the nearest sub-run with its own owner if one exists in this folder run, else the covering sub-run is skipped for it and it stays unlinked (keep no-block-mixes-customers true: a block may hold one customer's tagged events plus untagged ones, never two customers' tagged events). Every sub-run becomes a block (`span_block`) — none is dropped for evidence; `own` is that sub-run owner's events by lane key, falling back to the folder's events.
           4. A folder run with one sub-run behaves as today's single run.
           Keep `build_project_blocks` readable; put the division logic in `infer_lane_tags.rs`. infer_lanes.rs MUST stay ≤ 400 lines.
NEVER      (1) Never change behaviour for days with no lane_tag — all existing tests stay green untouched. (2) Never let a tag change which folder owns a minute or drop a folder run that passes the floor untagged. (3) Never put two customers' tagged events in one block. (4) Never edit files outside files:. (5) No lint suppressions.
TESTS      In infer_lane_tags_test.rs (fail first, against current code):
           - `untagged_folder_events_stay_linked`: folder split into Cust1 (09:00–09:40) and Cust2 (09:40–10:20) sessions plus a second folder elsewhere; an untagged `shell` event at 09:21 and an untagged `github_commit`-like event at 10:01 in the split folder, and a prompt from an unresolved session (no tag) at 09:30. Assert all three event ids are linked into some block, and the set of linked event ids equals the untagged build's set for that folder's events... except none; i.e. every folder event linked untagged is linked tagged.
           - `sparse_split_keeps_its_block`: the 09:00 Cust1 / 09:03 Cust2 probe (with and without a second folder) → the tagged build has the same folder minutes as the untagged build (≥ 1 block).
           - Keep all existing infer_lane_tags tests green; `split_keeps_folder_minutes` must still hold.
           Then `bash scripts/verify-inference.sh` must still exit 0 (it reads a copy of the live db; never write to the live db).
COMMIT     Message e.g. `fix(infer): split folders keep their evidence and event links`. No task ids. Nothing under .specs/.
