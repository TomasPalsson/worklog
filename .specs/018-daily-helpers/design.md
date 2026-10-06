# Design — Daily helpers (018)

Only what two task agents must agree on. Spec says WHAT; this says the seams.

## 1. Contract file + language

- Rust: `rust/crates/worklog-core/src/daily_helpers_contract.rs` (declared in `lib.rs`, unit-tested).
- TS mirror: `web/lib/daily_helpers_contract.ts` (hand mirror, the repo's `*_contract.ts` pattern).
- Orchestrator owns both. Phase 6 (after spec 017) extends them; no task adds a shape.

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| block change | `BlockChange` | contract | action, edit_kind, op |
| undo result | `UndoOutcome` | contract | UndoResult, UndoStatus |
| checklist row | `PreflightRow` / `PreflightCheck` | contract | CheckRow, Gate, Check |
| standup draft | `StandupDraft` (`to_text()`) | contract | Standup, Report, Summary |
| post result | `PostOutcome` | contract | PostResult, SlackResult |
| nudge | `Nudge` / `NudgeKind` | contract | Hint, Notice, Reminder |
| billed hours | `effective_seconds` on `TempoLine` (already half-hour rounded server-side) | `tempo_line_contract.rs` | billed_seconds, rounded_seconds, billable |
| channel setting | `SLACK_DAILY_CHANNEL_KEY` | contract | DAILY_CHANNEL, slack_channel |

Times are UTC ISO-8601 in the DB; day bucketing uses `$WORKLOG_TZ` (existing rule). Durations are `i64` seconds.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure granularity |
|---|---|---|---|
| Tempo error body | bytes, may be unreadable | `tempo::error_body(resp) -> String` | per request; never empty |
| Slack search/post response | JSON with `ok:false` + `error` | `slack_post::parse_ok` | per call → `PostOutcome::SlackRefused` |
| GitHub review search | JSON | `nudges::parse_review_search` | per call → zero review nudges, logged to stderr |
| Model output (standup) | text | `standup::parse_draft` | whole draft → error shown with Retry |
| Daemon request bodies | JSON | serde on handler args | 400 via existing `ApiError` |
| Ask query text | free text | `ask::to_match_query` (escapes quotes/operators) | per query |

## 3. Error taxonomy

Outcomes (`UndoOutcome`, `PostOutcome`, `PreflightRow.ok=false`) are **data, HTTP 200**. Only internal failures use the existing `ApiError` (500 + `{error}`). CLI: outcome → exit 0 with message; `RefusedSynced`/`NothingToUndo` exit 1; internal error → miette error, exit 1.

## 4. Module boundaries

- New core modules (one file each, created as stubs by T001, declared in `lib.rs` once): `billing_round`, `undo`, `ask`, `report`, `standup`, `slack_post`, `nudges`, `preflight`, `daemon_undo`, `daemon_ask`, `daemon_standup`, `daemon_nudges`, `daemon_preflight`.
- `daemon_*` modules export `pub async fn` handlers only; **routes are registered in `daemon.rs` only by T012, T019, T024 and T032**.
- Feature modules may import `db`, `block_service`, `billing`, `block_digest`, `status_hints`, `ticket_activity`, `scrub`, `estimate::ModelInvoker`, `secrets`, `daily_helpers_contract`. Feature modules never import each other, except `preflight` → `billing_round`.
- CLI: subcommand bodies live in `worklog-cli/src/helpers_cmd.rs`; `cli.rs` only gets `Cmd` variants + dispatch lines, in the `CLI wiring` tasks.
- Web: one client file per feature (`web/lib/daemonUndo.ts`, `daemonStandup.ts`, `daemonPreflight.ts`); never grow `web/lib/daemon.ts` or `types.ts` (400-line guard).

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| SQLite conn | `db::open` (daemon state / CLI) | `&Connection` param | every core fn |
| HTTP client | caller (`reqwest::blocking::Client`) | param, so tests inject httpmock base URLs | slack_post, nudges, tempo |
| Slack token | `secrets::get("slack_user_token")` | param (`&str`) | slack_post |
| Daily channel | envfile `SLACK_DAILY_CHANNEL_KEY` | param (`Option<&str>`) | slack_post |
| Model | `estimate::ModelInvoker` impl | `&dyn ModelInvoker` | standup |
| Clock | caller passes `today: NaiveDate` / `now` | param | standup, nudges, undo |
| Schema | `sql/schema.sql` + `SCHEMA_VERSION` in `db.rs` | — | **only T006, T008 and T029 bump it, in that order** |

## 6. Deliberately duplicated

- The TS contract mirrors the Rust one by hand (repo pattern). Do not add codegen here.
- Overlap/percent helpers in `web/lib/dayStripOverlaps.ts` are UI slider maths: leave them.

## 7. Decisions

- In the context of undo, facing block writes spread over 9 `block_service` fns, we chose a before-image journal table `block_undo(id, change, payload_json, at)` written inside each fn's transaction, and rejected event-sourcing blocks, to keep one table and one restore fn, accepting that non-`block_service` writers (estimate, sync) are not undoable. Makes hard: `block_service.rs`, `undo.rs`.
- In the context of billed hours, facing `TempoLine.effective_seconds` already rounded server-side, we chose to delete the web's `roundToHalfHour` and render the server value, and rejected a new `billed_seconds` field, to avoid a second name for one number. Makes hard: `web/lib/format.ts`, `LineHours.tsx`, `TicketGroup.tsx`, `app/[day]/page.tsx`.
- In the context of ask, facing ≤1 s over 12 months, we chose an SQLite FTS5 table over digest clues + scrubbed prompts filled on write and backfilled once, rejecting LIKE scans, accepting extra DB size. Makes hard: `ask.rs`, `schema.sql`.

## Contract for T005 — Web reads billed hours
CONTRACT   web/lib/tempo_line_contract.ts — import `TempoLine`.
NAMES      billed hours = `effective_seconds` (banned: billed_seconds, rounded_seconds)
CALLS      formatBilledHours(seconds) becomes display-only: no rounding inside.
THE FIVE   (1) NEVER invent a field name that exists in the contract. (2) NEVER type a boundary parameter as the narrow type. (3) NEVER add a flag to a shared helper — duplicate locally. (4) NEVER edit outside `files:`. (5) NEVER abbreviate identifiers.

## Contract for T006 — Undo journal
CONTRACT   rust/crates/worklog-core/src/daily_helpers_contract.rs — `BlockChange`, `UndoOutcome`, `UNDO_DEPTH`.
CALLS      `pub fn undo::record(tx: &Transaction, change: BlockChange, before: &[i64]) -> Result<()>`; `pub fn undo::undo_last(conn: &mut Connection) -> Result<UndoOutcome>`
THE FIVE   as T005.

## Contract for T015 — Standup draft
CONTRACT   daily_helpers_contract.rs — `StandupDraft`, `STANDUP_QUESTIONS`.
CALLS      `pub fn standup::draft(conn: &Connection, today: NaiveDate, model: &dyn ModelInvoker) -> Result<StandupDraft>`; text comes only from `StandupDraft::to_text()`.
THE FIVE   as T005.

## Contract for T016 — Slack post
CONTRACT   daily_helpers_contract.rs — `PostOutcome`, `DAILY_THREAD_PREFIX`.
CALLS      `pub fn slack_post::post_to_daily(client: &Client, base_url: &str, token: &str, channel: Option<&str>, today: NaiveDate, text: &str) -> Result<PostOutcome>`
THE FIVE   as T005.

## Contract for T018 — Nudges
CONTRACT   daily_helpers_contract.rs — `Nudge`, `NudgeKind`, `STALE_TICKET_DAYS`, `NUDGE_CACHE_SECONDS`.
CALLS      `pub fn nudges::current(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<Nudge>>`
THE FIVE   as T005.

## Contract for T023 — Preflight
CONTRACT   daily_helpers_contract.rs — `PreflightRow`, `PreflightCheck`.
CALLS      `pub fn preflight::check(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Result<Vec<PreflightRow>>`; `pub fn preflight::read_back(conn: &Connection, day: NaiveDate) -> Result<PreflightRow>`
THE FIVE   as T005.
