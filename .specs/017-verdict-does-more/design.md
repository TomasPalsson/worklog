# Design: Verdict does more

## 1. Contract files

- Rust: `rust/crates/worklog-core/src/verdict_contract.rs` (`use crate::verdict_contract::*`)
- Web: `web/lib/verdict_contract.ts`
- Both are orchestrator-owned. A missing type or constant is an escalation.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| decision log | `verdict_decisions` table, `DecisionRow` | contract + schema.sql | history, audit, feedback |
| ranking | `Ranking { ranking, abstain, agreed }` | contract | guess list, scores |
| order check | `Ranking.agreed` | contract | consistency, double-check |
| shortlist | `SHORTLIST_MAX` options | contract | candidates list, narrowed |
| line check | `LineCheck` on `tempo_line_texts.check_status` | contract | validation, lint |
| review | `ReviewLine`, `GET /review` | contract | audit, inbox |
| auto-send | `AUTO_SEND_KEY`, `tempo_line_texts.auto_sent_at` | contract | autosync, auto-sync |
| Verdict state | `VerdictState`, `GET /verdict/status` | contract | helper status, health |

Instants are RFC3339 UTC strings. Day bucketing and the 17:00 hour use `$WORKLOG_TZ` as elsewhere.

## 2. Trust boundaries

| boundary | untrusted input | parse | failure |
|---|---|---|---|
| Verdict HTTP response | JSON | `serde_json` into `Ranking` | any error → `Ok(None)` for that one call, counted as not answering |
| `verdict_decisions.ranking` / `events.verdict_ranking` | JSON text | `serde_json` into `Ranking` | bad row → treated as no ranking, never a panic |
| envfile keys | `on`/`off` text | exact match | anything else → the key's default |

## 3. Schema (T001 only)

- `verdict_decisions(id INTEGER PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','ticket','line_text')), source TEXT NOT NULL CHECK(source IN ('verdict','owner')), subject TEXT NOT NULL, state_json TEXT NOT NULL, options TEXT NOT NULL /*JSON array*/, ranking TEXT /*JSON Ranking*/, chosen TEXT, previous TEXT, decided_at TEXT NOT NULL)`; indexes on `(kind, subject)` and `(decided_at)`. Never named in `purge.rs` (FR-09).
- `events.verdict_ranking TEXT` (JSON `Ranking`), `tempo_line_texts.check_status TEXT CHECK(check_status IN ('passed','needs_look'))`, `.auto_sent_at TEXT`, `.confirmed_at TEXT`, `.send_error TEXT`: each via its own `ensure_*` in `db.rs`.

## 4. Verdict HTTP shape (T002 server, T003 client)

- `POST /classify` body `{"state": <json>, "options": [<id>...], "examples": {<id>: [<text>...]}}` (`examples` optional) → `{"ranking": [{"id","probability"}...≤3], "abstain": <f64>, "agreed": <bool>}`. `options` length 1 → `ranking` of that one id, `abstain` 1.0, `agreed` false.
- `POST /match` unchanged; line checks use it.
- `split_groups` / `merge_groups` and their self-test are deleted.

## 5. Module boundaries

- `verdict_decisions.rs` · may import: contract, rusqlite · exports: `record`, `list_since`, `examples_for`, `latest_for`, `unchecked_count`
- `verdict_supervisor.rs` · may import: contract, envfile, verdict · exports: `spawn`, `status`, `set_enabled`, `retry`, `uv_args`
- `routing_shortlist.rs` · exports: `shortlist(conn, event, day) -> Vec<String>`
- `ticket_verdict.rs` · exports: `ticket_options(conn, block) -> Vec<String>`, `pick(conn, classifier, block) -> Result<Option<String>>`
- `line_check.rs` · exports: `check(client, line_text, ticket_summary) -> Result<Option<LineCheck>>`
- `scorecard.rs` · exports: `run(conn, classifier, apply: bool) -> Result<Scorecard>`
- `auto_send.rs` · exports: `ready_lines`, `run_if_due`, `review_lines`, `confirm`
- Anything not listed is a bug. New modules each get a sibling `*_test.rs` via `#[path]`, like `routing.rs`, to stay under the 400-line guard.

## 6. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite connection | daemon `with_conn` / CLI `db::open` | `&Connection` argument | every module above |
| Verdict client | `VerdictClassifier::new()` | `&dyn Classifier` argument | routing, ticket_verdict, scorecard |
| Verdict `/match` client | `verdict::match_texts` | function call | line_check |
| envfile | `envfile::read` / `upsert` | direct call | supervisor, auto_send, settings |

## 7. Deliberately duplicated

- `SHORTLIST_MAX`, `EXAMPLE_CHARS_*` are restated as literals in `verdict_server.py`; do not make Python read Rust.
- Ticket-option and project-option shortlists are separate functions; do not merge them into one generic shortlist.

## 8. Decisions

- In the context of routing, facing the measured drop from 96% to 72% correct at 25 options, we chose a ≤6 shortlist and rejected grouping, accepting that a project idle for 14 days needs a pin or link to be offered. Makes hard: `routing_shortlist.rs`, `verdict_server.py`.
- In the context of filing, facing order flips (~4.5%), we chose to act only when `agreed` is true, accepting two model passes per event. Makes hard: `verdict_server.py`, `routing.rs`.
- In the context of readiness, facing `ticket_origin = 'auto'` being shared by Verdict and the cloud model, we chose the decision log as the proof of a Verdict pick and rejected a new origin value, keeping the CHECK constraint unchanged. Makes hard: `auto_send.rs`.

## Contract for T001 — decision log and schema
CONTRACT  rust/crates/worklog-core/src/verdict_contract.rs — import from it.
CALLS     record(&Connection, &DecisionRow) -> Result<i64>; list_since(&Connection, DecisionKind, since: &str) -> Result<Vec<DecisionRow>>; examples_for(&Connection, folder: &str, limit: usize) -> Result<Vec<String>> (titles of events the Owner corrected TO folder, newest first); latest_for(&Connection, DecisionKind, subject: &str) -> Result<Option<DecisionRow>>; unchecked_count(&Connection, day: &str) -> Result<u32>
THE FIVE  see §5 of design reference.

## Contract for T002 — Verdict server
SHAPE     §4 above, verbatim. Keep `--self-test` runnable without rlcd.

## Contract for T003 — Verdict client and filing rule
CALLS     trait Classifier { fn classify(&self, state: &Value, options: &[String], examples: &BTreeMap<String, Vec<String>>) -> Result<Option<Ranking>>; }; verdict::match_texts(query: &str, texts: &[String]) -> Result<Vec<bool>>; decide accepts iff agreed && top ∈ options && top ≥ abstain×margin && top ≥ second×ratio.

## Contract for T004 — Verdict supervisor
CALLS     GET /verdict/status?day= → VerdictStatus; POST /verdict/enabled {"on": bool}; POST /verdict/retry. uv found via UV_CANDIDATES then PATH.

## Contract for T009 — auto-send and review
CALLS     GET /review → [ReviewLine]; POST /review/confirm {"day","jira_issue"?}; ready iff ticket origin manual|event, or auto with latest_for(Ticket, block id) source Verdict and chosen = jira_issue; and check_status passed or text_origin manual.
