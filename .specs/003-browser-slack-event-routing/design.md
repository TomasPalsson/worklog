# Design — Browser + Slack event routing

Spec: spec.md · Contract file: `rust/crates/worklog-core/src/routing_contract.rs` (compiled, clippy-clean at handoff; orchestrator-owned — no task edits it)

## 1. Contract file + language

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| project key | `folder: String` | folders | routing_contract.rs `LabelRequest.folder` | project_name, label_name, tag |
| label origin | `LabelOrigin` | — | routing_contract.rs | source_kind, provenance, method |
| hard rule | `Rule` / `routing_rules` | rules | routing_contract.rs | mapping, alias, pin (pin = billing) |
| rule kind | `RuleKind` | — | routing_contract.rs | match_type |
| heartbeat | `Heartbeat` | heartbeats | routing_contract.rs | ping, beacon, visit |
| routed event | `RoutedEvent` | routed events | routing_contract.rs | browser_event, slack_event |
| guess | `Guess` | — | routing_contract.rs | prediction, result |
| unsorted | `folder: None` on a firefox/slack event | — | — | unlabeled, pending, inbox |
| model helper | `LAYA_ADDR` process | — | routing_contract.rs | sidecar server, ml service |

- Instants are UTC (`DateTime<Utc>` in Rust, ISO-8601 TEXT in SQLite) per CLAUDE.md; work-hours are evaluated in local time via `tz::day_offset()`.
- Event ids are `i64` (SQLite rowid). Confidence is `f64` in 0.0–1.0.
- Core is sync (`rusqlite`, `reqwest::blocking`); daemon handlers wrap core calls in `with_conn` / `spawn_blocking` as today.
- TS mirror of `RoutedEvent`, `Rule`, `LabelOrigin`, `RuleKind` lives in `web/lib/types.ts` (T010 owns it); field names identical (snake_case).

## 2. Trust boundaries

| boundary | untrusted input shape | parse fn | failure granularity |
|---|---|---|---|
| add-on → `POST /browser/heartbeat` | JSON `Heartbeat`, `Origin` header | axum `Json<Heartbeat>` + `browser_ingest::ingest_heartbeat` | whole request: 403 bad origin, 400 bad body, 200 `{"stored":false,"reason":…}` when filtered |
| Slack Web API response | JSON messages | serde structs in `collectors/slack.rs` | per message: skip + count in `CollectReport` |
| Laya helper response | JSON `{"choice","confidence"}` | `laya::LayaClassifier::classify` | per event: parse error or unreachable → `Ok(None)` (unsorted) |
| model output `folder` | string | `routing` checks it is in the options list | not in options → treat as `None` |
| `routing_rules` / `events` rows | TEXT kind/origin | `RuleKind::parse` / `LabelOrigin::parse` | unknown value → skip row, `warn!` |
| envfile work hours | `Mon-Fri 09:00-17:00` | `browser_ingest::WorkHours::parse` | invalid → fall back to `DEFAULT_WORK_HOURS`, `warn!` |

## 3. Error taxonomy

Daemon keeps its single `ApiError` enum (`daemon.rs:389`). T003 adds exactly one variant: `Forbidden(String)` → 403 `{"error": msg}` (same body shape as the others). Mapping: bad origin → Forbidden; malformed body / unknown folder / bad day → BadRequest; unknown event or rule id → NotFound; anything else → Internal. Core functions return `anyhow::Result`; the daemon maps.

## 4. Module boundaries

- `routing_contract` · layer 0 · may import: std, chrono, serde, serde_json, anyhow · exports: listed items only
- `browser_ingest` · layer 1 · may import: routing_contract, repo, models, tz, envfile · exports: `WorkHours`, `IngestOutcome`, `ingest_heartbeat`
- `collectors::slack` · layer 1 · may import: routing_contract, repo, models, secrets, http, collectors::CollectReport · exports: `SlackAuth`, `collect`, `collect_with`
- `laya` · layer 1 · may import: routing_contract, http · exports: `LayaClassifier`, `SERVER_SCRIPT`
- `routing` · layer 2 · may import: routing_contract, billing (work_prefix only), billing_registry, envfile, models · exports: `RouteStats`, `project_keys`, `load_pending`, `decide`, `commit_labels`, `route_day`, `label_event`, `list_rules`, `delete_rule`, `routed_for_day`
- `daemon`, `worklog-cli` · layer 3 · may import anything above
- Anything not listed is a bug.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite `Connection` | daemon `Shared` / CLI `db::open` | `&Connection` arg | every core fn |
| HTTP client | `crate::http::client()` | `collect_with(.., client)` / `LayaClassifier::with_client` | slack, laya |
| settings | `envfile::read(KEY)` | read at call time | browser_ingest, routing |
| clock | caller passes `now`/`ts` | arg | browser_ingest (tests inject) |
| migrations | T001 only | `db.rs` `SCHEMA_VERSION` 10 → 11 | — no other task touches schema |

