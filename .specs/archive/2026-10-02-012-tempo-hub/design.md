# Design — Tempo hub

Spec: spec.md · seams: shared wire types (Rust ↔ web), the hub error shape, the new tables, the lock-free call split.

## 1. Contract file + language

**Rust**: `rust/crates/worklog-core/src/tempo_hub_contract.rs` (exists, compiles). Import from it; never redeclare. **Web**: T011 mirrors it field-for-field (snake_case keys) into `web/lib/types.ts`; no other web file declares these shapes.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| task | `TaskRow` / `TasksResponse` | contract | card row, issue row |
| transition | `Transition` | contract | workflow step, move |
| status category | `StatusCategory` (`new`/`indeterminate`/`done`) | contract | state, bucket |
| draft | `TicketDraft` | contract | suggestion, proposal |
| pulled worklog | `PulledWorklog` (client) → `RemoteWorklog` (stored) | contract | tempo entry |
| owner | `WorklogOwner` (`worklog`/`outside`) | contract | source, origin |
| required day | `RequiredDay` | contract | schedule row, target |
| close-out | `WeekCloseout` / `CloseoutDay` | contract | week summary, report |
| hub error | `HubError` | contract | — |

- Days are `YYYY-MM-DD` strings in the configured day offset (`tz::day_offset()`); a week starts on `monday: NaiveDate`. Seconds are `i64`.
- Tempo ids are `String` everywhere (stringify Tempo's integer; compare with `blocks.tempo_worklog_id` as text).
- Core functions are sync (`rusqlite::Connection`, blocking `reqwest::blocking::Client`); daemon handlers are async and use `with_conn` / `spawn_blocking`.

## 2. Trust boundaries

| boundary | untrusted shape | parse | failure granularity |
|---|---|---|---|
| Jira transitions/status JSON | `{transitions:[{id,name,to:{name,statusCategory:{key}}}]}` | serde structs private to `collectors/jira.rs` | whole call fails |
| Tempo worklogs JSON | `{results:[{tempoWorklogId:int,issue:{id},timeSpentSeconds,startDate,description}],metadata:{next?}}` | private serde in `collectors/tempo.rs` | whole pull fails; DB untouched |
| Tempo schedule JSON | `{results:[{date,requiredSeconds}]}` | same | whole pull fails |
| Model output | `{comment:string, suggested_transition_id:string\|null}` | `task_draft::draft_with` | foreign id → `None`; empty comment → error |
| HTTP path `:key` | any string | daemon: `^[A-Z][A-Z0-9_]*-[0-9]+$` | 400 |
| `monday` | any string | daemon: parses and must be a Monday | 400 |
| comment text | any string | trimmed, 1..=`COMMENT_MAX_CHARS` chars | 400 |

## 3. Error taxonomy

`HubError` in the contract is closed. Client fns return `anyhow::Result` and, on a non-2xx, `Err(anyhow::Error::new(HubError::Upstream{service:"Jira"|"Tempo",status,body}))`. Daemon handlers map with `e.downcast_ref::<HubError>()`: `InvalidInput` → `ApiError::BadRequest`, `NotFound` → `ApiError::NotFound`, `Upstream` → **502** `{"error": "<Display>"}` (a new arm or a direct `(StatusCode::BAD_GATEWAY, Json(json!({"error": msg})))`). Anything else → `ApiError::Internal`. Web shows `error` verbatim.

## 4. Module boundaries

- `tempo_hub_contract` · may import: serde · exports: the types above.
- `collectors::jira` (+T002 fns) · may import: contract, http, repo, models · exports: `list_transitions_with`, `transition_with`, `fetch_status_with`, `add_comment_with`.
- `collectors::tempo` (+T003/T005) · may import: contract, tempo_remote · exports: `list_worklogs_with`, `user_schedule_with`, `resolve_account_id` (now `pub`).
- `tempo_remote`, `task_board`, `week_closeout`, `task_draft` · may import: contract, tempo_lines, tempo_line_contract, repo, estimate (`ModelInvoker` only, task_draft) · never import each other, never do HTTP.
- `daemon::daemon_tasks`, `daemon::daemon_week` · `#[path]` submodules of `daemon.rs` like `daemon_tempo_lines` · may import anything above.
- web: `web/lib/daemonHub.ts` (daemon calls) → `web/app/actions-hub.ts` (Server Actions) → components. Components never import `lib/daemon*`.

Anything not listed is a bug.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite conn | daemon state | `with_conn(state, |c| …)` closure, **never** across HTTP/AI | core fns as `&Connection` |
| HTTP client | `crate::http::client()` inside `spawn_blocking` | `&Client` | `*_with` fns |
| Jira auth | `JiraAuth::from_secrets()` | value | jira fns |
| Tempo auth | `TempoAuth::from_secrets()` | value | tempo fns |
| AI | `estimate::build_regenerate_invoker(line_text::LINE_TEXT_THINKING_TOKENS)` in `spawn_blocking`; model `line_text::LINE_TEXT_MODEL` | `&dyn ModelInvoker` | `task_draft::draft_with` |
| clock | handler computes `today` from `Utc::now()` + `tz::day_offset()` | `NaiveDate` arg | core fns (never read the clock) |
| migration number | T001 only | `SCHEMA_VERSION = 18` | — |

## 6. Deliberately duplicated

- Each daemon handler parses `:key` and `monday` itself — do not add a shared extractor type.
- `task_board` and `week_closeout` each sum `tempo_lines::lines_for_day` over seven days — do not build a shared week-lines helper.
- Web: `WeekCloseout` copies ActionBar's confirm-twice state (4 s revert) — do not extract a hook.

## 7. Decisions

- In `/tasks`, facing "0 Jira calls on load", we chose reading the cached `jira_tickets` and rejected a live JQL per load, accepting that status is as fresh as the last Refresh Jira. Makes hard: `task_board.rs`.
- In refresh, facing stale rows that never leave the cache, we chose `status_category='done'` for non-external rows the refresh did not return and rejected deleting them (deletion loses `issue_id`, which sync needs). Makes hard: `repo.rs`, `collectors/jira.rs`.
- In read-back, facing "never double-log", we chose replacing a week's `tempo_remote_worklogs` rows on each pull and a sync guard on `(day, issue_id)` for groups with no Tempo id, rejected a merge into blocks, to keep `tempo_worklog_id` untouched. Makes hard: `tempo_remote.rs`, `collectors/tempo.rs`.
- In week sync, facing the one-connection lock, we chose a browser loop over the existing `POST /sync` per day and rejected a week route. Makes hard: `web/lib/weekSync.ts`.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|
| — | — | — |

## Contract for T001 — Schema v18
CONTRACT   none (SQL only).
CALLS      `ensure_jira_tickets_status_category(conn)` registered in `migrate()` next to `ensure_jira_tickets_external`; `SCHEMA_VERSION = 18`.
SQL        `ALTER TABLE jira_tickets ADD COLUMN status_category TEXT` (NULL = unknown).
           `CREATE TABLE IF NOT EXISTS tempo_remote_worklogs (tempo_worklog_id TEXT PRIMARY KEY, day TEXT NOT NULL, issue_id INTEGER NOT NULL, jira_issue TEXT, seconds INTEGER NOT NULL, description TEXT NOT NULL DEFAULT '', owner TEXT NOT NULL CHECK (owner IN ('worklog','outside')), pulled_at TEXT NOT NULL);` + `CREATE INDEX IF NOT EXISTS idx_tempo_remote_day_issue ON tempo_remote_worklogs(day, issue_id);`
           `CREATE TABLE IF NOT EXISTS tempo_required_days (day TEXT PRIMARY KEY, required_seconds INTEGER NOT NULL CHECK (required_seconds >= 0), pulled_at TEXT NOT NULL);`
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Jira status, transitions, comments
CONTRACT   `crate::tempo_hub_contract::{StatusCategory, Transition, HubError, COMMENT_MAX_CHARS}`.
CALLS      `pub fn list_transitions_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<Vec<Transition>>` — GET `/rest/api/3/issue/{key}/transitions`.
           `pub fn transition_with(auth: &JiraAuth, key: &str, transition_id: &str, client: &Client) -> Result<()>` — POST `…/transitions` `{"transition":{"id":…}}`, expect 204 (no body decode).
           `pub fn fetch_status_with(auth: &JiraAuth, key: &str, client: &Client) -> Result<(String, Option<StatusCategory>)>` — GET `/rest/api/3/issue/{key}?fields=status`.
           `pub fn add_comment_with(auth: &JiraAuth, key: &str, text: &str, client: &Client) -> Result<()>` — POST `…/comment`; body ADF: paragraphs split on blank lines, single newlines → `hardBreak`; 201 expected.
           Refresh: follow `nextPageToken` until absent (today it reads one page of 200); only after the last page succeeds call `mark_unreturned_done`. Request `status` already includes `statusCategory.key`; write it with `repo::set_ticket_status(conn, key, status: &str, category: Option<StatusCategory>)`, then `repo::mark_unreturned_done(conn, returned_keys: &[String]) -> Result<usize>` (only `external = 0` rows).
ERRORS     non-2xx → `HubError::Upstream{service:"Jira",status,body}` wrapped in anyhow (copy `create_issue_with`'s manual send).
THE FIVE   (1)–(5) as in T001.

## Contract for T003 — Tempo read client
CONTRACT   `crate::tempo_hub_contract::{PulledWorklog, RequiredDay, HubError, TEMPO_PAGE_LIMIT}`.
CALLS      `pub fn list_worklogs_with(auth: &TempoAuth, account_id: &str, from: NaiveDate, to: NaiveDate, client: &Client) -> Result<Vec<PulledWorklog>>` — GET `{base}/worklogs/user/{account_id}?from=&to=&limit=1000&offset=`; follow pages while `results.len() == TEMPO_PAGE_LIMIT`.
           `pub fn user_schedule_with(auth: &TempoAuth, account_id: &str, from: NaiveDate, to: NaiveDate, client: &Client) -> Result<Vec<RequiredDay>>` — GET `{base}/user-schedule/{account_id}?from=&to=`.
           `resolve_account_id` becomes `pub` (signature unchanged).
ERRORS     non-2xx → `HubError::Upstream{service:"Tempo",…}`.
THE FIVE   (1)–(5) as in T001.

## Contract for T004 — Store pulled week
CONTRACT   `crate::tempo_hub_contract::{PulledWorklog, RequiredDay, RemoteWorklog, WorklogOwner, PullReport}`.
CALLS      `pub fn store_week(conn: &Connection, monday: NaiveDate, worklogs: &[PulledWorklog], schedule: &[RequiredDay], pulled_at: &str) -> Result<PullReport>` — one transaction: delete both tables' rows with day in monday..=monday+6, insert; `owner = worklog` iff `EXISTS blocks WHERE tempo_worklog_id = id`; `jira_issue` from `jira_tickets.issue_id`.
           `pub fn list_week(conn: &Connection, monday: NaiveDate) -> Result<Vec<RemoteWorklog>>`.
           `pub fn outside_exists(conn: &Connection, day: &str, issue_id: i64) -> Result<bool>`.
THE FIVE   (1)–(5) as in T001.

## Contract for T005 — Sync guard
CALLS      in `sync_day_with_invoker`, for an `AllUnsynced` group, after `resolve_issue_id`: `if tempo_remote::outside_exists(conn, day, issue_id)?` → every block in the group gets `SyncResult` status `skipped`, reason exactly `already in Tempo — logged outside worklog`; no HTTP call. `SharedId` and `MixedLegacy` unchanged. `tempo_worklog_id` never written by this path.
THE FIVE   (1)–(5) as in T001.

## Contract for T006 — Task board query
CONTRACT   `crate::tempo_hub_contract::{TaskRow, TasksResponse, StatusCategory}`.
CALLS      `pub fn tasks(conn: &Connection, monday: NaiveDate, today: NaiveDate, jira_base_url: Option<&str>) -> Result<TasksResponse>` — assigned = `external = 0 AND COALESCE(status_category,'') != 'done'`; plus every other key with a ticket line Mon..Sun. Hours from `tempo_lines::lines_for_day` (`effective_seconds`). Order: assigned first, then `week_seconds` desc, then key. Summary falls back to the key when uncached.
THE FIVE   (1)–(5) as in T001.

## Contract for T007 — Week close-out query
CONTRACT   `crate::tempo_hub_contract::{WeekCloseout, CloseoutDay}`.
CALLS      `pub fn week_closeout(conn: &Connection, monday: NaiveDate) -> Result<WeekCloseout>` — 7 days always. `synced_seconds`/`pending_lines` per line from its blocks' `tempo_worklog_id`/`dirty` (same block filter as `lines_for_day`). `unticketed_seconds` = `SUM(duration_seconds)` of blocks with empty `jira_issue`, `is_personal = 0`, `ignored_at IS NULL`.
THE FIVE   (1)–(5) as in T001.

## Contract for T008 — AI draft
CONTRACT   `crate::tempo_hub_contract::{TicketDraft, Transition, COMMENT_MAX_CHARS}`; `crate::estimate::ModelInvoker`.
CALLS      `pub struct DraftPrep { pub key: String, pub summary: String, pub status: Option<String>, pub recent_lines: Vec<(String, String)> /* (day, text) newest first, ≤20, last 14 days */ }`
           `pub fn prepare_draft(conn: &Connection, key: &str, today: NaiveDate) -> Result<DraftPrep>` (line text = stored text, else `fallback_text`).
           `pub fn draft_with(invoker: &dyn ModelInvoker, prep: &DraftPrep, transitions: &[Transition], model: &str) -> Result<TicketDraft>` — writes nothing.
PROMPT     only key, summary, status, transition names, `recent_lines` — assert this in a test of the private prompt builder.
TESTS      `crate::estimate::FixedInvoker` (cfg(test)). Never log comment or draft text.
THE FIVE   (1)–(5) as in T001.

## Contract for T009 — Daemon task routes
CALLS      `GET /tasks?monday=` → `TasksResponse` (default monday = this week) · `GET /tickets/:key/transitions` → `Vec<Transition>` · `POST /tickets/:key/transition` `TransitionBody` → `TicketStatus` · `POST /tickets/:key/comment` `CommentBody` → `{"ok":true}` · `POST /tickets/:key/draft` → `TicketDraft`.
           Every Jira/AI call outside `with_conn` (read → call → write), as `estimate_block` does.
THE FIVE   (1)–(5) as in T001.

## Contract for T010 — Daemon week routes
CALLS      `POST /tempo/pull` `{"monday":"YYYY-MM-DD"}` → `PullReport` · `GET /weeks/:monday/closeout` → `WeekCloseout`. Pull: resolve account + both GETs in `spawn_blocking` with no lock, then one `with_conn(store_week)`.
THE FIVE   (1)–(5) as in T001.

## Contract for T011 — Web types, client, actions
NAMES      mirror every contract type into `web/lib/types.ts` with the same names and snake_case keys.
CALLS      `web/lib/daemonHub.ts`: `tasks(monday?)`, `transitions(key)`, `transition(key, transitionId)`, `comment(key, text)`, `draft(key)`, `pullTempo(monday)`, `closeout(monday)`. `web/app/actions-hub.ts`: same verbs as Server Actions returning `ActionResult<T>` (copy the local `run()` used in `actions-tempo-lines.ts`). Timeouts in `daemon.ts` `timeoutMs`: `/draft` 90 s, `/tempo/pull` 60 s, `/transitions` and `/transition` and `/comment` 20 s.
THE FIVE   (1)–(5) as in T001.
