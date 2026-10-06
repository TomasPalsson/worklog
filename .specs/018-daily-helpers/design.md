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
| Schema | `sql/schema.sql` (`CREATE … IF NOT EXISTS`, runs on every open) + `db.rs` `ensure_*` column adds | — | **no `SCHEMA_VERSION` bump in this spec** (Ruling 2026-10-06: a bump broke db_test pins). New tables go in schema.sql; new columns on existing tables use the `add_columns_if_missing` pattern of `ensure_tempo_line_texts_verdict_columns` (db.rs ~396) |

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

## Contract for T029 — Already-in-Tempo check (tempo_match)

Facts after spec 017 merged (paths under `rust/crates/worklog-core/`):
- Same-work judge: `verdict::match_texts(query: &str, texts: &[String]) -> Result<Vec<bool>>` (src/verdict.rs:149, POST /match). `Err` = Verdict off or unreachable. Wrap it as an injectable `F: Fn(&str, &[String]) -> Result<Vec<bool>>` like `line_check::check` (src/line_check.rs:13); tests pass a closure, never hit :9324.
- Tempo read: `tempo::list_worklogs_with(auth, account_id, from, to, client) -> Vec<PulledWorklog{tempo_worklog_id, day, issue_id: i64, seconds, description}>` (collectors/tempo.rs:1186); account via `tempo::resolve_account_id`; Jira key → issue id via `repo::get_ticket_issue_id` (TEXT, parse to i64). Use `http::client_with_timeout(Duration::from_secs(TEMPO_READ_TIMEOUT_SECONDS))` for the read (§5: ≤10 s, then FR-09).
- Status column: add `match_status TEXT` (NULL or `ALREADY_IN_TEMPO`), `match_tempo_id TEXT` and `match_basis TEXT` to `tempo_line_texts` (PK day, jira_issue) via a new `ensure_tempo_line_texts_match_columns` in db.rs called from `migrate`. No SCHEMA_VERSION bump.
- API (src/tempo_match.rs): `pub fn check_line(conn, line: &TempoLine, existing: &[PulledWorklog], matcher: F) -> MatchVerdict` — same work = matcher says true for an entry on the same issue that day AND |entry.seconds − line.effective_seconds| ≤ SAME_HOURS_TOLERANCE_SECONDS (30 min counts as same, 31 does not). Only entries NOT owned by worklog (no block carries their id) are candidates. Matcher Err / empty / abstain → `Unchecked{reason}`. `pub fn mark_already(conn, line: &TempoLine, tempo_id)` stores match_status, match_tempo_id and `match_basis` = a fingerprint of the line's text and effective_seconds; `pub fn is_already(conn, line: &TempoLine) -> bool` is true only while the stored basis equals the line's current fingerprint — FR-10: an Owner edit of the text or hours changes the fingerprint, so the next sync re-checks with no edit hooks.
- T029 does NOT touch the send path (T030 does).

## Contract for T030 — Send path asks tempo_match first

- Insert the check in `collectors/tempo.rs` inside `sync_group_aggregated` right after the `total_seconds == 0` skip precedent (~tempo.rs:430-445) and before the payload build: only for groups with no existing tempo id (a first send, AllUnsynced). If `tempo_match::is_already(conn, &line)` → push `SyncResult{status:"skipped", reason:Some("already in Tempo")}` per block, no read, no write (FR-10). Else read + `check_line`; `AlreadyInTempo` → mark_already + same skipped results, no write (FR-07); `Different`/`Unchecked` → write as today (FR-08/09).
- The read and Verdict calls must be injectable for tests (httpmock Tempo; closure matcher). Keep `sync_day_with` signatures working for existing callers; thread an optional matcher through `sync_day_with_invoker_for` or a new `_with_matcher` variant.
- FR-16: auto-send skips the pre-send checklist — it already does (the checklist is only in the hand-send UI/CLI); assert no preflight call on the auto-send path.
- The old comment at tempo.rs:~221 ("hand-logged entries are extra work… never block it") is superseded by FR-07; update it.

## Contract for T031 — Recap built inside the 17:00 run

- `auto_send::run_if_due(conn, local, enabled, send)` (src/auto_send.rs:122) returns `()` and stores outcomes on `tempo_line_texts` (auto_sent_at, send_error). After its per-line loop (before the final `Ok(())`, ~L175) call `recap::build_and_store(conn, today, work_hours)` guarded once per day by a meta key (`purge::meta_get/meta_set`, e.g. `recap_built_for`=day), so later 15-min ticks do not rebuild. No new timer (FR-45).
- `recap::build(conn, day, window) -> Recap`: sent = today's lines with auto_sent_at set; held_back = `ready_lines`-rejected or planned-but-failed lines with a plain reason (send_error text, "already in Tempo", "text needs a look", "ticket not confirmed", "no text"); coverage = block seconds (non-ignored) inside the work-hours window ÷ window seconds, rounded to 0..=100; gaps = `timeline::day_gaps` with min `RECAP_MIN_GAP_SECONDS`, clipped to the window, minus recorded breaks, longest first, top `RECAP_TOP_GAPS`.
- Work-hours window: `browser_ingest::WorkHours::parse` has no public start/end accessor — add a small public accessor there or parse `HH:MM-HH:MM` from `routing_contract::WORK_HOURS_KEY` (default `DEFAULT_WORK_HOURS`). Day offset `tz::day_offset()`.
- Store: `recaps(day TEXT PRIMARY KEY, json TEXT NOT NULL, built_at TEXT NOT NULL)` and `recap_breaks(day TEXT, started_at TEXT, ended_at TEXT, PRIMARY KEY(day, started_at))` in schema.sql. `recap::latest(conn) -> Option<Recap>`, `recap::apply_gap(conn, day, gap_started_at, action: GapAction)`: Personal → insert a block over the gap with is_personal=1, estimated_by='manual'; Break → insert into recap_breaks; PickTicket → `ticket_log::log_time` (manual, that key) over the gap. Each resolves the gap (rebuild + store the recap); only PickTicket adds billable time (FR-44).

## Contract for T032 — Recap route and Day banner

- Daemon: `GET /recap` → `Option<Recap>` (latest, today's), `POST /recap/gap {day, started_at, action: GapAction}` → updated `Recap`. Register next to the auto-send/review routes in daemon.rs.
- Web: `web/components/RecapBanner.tsx` on today's Day page (`web/app/[day]/page.tsx`, near 017's ReviewSection): sent count + hours, held-back lines with reasons, coverage %, up to 3 gaps each with Personal / Break / Pick a ticket buttons (ticket picker: reuse the existing TicketCombobox pattern or a simple key input). Use existing tokens and classes (see the spec 018 block at the end of globals.css). Built with the /design:design skill (Owner's UI rule) — the orchestrator runs a design pass after.

## Contract for T033 — Recap in the Claude Code footer

- `mods/worklog/hooks/status.ts`: after 17:00 when `GET /recap` returns today's recap, the footer tail shows one line, e.g. `recap: 6 sent · 1 held · 82% · 3 gaps` (instead of a nudge while a recap exists and has unresolved gaps). Tests run with `claude plugin test mods/worklog`.
