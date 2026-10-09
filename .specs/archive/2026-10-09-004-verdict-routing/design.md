# Design — Verdict routing

Spec: spec.md · Contract file: `rust/crates/worklog-core/src/routing_contract.rs` (T001 writes the declarations below verbatim; no later task edits it)

## 1. Contract file + language

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| model helper | `CLASSIFIER_ADDR` process, `worklog verdict serve` | — | routing_contract.rs | laya, sidecar, ml service |
| classifier client | `verdict::VerdictClassifier` | — | verdict.rs | LayaClassifier, VerdictClient |
| guess | `Guess { folder, confidence, runner_up, abstain }` | — | routing_contract.rs | prediction, result |
| abstain margin | `abstain_margin: f64` / `WORKLOG_ROUTE_ABSTAIN_MARGIN` | — | routing_contract.rs | threshold, min_confidence |
| runner-up ratio | `runner_up_ratio: f64` / `WORKLOG_ROUTE_RUNNER_UP_RATIO` | — | routing_contract.rs | second_ratio, gap |
| helper reachable | `classifier_reachable: bool` | — | daemon `/routing/status` | laya_reachable, model_up |

Declarations T001 puts in `routing_contract.rs` (replacing `ROUTE_THRESHOLD_KEY`, `DEFAULT_ROUTE_THRESHOLD`, `LAYA_ADDR`):

```rust
pub const CLASSIFIER_ADDR: &str = "127.0.0.1:9324";
pub const ABSTAIN_MARGIN_KEY: &str = "WORKLOG_ROUTE_ABSTAIN_MARGIN";
pub const DEFAULT_ABSTAIN_MARGIN: f64 = 1.20;
pub const RUNNER_UP_RATIO_KEY: &str = "WORKLOG_ROUTE_RUNNER_UP_RATIO";
pub const DEFAULT_RUNNER_UP_RATIO: f64 = 1.10;
/// Both ratios must lie in this closed range.
pub const RATIO_RANGE: (f64, f64) = (1.0, 5.0);
pub const VERDICT_GIT_URL: &str = "git+https://github.com/Heman10x-NGU/Verdict-open-jev";
pub const VERDICT_GIT_REV: &str = "30f15564821626ca5c1ad5b2638c4eb7078787dd";
pub const VERDICT_MODEL_REPO: &str = "heman10x/rlcd-modernbert-151m";
pub const VERDICT_MODEL_REVISION: &str = "8af2496eb63c7fa66d7d234e1f62629380030eb4";

pub struct Guess { pub folder: String, pub confidence: f64, pub runner_up: f64, pub abstain: f64 }
/// `confidence` = the winner's probability. All three are model probabilities in 0.0–1.0.

pub struct RouteRule { pub abstain_margin: f64, pub runner_up_ratio: f64 }
```

- Core stays sync (`reqwest::blocking`); the daemon wraps calls in `spawn_blocking` as today.
- TS mirror in `web/lib/types.ts`: `abstain_margin: number`, `runner_up_ratio: number` on `SettingsView`/`SettingsUpdate`; `classifier_reachable: boolean` on routing status.

## 2. Trust boundaries

| boundary | untrusted input shape | parse fn | failure granularity |
|---|---|---|---|
| helper `POST /classify` response | JSON `{"choice": str, "probability": f64, "runner_up": f64, "abstain": f64}` | `VerdictClassifier::classify` | per event: unreachable, non-200, bad JSON → `Ok(None)` (unsorted) |
| helper `choice` | string | `routing::decide` checks it is in the options list | not an option → unsorted |
| envfile ratios | decimal text | `daemon::configured_route_rule()` | unparsable or out of `RATIO_RANGE` → default, `warn!` |
| `POST /settings` ratios | JSON number | daemon settings handler | out of `RATIO_RANGE` → 400 `{"error": msg}`, nothing saved |

## 3. Error taxonomy

No new variants. Helper failures never surface as errors: they are `Ok(None)`. Settings range errors use the existing `ApiError::BadRequest` → 400 `{"error": msg}`.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| `RouteRule` | `daemon::configured_route_rule()` from the envfile | by value | `routing::decide(pending, classifier, rule)` |
| helper script | `verdict::SERVER_SCRIPT` (`include_str!("../templates/verdict_server.py")`) | written to `<data>/verdict_server.py` | `worklog verdict serve` |
| model files | the helper, `huggingface_hub.snapshot_download(VERDICT_MODEL_REPO, revision=VERDICT_MODEL_REVISION, local_dir=<data>/verdict-model, allow_patterns=[config.json, model.safetensors, model.onnx, tokenizer.json, tokenizer_config.json, calibrator.json])` | env `WORKLOG_VERDICT_MODEL_DIR`, `WORKLOG_VERDICT_MODEL_REPO`, `WORKLOG_VERDICT_MODEL_REVISION` set by the CLI | `DecisionEngine(model_name_or_path=<dir>, device="cpu")` |

## 6. Deliberately duplicated

- The pinned revisions live as Rust constants and reach Python only as env vars set by the CLI — do not hard-code them again in the script.

## 7. Decisions