## 6. Deliberately duplicated

- Slack HTTP retry/timeout: use `crate::http::client()` defaults; do not add a retry helper.
- Domain extraction from a URL lives in `routing` only (for rule matching); the add-on sends the full URL and does not pre-extract.
- UI date formatting: reuse `web/lib/format.ts`; do not add a new formatter.

## 7. Decisions

1. In the context of `routing`, facing "blocks are rebuilt on re-inference (infer.rs:428)", we chose to write the label into `events.project_path = <work_prefix>/<folder>` plus `label_origin`/`label_confidence`, rejected a block-id link, to keep infer/personal/billing unchanged, accepting that a named project and a real folder with the same name are the same project. Makes hard: routing.rs, infer.rs.
2. In the context of `browser_ingest`, facing "infer credits every non-calendar event 2 min (infer.rs:59)", we chose one event per heartbeat minute (`source_id` = `<minute UTC ISO>`), rejected merged long events, to reuse gap clustering unchanged, accepting ~480 rows per workday. Makes hard: browser_ingest.rs.
3. In the context of `infer::load_day_events`, facing "NULL project_path events inherit the neighbour's project (infer.rs:284)", we chose to exclude firefox/slack events whose `label_origin IS NULL`, rejected letting them inherit, so unsorted time never inflates a block. Makes hard: infer.rs.
4. In the context of `route_day`, facing "the daemon must not hold the sqlite mutex across a slow model call (PR #41)", we chose three steps `load_pending` (lock) → `decide` (no lock) → `commit_labels` (lock), mirroring `estimate::prepare/invoke/commit_block_estimate`, rejected one locked loop. Makes hard: routing.rs, daemon.rs.
5. In the context of `POST /browser/heartbeat`, facing "any web page can POST to 127.0.0.1", we chose to require `Origin` starting with `moz-extension://`, rejected a shared secret, since pages cannot forge `Origin`. Makes hard: daemon.rs, extension.
6. In the context of the model helper, facing "the product ships no Python (install.sh, release.yml)", we chose an optional loopback process started by `worklog laya serve` via `uv run --with laya`, rejected ONNX-in-Rust, accepting a first-run model download. Makes hard: laya.rs, cli.rs.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|
| `Classifier` trait with one production impl | Tests must run without Python/Laya | Mirrors the existing `estimate::ModelInvoker` seam; calling HTTP directly makes routing untestable offline |

## Contract for T001 — Schema v11 and module stubs
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
NAMES      events.container TEXT NULL · events.label_origin TEXT NULL · events.label_confidence REAL NULL · routing_rules(id INTEGER PK, kind TEXT NOT NULL, pattern TEXT NOT NULL, folder TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(kind, pattern))
CALLS      db::migrate (idempotent ALTERs like db.rs:84-161); SCHEMA_VERSION 11; create empty modules browser_ingest.rs, routing.rs, laya.rs, collectors/slack.rs (doc comment only) and register them
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Heartbeat ingest
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
NAMES      Heartbeat · SOURCE_FIREFOX · PERSONAL_CONTAINER · WORK_HOURS_KEY · DEFAULT_WORK_HOURS
MODULE     browser_ingest.rs · layer 1 · exports: WorkHours, IngestOutcome, ingest_heartbeat
CALLS      `pub struct WorkHours` · `WorkHours::parse(s: &str) -> Result<WorkHours>` · `WorkHours::contains(&self, ts: DateTime<Utc>, offset: FixedOffset) -> bool` · `pub enum IngestOutcome { Stored(i64), Filtered(&'static str) }` · `pub fn ingest_heartbeat(conn: &Connection, hb: &Heartbeat, hours: &WorkHours, offset: FixedOffset) -> Result<IngestOutcome>` — filters incognito, container == PERSONAL_CONTAINER, outside hours; upserts via `repo::upsert_event` with source_id = ts truncated to minute ISO, title = hb.title, details = hb.url, then sets events.container
THE FIVE   (1)–(5) as in T001.

