# Brief — T003
Base: 3277638
Feature: /Users/tomas/Desktop/Projects/worklog/.claude/worktrees/embedding-stats/.specs/023-topic-atlas
Approved: 2026-10-11 by user (pre-approved: /goal directive, /flow:next --finish)
Spec: spec.md
Design: design.md
Route: dispatch
Test: `cargo test --manifest-path rust/Cargo.toml && (cd web && bun test)`

## Phase 1 — Honest, named themes on a map
Goal: /stats/topics returns centered-vector topics with keywords, cached AI names, map points, per-day metrics, river and highlights; the web shows them as the Topic atlas.
Independent test: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic && cargo test --manifest-path rust/Cargo.toml -p worklog-core daemon_stats && (cd web && bun test components/stats)` — Rust math, naming cache, route and every atlas component together.

## Your task
- [ ] T003 [P] c-TF-IDF keywords + MMR examples (B2) — files: rust/crates/worklog-core/src/topic_terms.rs, rust/crates/worklog-core/src/topic_terms_test.rs — verify: `cargo test --manifest-path rust/Cargo.toml -p worklog-core topic_terms` — after: T002, T014


## Tests
verify: names one test; the spec states more. List every boundary and negative case it
states for this task (exactly-at and just-past each limit, empty, missing, sibling/prefix,
error path) and write ONE assertion per item in RED, even when verify: runs a single test.
A guard delegated to a shared helper still needs tests at the call site that fail for the
helper's plausible wrong versions. Beside each assertion name the wrong implementation it
catches (swallow the error, startswith with no separator, > for >=, a running total from
zero); an assertion that would still pass against that version is not a RED test.

## Prior art
Prior art: none found


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

Third-party APIs: training data is older than the installed package. For each
third-party symbol your diff calls that `git grep` finds nowhere else in the repo, read
the installed version and the symbol's source or types before you write the call, then
put one line in your report: `api: <pkg>@<version from the lockfile or the installed
package.json/METADATA> <path:line in the installed types or source | doc URL>`. When no
installed source or doc can be read, write `api: <pkg>@<version> UNVERIFIED` and say so.
A symbol already used elsewhere in the repo needs none: copy that call's shape.

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
no sleeps (wait on a condition with a timeout that fails with a message), no snapshot
as the only assertion. Never skip, weaken, or delete a test. A mock returns the
full shape of the real response, not only the fields this test reads. A method only
tests call lives in test utilities, not the production class.
Test through the public interface; mock only at system boundaries (network, clock,
randomness, third-party APIs), never your own modules; verify through the interface,
never through a side channel such as querying the database the code just wrote.
Scratch checks verify; they do not become committed test files unless the task or repo convention asks.
A new external call gets its own named function (getUser, createOrder) beside the
existing ones, and a test mocks that function. A mock never branches on URL or
endpoint. Never refactor an existing generic client inside this task; report it instead.

Write a high-quality, general-purpose solution. Do not hard-code values or special-case
the test inputs. If the task is unreasonable or a test is wrong, say so in the report
instead of working around it.

Before reporting done, run `scripts/slop-check --base <base>` and either fix each
finding or justify it in one line in the report.
```

The slop-check command above lives at an absolute path; run it as:
/Users/tomas/Desktop/Projects/flow/plugins/flow/skills/no-slop/scripts/slop-check --base 3277638

## Contract for T003 — c-TF-IDF keywords + MMR examples
USAGE      `let terms: Vec<Vec<String>> = topic_terms::keywords(&docs_per_topic, TOPIC_KEYWORDS); let ex: Vec<usize> = topic_terms::representatives(&centered_vecs_of_topic, &descriptions_of_topic, NAMING_EXAMPLES);`
CALLS      `pub fn tokenize(text: &str) -> Vec<String>`: strip `\b[A-Z][A-Z0-9]+-\d+\b`, lowercase, split on non-alphanumeric (Unicode: keep ð þ æ ö á é í ó ú ý), drop tokens < 3 chars and pure digits, drop stopwords (inline const: the ask.rs English+Icelandic list plus: work, worked, working, task, tasks, update, updated, fix, fixed, add, added, review, check, make, made, using, use, new, into, with, from, this, that, over, also, och — keep it short). `pub fn keywords(docs_per_topic: &[Vec<String>], top: usize) -> Vec<Vec<String>>`: c-TF-IDF `tf(t,c)/len(c) · ln(1 + A / f(t))`, A = mean tokens per topic, f(t) = count of t over all topics; ties by term asc. `pub fn representatives(vecs: &[Vec<f32>], descriptions: &[String], n: usize) -> Vec<usize>`: MMR — score = cos(v, centroid) − 0.5·max cos(v, picked); skip a description equal (case-insensitive, trimmed) to a picked one; deterministic ties by lower index.
TESTS      Jira keys stripped; Icelandic tokens kept intact; stopwords gone; a term unique to one topic outranks a term common to all; MMR never returns two equal descriptions; returns ≤ n indices.
THE FIVE   (same five as T002)

## Report
End your report with a Seen: section, apart from the searched: receipt: one line per
defect you noticed in any file you read (re-read each file you edited end to end),
even outside your task, as Seen: <file>:<line> — <what>, or the literal Seen: none.
Do not fix them.

## Rules
Touch only the paths in files:. Failing test first, verify it fails, minimal
implementation, verify it passes, commit. Do not tick the box — the
orchestrator re-runs verify: and runs flow tick T003.