- In the context of the helper, facing Verdict's 24-option cap, we chose one batched `evaluate` over groups of ≤24 and take winner / runner-up across all groups by raw probability, abstain = the highest group abstain; rejected a final round between group winners, to match the method measured at 4/4 right, 0 wrong, accepting that per-group normalisation makes cross-group scores approximate. Makes hard: `verdict_server.py`.
- In the context of `routing::decide`, facing uncalibrated absolute confidence, we chose `accepts = folder ∈ options && confidence >= abstain * abstain_margin && confidence >= runner_up * runner_up_ratio`; rejected a single threshold. Makes hard: `routing.rs`, daemon settings, web settings.
- In the context of install, facing no PyPI release and a broken download script checksum, we chose `uv run --with <VERDICT_GIT_URL>@<VERDICT_GIT_REV> --with huggingface_hub python <script>`; rejected the repo's `download_artifacts.py`. Makes hard: `cli.rs` serve.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|

## Contract for T001 — Contract and rename
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — write §1's declarations verbatim; remove ROUTE_THRESHOLD_KEY, DEFAULT_ROUTE_THRESHOLD, LAYA_ADDR.
NAMES      model helper, classifier client, guess, abstain margin, runner-up ratio, helper reachable (§1 rows, banned synonyms included)
MODULE     rust/crates/worklog-core/src/verdict.rs replaces laya.rs (git mv); lib.rs `pub mod verdict;`
CALLS      `routing::decide(pending: &[Pending], classifier: &dyn Classifier, rule: RouteRule) -> Vec<(i64, Guess)>` — keep today's threshold body adapted to compile; T003 writes the real rule. `daemon::configured_route_rule() -> RouteRule` returns defaults for now. CLI subcommand renamed `laya` → `verdict`. `/routing/status` field → `classifier_reachable`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Verdict helper and client
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it. A type you need that is not there is an escalation, never a local declaration.
NAMES      classifier client `VerdictClassifier`; guess `Guess { folder, confidence, runner_up, abstain }`
CALLS      helper: GET /health → 200; POST /classify `{"state", "options"}` → `{"choice", "probability", "runner_up", "abstain"}`; empty options → 400. Script reads model dir/repo/revision from env (§5); `--self-test` runs the pure group-split/merge function on fakes without importing rlcd. Client: `VerdictClassifier::classify(&self, state: &Value, options: &[String]) -> Result<Option<Guess>>`, 10 s timeout.
DUPLICATE  pins reach the script only via env vars.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — Relative filing rule
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import `Guess`, `RouteRule`.
CALLS      `routing::decide(pending: &[Pending], classifier: &dyn Classifier, rule: RouteRule) -> Vec<(i64, Guess)>`; accept iff `options.contains(folder) && confidence >= abstain * rule.abstain_margin && confidence >= runner_up * rule.runner_up_ratio`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T004 — Verdict CLI
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import `CLASSIFIER_ADDR`, `VERDICT_*`.
CALLS      `worklog verdict serve`: write `verdict::SERVER_SCRIPT` to `<data>/verdict_server.py`, run `uv run --with {VERDICT_GIT_URL}@{VERDICT_GIT_REV} --with huggingface_hub python <script>` with env `WORKLOG_VERDICT_MODEL_DIR=<data>/verdict-model`, `WORKLOG_VERDICT_MODEL_REPO`, `WORKLOG_VERDICT_MODEL_REVISION`. `worklog verdict status [--json]` → `{"reachable": bool}` / "verdict reachable at …" or "verdict not reachable — run `worklog verdict serve`". Build the uv args in a pure fn so a test can assert the pins.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T005 — Daemon ratio settings
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import `ABSTAIN_MARGIN_KEY`, `RUNNER_UP_RATIO_KEY`, defaults, `RATIO_RANGE`, `RouteRule`.
CALLS      `daemon::configured_route_rule() -> RouteRule` reads both envfile keys (bad/out-of-range → default + `warn!`). `GET /settings` returns `abstain_margin`, `runner_up_ratio` in place of `route_threshold`; `POST /settings` accepts them, out of `RATIO_RANGE` → `ApiError::BadRequest`, nothing saved. Routing call sites pass `configured_route_rule()`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T006 — Web ratio settings
NAMES      `abstain_margin`, `runner_up_ratio` (SettingsView/SettingsUpdate), `classifier_reachable`; UI text says "Verdict", never "Laya".
CALLS      two number inputs (step 0.01, min 1, max 5) replace the route-threshold input; a blank input sends nothing for that field.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T007 — Exact repo or path mention files by rule
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import `LabelOrigin`, `DEFAULT_ABSTAIN_MARGIN` (now 1.20). A type you need that is not there is an escalation.
CALLS      private `fn named_project(row: &EventRow, options: &[String]) -> Option<String>` in routing.rs: scan `row.title` and `row.details` for `github.com/<org>/<key>` and `Desktop/Work/<key>` where `<key>` is followed by `/`, `|`, `>`, `)`, whitespace, a quote, `` ` `` or end of text, and `<key>` is in `options` (exact, case-sensitive). Exactly one distinct key → Some(key); zero or two+ → None. In `load_pending`, after `matching_rule` and before building a `Pending`, a `Some(key)` goes to `rule_hits` (so it is committed with origin `rule`, never sent to the classifier). Update B1's test values so they still clear the new 1.20 default (e.g. abstain 0.060) — the spec amendment changed the default; do not weaken any other assertion.
TESTS      exact_repo_mention_files_by_rule (PR link → vitinn-infra, classifier never called — assert with a classifier that panics), path_mention_files_by_rule (`cd ~/Desktop/Work/vitinn-infra`), two_named_projects_is_no_match, prefix_is_not_a_match (`vitinn-infra-old` must not match `vitinn-infra`), unknown_repo_is_no_match.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.