## Contract for T003 — Daemon routes
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
CALLS      `POST /browser/heartbeat` (Origin must start `moz-extension://` else ApiError::Forbidden) → `{"stored":bool,"reason":string|null}` · `GET /days/:day/routed` → `Vec<RoutedEvent>` · `POST /events/:id/label` body LabelRequest → `RoutedEvent` · `GET /routing/rules` → `Vec<Rule>` · `POST /routing/rules/:id/delete` · `GET /routing/status` → `{"last_heartbeat":string|null,"last_slack":string|null,"laya_reachable":bool}` · `/settings` GET/POST also reads/writes WORK_HOURS_KEY and ROUTE_THRESHOLD_KEY via envfile
THE FIVE   (1)–(5) as in T001.

## Contract for T004 — Slack collector
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
NAMES      SOURCE_SLACK · SLACK_TOKEN_KEY
CALLS      `pub fn collect(conn: &Connection, auth: &SlackAuth, since: NaiveDate, until: NaiveDate) -> Result<CollectReport>` · `pub fn collect_with(conn, auth, since, until, client: &reqwest::blocking::Client, base_url: &str) -> Result<CollectReport>` · event: source_id = `<channel_id>:<ts>`, title = channel name, details = message text, started_at = message ts UTC · `CollectTarget::Slack` in cli.rs, included in `All`
THE FIVE   (1)–(5) as in T001.

## Contract for T005 — Router core
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
CALLS      `pub fn project_keys(conn: &Connection) -> Result<Vec<String>>` (billing_folder_map folders ∪ dirs under work_prefix, sorted, deduped) · `pub struct Pending { pub event: RoutedEvent, pub options: Vec<String>, pub state: serde_json::Value }` · `pub fn load_pending(conn: &Connection, day: NaiveDate) -> Result<(Vec<(i64, String)>, Vec<Pending>)>` (rule hits first, then model candidates) · `pub fn decide(pending: &[Pending], classifier: &dyn Classifier, threshold: f64) -> Vec<(i64, Guess)>` · `pub fn commit_labels(conn: &Connection, rule_hits: &[(i64, String)], guesses: &[(i64, Guess)]) -> Result<RouteStats>` · `pub fn route_day(conn, day, classifier, threshold) -> Result<RouteStats>` (CLI convenience = the three steps) · `pub fn label_event(conn, id: i64, req: &LabelRequest) -> Result<RoutedEvent>` · `list_rules`, `delete_rule(conn, id) -> Result<bool>`, `routed_for_day(conn, day) -> Result<Vec<RoutedEvent>>` · infer.rs `load_day_events` excludes firefox/slack rows with label_origin IS NULL · container narrowing: `Registry::customer_in_text(container)` → exactly one customer → options = folders pinned to it
THE FIVE   (1)–(5) as in T001.

## Contract for T006 — Laya client and helper
CONTRACT   rust/crates/worklog-core/src/routing_contract.rs — import from it.
CALLS      `pub struct LayaClassifier` · `LayaClassifier::new() -> Self` (LAYA_ADDR, 10 s timeout) · `impl Classifier for LayaClassifier` → POST `http://LAYA_ADDR/classify` `{"state":…,"options":[…]}` → `{"choice":str,"confidence":f64}`; connect error → Ok(None) · `pub const SERVER_SCRIPT: &str = include_str!("../templates/laya_server.py")` · CLI `worklog laya serve` (foreground: writes script to data dir, execs `uv run --with laya python <script>`, binds 127.0.0.1 only) and `worklog laya status`
THE FIVE   (1)–(5) as in T001.

## Contract for T010 — Web types and actions
CONTRACT   web/lib/types.ts mirrors routing_contract.rs field-for-field.
CALLS      daemon.ts: `daemonRoutedForDay(day)`, `daemonLabelEvent(id, folder, always)`, `daemonRoutingRules()`, `daemonDeleteRule(id)`, `daemonRoutingStatus()` · actions.ts server actions wrapping each, revalidating the day path
THE FIVE   (1)–(5) as in T001.
